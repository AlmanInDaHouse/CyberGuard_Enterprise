import { RequestCookies } from "next/dist/server/web/spec-extension/cookies";
import type { AuthApi, AuthApiResponse } from "./api";
import {
  API_SESSION_COOKIE,
  CSRF_COOKIE,
  SESSION_COOKIE,
  TOKEN,
  clearCookie,
  setCookie,
  tokenShaped,
} from "./cookies";

// SPEC-009's second Amendment 2026-10-15, the dashboard's login and logout: two route
// handlers on the dashboard's origin that call the api server-side. Nothing here logs a
// body, a credential or a token.

/** The login body is read up to this many bytes (item 3). */
export const LOGIN_BODY_LIMIT = 16 * 1024;

type Answer = { status: number; body: Record<string, unknown>; cookies?: string[] };

function respond({ status, body, cookies = [] }: Answer): Response {
  const headers = new Headers({ "content-type": "application/json", "cache-control": "no-store" });
  for (const c of cookies) headers.append("set-cookie", c);
  return new Response(JSON.stringify(body), { status, headers });
}

const CLEAR_BOTH = [clearCookie(SESSION_COOKIE), clearCookie(CSRF_COOKIE)];

/** The host addressed, with port 80 or 443 removed: behind a proxy that ends TLS its
 *  scheme may be unknown (item 2). */
function addressedHost(host: string): string {
  return host
    .trim()
    .toLowerCase()
    .replace(/:(80|443)$/, "");
}

/**
 * Item 2: the request carries an `Origin` whose host is the host it was addressed to —
 * the first entry of `X-Forwarded-Host`, or else, when that entry is empty or the header
 * absent, `Host`, as Next's Server Actions check reads them — and, when it carries
 * `Sec-Fetch-Site`, that header is `same-origin`. The `Origin`'s host is read as its URL
 * gives it, without its own scheme's default port. The scheme is not compared.
 */
export function isSameOrigin(req: Request): boolean {
  const site = req.headers.get("sec-fetch-site");
  if (site !== null && site !== "same-origin") return false;
  const origin = req.headers.get("origin");
  if (origin === null) return false;
  let originHost: string;
  try {
    originHost = new URL(origin).host.toLowerCase();
  } catch {
    return false;
  }
  const forwarded = req.headers.get("x-forwarded-host")?.split(",")[0]?.trim();
  const addressed = addressedHost(forwarded ? forwarded : (req.headers.get("host") ?? ""));
  return addressed !== "" && originHost === addressed;
}

const FORBIDDEN: Answer = { status: 403, body: { error: "forbidden" } };
const BAD_REQUEST: Answer = { status: 400, body: { error: "bad_request" } };
const UNAVAILABLE: Answer = { status: 503, body: { error: "unavailable" } };
const UPSTREAM: Answer = { status: 502, body: { error: "upstream" } };

/** The request's `X-Forwarded-For`, passed on as received; none when it carries none
 *  (item 5). */
function forwardedFor(req: Request): Record<string, string> {
  const xff = req.headers.get("x-forwarded-for");
  return xff === null ? {} : { "x-forwarded-for": xff };
}

/** The request's body as text, or `null` past the limit or when it is not UTF-8. */
async function boundedText(req: Request, limit: number): Promise<string | null> {
  if (req.body === null) return "";
  const reader = req.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > limit) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

/** The browser's session and CSRF cookies, read as the pages read the session (Next's
 *  `cookies()` is built on `RequestCookies`), so a repeated cookie means the same
 *  session to both. */
function browserCookies(req: Request): { session?: string; csrf?: string } {
  const jar = new RequestCookies(req.headers);
  return {
    session: tokenShaped(jar.get(SESSION_COOKIE)?.value),
    csrf: tokenShaped(jar.get(CSRF_COOKIE)?.value),
  };
}

type Credentials = { email: string; password: string; totp_code: string };

/** Item 3: `application/json` (any parameters), up to the limit, an object with exactly
 *  the three string fields. */
