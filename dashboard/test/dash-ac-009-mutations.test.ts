// @vitest-environment node
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, test, vi } from "vitest";
import { POST as accountPassword } from "../app/api/account/password/route";
import { POST as invitationComplete } from "../app/api/invitations/complete/route";
import { POST as invitationTotp } from "../app/api/invitations/totp/route";
import { POST as userInvitation } from "../app/api/users/[id]/invitation/route";
import { POST as userInvitationWithdraw } from "../app/api/users/[id]/invitation/withdraw/route";
import { POST as userReset } from "../app/api/users/[id]/reset/route";
import { POST as userRole } from "../app/api/users/[id]/role/route";
import { POST as userStatus } from "../app/api/users/[id]/status/route";
import { POST as createUser } from "../app/api/users/route";
import type { AuthApi } from "../src/lib/auth/api";
import { MUTATIONS, MUTATION_BODY_LIMIT, handleMutation } from "../src/lib/auth/mutations";

// dash_ac_009 (SPEC-026 §Acceptance criteria): each mutation handler refuses a cross-site
// request (the same-origin check), forwards the CSRF cookie as `x-csrf-token` and
// `X-Forwarded-For`, and answers `no-store`; the invitation's handlers send neither session
// nor CSRF token. The handlers are the route modules under `app/api/`, which call the api
// with `fetch` at `API_BASE_URL`; `fetch` records each call. The last block runs the handlers
// against the real api in-process, as dash_ac_003 does.

const ORIGIN = "https://soc.example";
const SAME_ORIGIN = { origin: ORIGIN, host: "soc.example", "sec-fetch-site": "same-origin" };
const COOKIES = "theme=dark; __Host-cgsess=tok-009; __Host-cgcsrf=csrf-009";
const ID = "3f2b8c1e-5a4d-4e6f-9a7b-1c2d3e4f5a6b";

type Handler = (req: Request, ctx: { params: Promise<{ id: string }> }) => Promise<Response>;

interface Case {
  name: string;
  handler: Handler;
  api: string;
  body: Record<string, string>;
  hasId: boolean;
  session: boolean;
}

const CASES: Case[] = [
  {
    name: "account/password",
    handler: accountPassword as Handler,
    api: "/v1/auth/password",
    body: { currentPassword: "old one 1", newPassword: "a new one 12", totp_code: "123456" },
    hasId: false,
    session: true,
  },
  {
    name: "users",
    handler: createUser as Handler,
    api: "/v1/users",
    body: { email: "new@dash009.test", role: "analyst" },
    hasId: false,
    session: true,
  },
  {
    name: "users/[id]/role",
    handler: userRole,
    api: `/v1/users/${ID}/role`,
    body: { role: "viewer" },
    hasId: true,
    session: true,
  },
  {
    name: "users/[id]/status",
    handler: userStatus,
    api: `/v1/users/${ID}/status`,
    body: { status: "disabled" },
    hasId: true,
    session: true,
  },
  {
    name: "users/[id]/invitation",
    handler: userInvitation,
    api: `/v1/users/${ID}/invitation`,
    body: {},
    hasId: true,
    session: true,
  },
  {
    name: "users/[id]/invitation/withdraw",
    handler: userInvitationWithdraw,
    api: `/v1/users/${ID}/invitation/withdraw`,
    body: {},
    hasId: true,
    session: true,
  },
  {
    name: "users/[id]/reset",
    handler: userReset,
    api: `/v1/users/${ID}/reset`,
    body: {},
    hasId: true,
    session: true,
  },
  {
    name: "invitations/totp",
    handler: invitationTotp as Handler,
    api: "/v1/invitations/totp",
    body: { token: "aW52aXRhdGlvbg" },
    hasId: false,
    session: false,
  },
  {
    name: "invitations/complete",
    handler: invitationComplete as Handler,
    api: "/v1/invitations/complete",
    body: { token: "aW52aXRhdGlvbg", password: "a new one 12", totp_code: "123456" },
    hasId: false,
    session: false,
  },
];

const fetchMock = vi.fn(async (_url: URL | string, _init?: RequestInit) => new Response(null));

function answer(status: number, body?: unknown) {
  fetchMock.mockResolvedValue(
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: body === undefined ? {} : { "content-type": "application/json" },
    }),
  );
}

function call(c: Case, headers: Record<string, string> = {}, body?: string, id = ID) {
  const req = new Request(`${ORIGIN}/api/${c.name.replace("[id]", id)}`, {
    method: "POST",
    headers: {
      ...SAME_ORIGIN,
      "content-type": "application/json",
      cookie: COOKIES,
      ...headers,
    },
    body: body ?? JSON.stringify(c.body),
  });
  return c.handler(req, { params: Promise.resolve({ id }) });
}

