import type { AuthApi, AuthApiResponse, AuthPath } from "./api";
import { API_SESSION_COOKIE } from "./cookies";
import {
  type Answer,
  CLEAR_BOTH,
  apiError,
  boundedText,
  browserCookies,
  forwardedFor,
  isSameOrigin,
  respond,
} from "./handlers";

// SPEC-026 §Operational §9: each mutation of the dashboard is a route handler on its origin
// that calls the api server-side, as the login and the logout are (SPEC-009's second
// Amendment 2026-10-15): the same-origin check, a 16 KiB body, the CSRF cookie sent as
// `x-csrf-token`, `X-Forwarded-For` passed through and `Cache-Control: no-store`. The
// invitation's two handlers send neither the session nor the CSRF token. Nothing here logs
// a body, a credential or a token.

/** A mutation's body is read up to this many bytes. */
export const MUTATION_BODY_LIMIT = 16 * 1024;

export interface MutationRoute {
  /** The api's path; `id` is the route's `[id]`, already checked as a UUID. */
  readonly path: (id: string) => AuthPath;
  /** Whether the dashboard's route has an `[id]`. */
  readonly hasId: boolean;
  /** The body's fields, each a string, and no other; none for `{}`. */
  readonly fields: readonly string[];
  /** Whether the call carries the session and its CSRF token. */
  readonly session: boolean;
  /** The api's `200` ended every session of the user: both cookies are cleared. */
  readonly clearsCookies?: boolean;
}

/** The dashboard's mutation routes (SPEC-026 §Data contracts §3), by the handler's path
 *  under `app/api/`. */
export const MUTATIONS = {
  accountPassword: {
    path: () => "/v1/auth/password",
    hasId: false,
    fields: ["currentPassword", "newPassword", "totp_code"],
    session: true,
    clearsCookies: true,
  },
  createUser: { path: () => "/v1/users", hasId: false, fields: ["email", "role"], session: true },
  userRole: { path: (id) => `/v1/users/${id}/role`, hasId: true, fields: ["role"], session: true },
  userStatus: {
    path: (id) => `/v1/users/${id}/status`,
    hasId: true,
    fields: ["status"],
    session: true,
  },
  userInvitation: {
    path: (id) => `/v1/users/${id}/invitation`,
    hasId: true,
    fields: [],
    session: true,
  },
  userInvitationWithdraw: {
    path: (id) => `/v1/users/${id}/invitation/withdraw`,
    hasId: true,
    fields: [],
    session: true,
  },
  userReset: { path: (id) => `/v1/users/${id}/reset`, hasId: true, fields: [], session: true },
  invitationTotp: {
    path: () => "/v1/invitations/totp",
    hasId: false,
    fields: ["token"],
    session: false,
  },
  invitationComplete: {
    path: () => "/v1/invitations/complete",
    hasId: false,
    fields: ["token", "password", "totp_code"],
    session: false,
  },
} as const satisfies Record<string, MutationRoute>;

/** An `[id]` as the api writes ids. Any other text, a `/` among them, reaches no api path. */
const UUID = /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;

const FORBIDDEN: Answer = { status: 403, body: { error: "forbidden" } };
const UNAUTHORIZED: Answer = { status: 401, body: { error: "unauthorized" } };
const NOT_FOUND: Answer = { status: 404, body: { error: "not_found" } };
const BAD_REQUEST: Answer = { status: 400, body: { error: "bad_request" } };
const UNAVAILABLE: Answer = { status: 503, body: { error: "unavailable" } };
const UPSTREAM: Answer = { status: 502, body: { error: "upstream" } };

/** The api's refusals, passed on with its `error` code alone. */
const PASSED_THROUGH: Record<number, string> = {
  400: "bad_request",
  401: "unauthorized",
  403: "forbidden",
  404: "not_found",
  409: "conflict",
  429: "rate_limited",
  503: "unavailable",
};

/** `application/json` (any parameters), up to the limit, an object with exactly the route's
 *  fields, each a string. The body sent on holds those fields alone, in the route's order. */
async function readBody(req: Request, fields: readonly string[]): Promise<string | null> {
  const mediaType = (req.headers.get("content-type") ?? "").split(";")[0]?.trim().toLowerCase();
  if (mediaType !== "application/json") return null;
  let text: string | null;
  try {
    text = await boundedText(req, MUTATION_BODY_LIMIT);
  } catch {
    // The client stopped sending its body.
    return null;
  }
  if (text === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return null;
  const record = parsed as Record<string, unknown>;
  if (Object.keys(record).sort().join(",") !== [...fields].sort().join(",")) return null;
  const out: Record<string, string> = {};
  for (const field of fields) {
    const value = record[field];
    if (typeof value !== "string") return null;
    out[field] = value;
  }
  return JSON.stringify(out);
}

/**
 * A mutation's handler (SPEC-026 §Operational §9). In order: the same-origin check (`403`,
 * before the body or a cookie is read); for a route with a session, the session and CSRF
 * cookies (`401` without both); the `[id]` (`404` unless a UUID); the body (`400`). Then the
 * api's route, with `Cookie: cgsess=<token>` and `x-csrf-token` for a route with a session,
 * neither for the invitation's, and the request's `X-Forwarded-For`. Its `200` and `201` are
 * answered with its body, its `204` with none, its refusals with their status and `error`
 * code; any other answer, or a `200` or `201` that is not JSON, `502`; no answer, `503`.
 */
export async function handleMutation(
  req: Request,
  api: AuthApi,
  route: MutationRoute,
  id?: string,
): Promise<Response> {
  if (!isSameOrigin(req)) return respond(FORBIDDEN);

  const headers: Record<string, string> = {
    "content-type": "application/json",
    ...forwardedFor(req),
  };
  if (route.session) {
    const { session, csrf } = browserCookies(req);
    if (!session || !csrf) return respond(UNAUTHORIZED);
    headers.cookie = `${API_SESSION_COOKIE}=${session}`;
    headers["x-csrf-token"] = csrf;
  }

  let path: AuthPath;
  if (route.hasId) {
    if (id === undefined || !UUID.test(id)) return respond(NOT_FOUND);
    path = route.path(id);
  } else {
    path = route.path("");
  }

  const body = await readBody(req, route.fields);
  if (body === null) return respond(BAD_REQUEST);

  let res: AuthApiResponse;
  try {
    res = await api.post(path, headers, body);
  } catch {
    return respond(UNAVAILABLE);
  }

  if (res.status === 204) {
    return new Response(null, { status: 204, headers: { "cache-control": "no-store" } });
  }
  if (res.status === 200 || res.status === 201) {
    const answered = res.body;
    if (typeof answered !== "object" || answered === null || Array.isArray(answered)) {
      return respond(UPSTREAM);
    }
    const cookies = route.clearsCookies && res.status === 200 ? CLEAR_BOTH : [];
    return respond({ status: res.status, body: answered as Record<string, unknown>, cookies });
  }
  const fallback = PASSED_THROUGH[res.status];
  if (fallback !== undefined) {
    return respond({ status: res.status, body: { error: apiError(res, fallback) } });
  }
  return respond(UPSTREAM);
}