async function readCredentials(req: Request): Promise<Credentials | null> {
  const mediaType = (req.headers.get("content-type") ?? "").split(";")[0]?.trim().toLowerCase();
  if (mediaType !== "application/json") return null;
  let text: string | null;
  try {
    text = await boundedText(req, LOGIN_BODY_LIMIT);
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
  const keys = Object.keys(parsed).sort();
  if (keys.join(",") !== "email,password,totp_code") return null;
  const { email, password, totp_code } = parsed as Record<string, unknown>;
  if (typeof email !== "string" || typeof password !== "string" || typeof totp_code !== "string") {
    return null;
  }
  return { email, password, totp_code };
}

/** The session token in the api's `Set-Cookie`, and the CSRF token in its body. */
function issuedTokens(res: AuthApiResponse): { session: string; csrf: string } | null {
  let session: string | undefined;
  for (const c of res.setCookie) {
    const pair = c.split(";")[0] ?? "";
    const eq = pair.indexOf("=");
    if (eq !== -1 && pair.slice(0, eq).trim() === API_SESSION_COOKIE) {
      session = pair.slice(eq + 1).trim();
      break;
    }
  }
  const csrf = (res.body as { csrf_token?: unknown } | undefined)?.csrf_token;
  if (session === undefined || !TOKEN.test(session)) return null;
  if (typeof csrf !== "string" || !TOKEN.test(csrf)) return null;
  return { session, csrf };
}

/** The api's `error` code for an answer the handler passes through. */
function apiError(res: AuthApiResponse, fallback: string): string {
  const error = (res.body as { error?: unknown } | undefined)?.error;
  return typeof error === "string" ? error : fallback;
}

const PASSED_THROUGH: Record<number, string> = {
  400: "bad_request",
  401: "unauthorized",
  429: "rate_limited",
  503: "unavailable",
};

/** Revoke a session at the api (item 4's call). Resolves with the api's answer, or
 *  `null` when the api did not answer. */
async function revoke(
  api: AuthApi,
  req: Request,
  session: string,
  csrf: string,
): Promise<AuthApiResponse | null> {
  try {
    return await api.post("/v1/auth/logout", {
      cookie: `${API_SESSION_COOKIE}=${session}`,
      "x-csrf-token": csrf,
      ...forwardedFor(req),
    });
  } catch {
    return null;
  }
}

/** `POST /api/auth/login` (item 3). */
export async function handleLogin(req: Request, api: AuthApi): Promise<Response> {
  if (!isSameOrigin(req)) return respond(FORBIDDEN);
  const credentials = await readCredentials(req);
  if (credentials === null) return respond(BAD_REQUEST);

  // The api's login deletes no prior session (debt #102): revoke the browser's first,
  // whatever that revocation's outcome.
  const { session: priorSession, csrf: priorCsrf } = browserCookies(req);
  const revokedFirst = Boolean(priorSession && priorCsrf);
  if (priorSession && priorCsrf) await revoke(api, req, priorSession, priorCsrf);
  const onFailure = revokedFirst ? CLEAR_BOTH : [];

  let res: AuthApiResponse;
  try {
    res = await api.post(
      "/v1/auth/login",
      { "content-type": "application/json", ...forwardedFor(req) },
      JSON.stringify(credentials),
    );
  } catch {
    return respond({ ...UNAVAILABLE, cookies: onFailure });
  }
  if (res.status === 200) {
    const tokens = issuedTokens(res);
    if (tokens === null) return respond({ ...UPSTREAM, cookies: onFailure });
    return respond({
      status: 200,
      body: { ok: true },
      cookies: [setCookie(SESSION_COOKIE, tokens.session), setCookie(CSRF_COOKIE, tokens.csrf)],
    });
  }
  const fallback = PASSED_THROUGH[res.status];
  if (fallback !== undefined) {
    return respond({
      status: res.status,
      body: { error: apiError(res, fallback) },
      cookies: onFailure,
    });
  }
  return respond({ ...UPSTREAM, cookies: onFailure });
}

/** `POST /api/auth/logout` (item 4). */
export async function handleLogout(req: Request, api: AuthApi): Promise<Response> {
  if (!isSameOrigin(req)) return respond(FORBIDDEN);
  const { session, csrf } = browserCookies(req);
  if (!session) return respond({ status: 200, body: { ok: true }, cookies: CLEAR_BOTH });
  if (!csrf) return respond({ ...UNAVAILABLE, cookies: CLEAR_BOTH });

  const res = await revoke(api, req, session, csrf);
  if (res === null || res.status === 503) return respond({ ...UNAVAILABLE, cookies: CLEAR_BOTH });
  if (res.status === 200 || res.status === 401) {
    return respond({ status: 200, body: { ok: true }, cookies: CLEAR_BOTH });
  }
  // Any other answer, the api's 403 included (a CSRF token that does not match the
  // session, which a dashboard that sets both cookies together does not expect).
  return respond({ ...UPSTREAM, cookies: CLEAR_BOTH });
}