/** A body's JSON followed by spaces, `size` bytes in all: JSON that only its size refuses. */
const padded = (body: Record<string, string>, size: number) => {
  const text = JSON.stringify(body);
  return text + " ".repeat(size - text.length);
};

const sent = () => {
  const [url, init] = fetchMock.mock.calls[0] ?? [];
  return { url: String(url), init, headers: new Headers(init?.headers) };
};

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
  vi.stubEnv("API_BASE_URL", "http://api.internal:8081");
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe.each(CASES)("dash_ac_009 — POST /api/$name", (c) => {
  test.each([
    ["an Origin of another host", { origin: "https://evil.example" }],
    ["no Origin", { origin: "" }],
    ["Sec-Fetch-Site cross-site", { "sec-fetch-site": "cross-site" }],
    ["Sec-Fetch-Site same-site", { "sec-fetch-site": "same-site" }],
  ])("%s answers 403 without calling the api", async (_n, headers) => {
    const h: Record<string, string> = { ...headers };
    const res = await call(c, h);
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: "forbidden" });
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(res.headers.getSetCookie()).toEqual([]);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("calls the api's route with exactly the body's fields and the client's address", async () => {
    answer(200, { ok: true });
    const res = await call(c, { "x-forwarded-for": "203.0.113.7, 10.0.0.2" });
    expect(res.headers.get("cache-control")).toBe("no-store");
    const { url, init, headers } = sent();
    expect(url).toBe(`http://api.internal:8081${c.api}`);
    expect(init?.method).toBe("POST");
    expect(headers.get("content-type")).toBe("application/json");
    expect(headers.get("x-forwarded-for")).toBe("203.0.113.7, 10.0.0.2");
    expect(JSON.parse(String(init?.body))).toEqual(c.body);
  });

  test("sends no X-Forwarded-For when the request carries none", async () => {
    answer(200, { ok: true });
    await call(c);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(sent().headers.has("x-forwarded-for")).toBe(false);
  });

  if (c.session) {
    test("sends the session as cgsess and the CSRF cookie as x-csrf-token", async () => {
      answer(200, { ok: true });
      await call(c);
      const { headers } = sent();
      expect(headers.get("cookie")).toBe("cgsess=tok-009");
      expect(headers.get("x-csrf-token")).toBe("csrf-009");
    });

    test.each([
      ["no session", "__Host-cgcsrf=csrf-009"],
      ["no CSRF cookie", "__Host-cgsess=tok-009"],
      ["a session that is not a token", "__Host-cgsess=a%0Ab; __Host-cgcsrf=csrf-009"],
    ])("%s answers 401 without calling the api", async (_n, cookie) => {
      const res = await call(c, { cookie });
      expect(res.status).toBe(401);
      expect(await res.json()).toEqual({ error: "unauthorized" });
      expect(res.headers.get("cache-control")).toBe("no-store");
      expect(fetchMock).not.toHaveBeenCalled();
    });
  } else {
    test("sends neither the session nor a CSRF token, though the browser holds both", async () => {
      answer(200, { ok: true });
      await call(c);
      expect(fetchMock).toHaveBeenCalledTimes(1);
      const { headers } = sent();
      expect(headers.has("cookie")).toBe(false);
      expect(headers.has("x-csrf-token")).toBe(false);
    });

    test("needs no session", async () => {
      answer(200, { ok: true });
      const res = await call(c, { cookie: "" });
      expect(res.status).toBe(200);
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });
  }

  if (c.hasId) {
    test.each([
      ["not a UUID", "not-a-uuid"],
      ["a path", "../../auth/logout"],
      ["a UUID and a path", `${ID}/x`],
    ])("an id that is %s answers 404 without calling the api", async (_n, id) => {
      const res = await call(c, {}, undefined, id);
      expect(res.status).toBe(404);
      expect(await res.json()).toEqual({ error: "not_found" });
      expect(res.headers.get("cache-control")).toBe("no-store");
      expect(fetchMock).not.toHaveBeenCalled();
    });
  }

  const extra = JSON.stringify({ ...c.body, extra: "x" });
  const firstKey = Object.keys(c.body)[0];
  const badType = firstKey ? JSON.stringify({ ...c.body, [firstKey]: 1 }) : "[]";
  const missing = firstKey
    ? JSON.stringify(Object.fromEntries(Object.entries(c.body).slice(1)))
    : "null";
  test.each([
    ["an extra field", extra, "application/json"],
    ["a field that is not a string, or a body that is not an object", badType, "application/json"],
    ["a missing field, or a body that is not an object", missing, "application/json"],
    ["text that is not JSON", "{", "application/json"],
    ["another media type", JSON.stringify(c.body), "text/plain"],
    ["a body past 16 KiB", padded(c.body, MUTATION_BODY_LIMIT + 1)],
  ])("%s answers 400 without calling the api", async (_n, body, type = "application/json") => {
    const res = await call(c, { "content-type": type }, body);
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: "bad_request" });
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("a body of exactly 16 KiB, with a charset, is read", async () => {
    answer(200, { ok: true });
    const res = await call(
      c,
      { "content-type": "application/json; charset=utf-8" },
      padded(c.body, MUTATION_BODY_LIMIT),
    );
    expect(res.status).toBe(200);
    expect(JSON.parse(String(sent().init?.body))).toEqual(c.body);
  });

  test("passes the api's answer and its error codes through", async () => {
    answer(201, { token: "t", expires_at: "2026-11-25T00:00:00.000Z" });
    let res = await call(c);
    expect(res.status).toBe(201);
    expect(await res.json()).toEqual({ token: "t", expires_at: "2026-11-25T00:00:00.000Z" });
    expect(res.headers.get("cache-control")).toBe("no-store");

    answer(204);
    res = await call(c);
    expect(res.status).toBe(204);
    expect(await res.text()).toBe("");
    expect(res.headers.get("cache-control")).toBe("no-store");

    for (const [status, error] of [
      [400, "bad_request"],
      [401, "invitation_rejected"],
      [403, "forbidden"],
      [404, "not_found"],
      [409, "user_has_credentials"],
      [429, "rate_limited"],
      [503, "unavailable"],
    ] as const) {
      answer(status, { error, detail: "not passed on" });
      res = await call(c);
      expect(res.status).toBe(status);
      expect(await res.json()).toEqual({ error });
      expect(res.headers.get("cache-control")).toBe("no-store");
    }

    answer(500, { error: "internal" });
    res = await call(c);
    expect(res.status).toBe(502);
    expect(await res.json()).toEqual({ error: "upstream" });
    expect(res.headers.get("cache-control")).toBe("no-store");

    answer(200);
    res = await call(c);
    expect(res.status).toBe(502);
    expect(await res.json()).toEqual({ error: "upstream" });
    expect(res.headers.get("cache-control")).toBe("no-store");

    fetchMock.mockRejectedValue(new TypeError("fetch failed"));
    res = await call(c);
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ error: "unavailable" });
    expect(res.headers.get("cache-control")).toBe("no-store");
  });
});

describe("dash_ac_009 — the password change", () => {
  const password = CASES[0] as Case;

  test("clears both cookies once the api has ended every session", async () => {
    answer(200, { ok: true });
    const res = await call(password);
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("no-store");
    const cleared = res.headers.getSetCookie();
    expect(cleared).toContain(
      "__Host-cgsess=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0",
    );
    expect(cleared).toContain(
      "__Host-cgcsrf=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0",
    );
  });

  test("keeps them when the api refuses the change", async () => {
    answer(401, { error: "unauthorized" });
    const res = await call(password);
    expect(res.status).toBe(401);
    expect(res.headers.getSetCookie()).toEqual([]);
  });
});

test("no other mutation clears a cookie", async () => {
  for (const c of CASES.slice(1)) {
    answer(200, { ok: true });
    const res = await call(c);
    expect(res.headers.getSetCookie()).toEqual([]);
  }
});

// Against the real api in-process: an admin creates a user through the handler, the user's
// handlers ask for a secret and complete the invitation, the user signs in and changes their
// password, after which their session is gone (SC-F3-001).
describe("dash_ac_009 — the handlers against the api", () => {
  const loadApi = (specifier: string) => import(specifier);
  let stop: (() => Promise<void>) | undefined;
  let close: (() => Promise<void>) | undefined;
  let api: AuthApi;
  let read: (cookie: string) => Promise<number>;
  let config: unknown;
  let helpers: {
    insertUserDirect: (config: unknown, opts: Record<string, unknown>) => Promise<string>;
    currentTotp: () => string;
    totpAt: (secret: string, steps?: number) => string;
    DEFAULT_PASSWORD: string;
  };

  beforeAll(async () => {
    const { startBackends } = await loadApi("@cyberguard/api/test/helpers/backends.js");
    const { buildServices } = await loadApi("@cyberguard/api/src/services.js");
    const { buildApp } = await loadApi("@cyberguard/api/src/app.js");
    helpers = await loadApi("@cyberguard/api/test/helpers/db.js");
    const backends = await startBackends();
    stop = backends.stop;
    config = backends.config;
    const services = await buildServices(config);
    const app = buildApp(services);
    await app.ready();
    close = async () => {
      await app.close();
      await services.close();
    };
    api = {
      async post(path, headers, body) {
        const res = await app.inject({ method: "POST", url: path, headers, payload: body });
        const sc = res.headers["set-cookie"];
        let parsed: unknown;
        try {
          parsed = res.json();
        } catch {
          parsed = undefined;
        }
        return {
          status: res.statusCode,
          setCookie: sc === undefined ? [] : Array.isArray(sc) ? sc : [String(sc)],
          body: parsed,
        };
      },
    };
    read = async (cookie) =>
      (await app.inject({ method: "GET", url: "/v1/auth/session", headers: { cookie } }))
        .statusCode;
  });

  afterAll(async () => {
    await close?.();
    await stop?.();
  });

  const post = (path: string, body: unknown, cookie?: string) =>
    new Request(`${ORIGIN}${path}`, {
      method: "POST",
      headers: {
        ...SAME_ORIGIN,
        "content-type": "application/json",
        ...(cookie ? { cookie } : {}),
      },
      body: JSON.stringify(body),
    });

  /** A signed-in browser's cookies, through the api's login. */
  async function signIn(email: string, password: string, code: string): Promise<string> {
    const res = await api.post(
      "/v1/auth/login",
      { "content-type": "application/json" },
      JSON.stringify({ email, password, totp_code: code }),
    );
    expect(res.status).toBe(200);
    const sess = (res.setCookie[0] ?? "").split(";")[0]?.split("=")[1];
    const csrf = (res.body as { csrf_token: string }).csrf_token;
    return `__Host-cgsess=${sess}; __Host-cgcsrf=${csrf}`;
  }

  test("an invited user's first sign-in, then a password change", async () => {
    await helpers.insertUserDirect(config, { email: "admin@dash009.test", role: "admin" });
    const admin = await signIn(
      "admin@dash009.test",
      helpers.DEFAULT_PASSWORD,
      helpers.currentTotp(),
    );

    const created = await handleMutation(
      post("/api/users", { email: "analyst@dash009.test", role: "analyst" }, admin),
      api,
      MUTATIONS.createUser,
    );
    expect(created.status).toBe(201);
    const { invitation } = (await created.json()) as { invitation: { token: string } };

    // The invitation's handlers carry no session: the admin's cookies are not sent.
    const shown = await handleMutation(
      post("/api/invitations/totp", { token: invitation.token }, admin),
      api,
      MUTATIONS.invitationTotp,
    );
    expect(shown.status).toBe(200);
    const setup = (await shown.json()) as { email: string; provisioning_uri: string };
    expect(setup.email).toBe("analyst@dash009.test");
    const secret = new URL(setup.provisioning_uri).searchParams.get("secret") ?? "";

    const chosen = "analyst password 9";
    const done = await handleMutation(
      post("/api/invitations/complete", {
        token: invitation.token,
        password: chosen,
        totp_code: helpers.totpAt(secret),
      }),
      api,
      MUTATIONS.invitationComplete,
    );
    expect(done.status).toBe(204);

    const analyst = await signIn("analyst@dash009.test", chosen, helpers.totpAt(secret, 1));
    const sessionCookie = `cgsess=${analyst.split(";")[0]?.split("=")[1]}`;
    expect(await read(sessionCookie)).toBe(200);

    const changed = await handleMutation(
      post(
        "/api/account/password",
        {
          currentPassword: chosen,
          newPassword: "another password 10",
          totp_code: helpers.totpAt(secret, -1),
        },
        analyst,
      ),
      api,
      MUTATIONS.accountPassword,
    );
    expect(changed.status).toBe(200);
    expect(changed.headers.getSetCookie()).toHaveLength(2);
    expect(await read(sessionCookie)).toBe(401);
  });

  test("an analyst's request to create a user is the api's 403", async () => {
    await helpers.insertUserDirect(config, { email: "plain@dash009.test", role: "analyst" });
    const analyst = await signIn(
      "plain@dash009.test",
      helpers.DEFAULT_PASSWORD,
      helpers.currentTotp(),
    );
    const res = await handleMutation(
      post("/api/users", { email: "other@dash009.test", role: "viewer" }, analyst),
      api,
      MUTATIONS.createUser,
    );
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: "forbidden" });
  });
});
