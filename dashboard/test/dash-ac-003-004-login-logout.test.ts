import { RequestCookies } from "next/dist/server/web/spec-extension/cookies";
// @vitest-environment node
import { afterAll, beforeAll, describe, expect, test } from "vitest";
import type { AuthApi, AuthApiResponse } from "../src/lib/auth/api";
import { sessionCookieHeader } from "../src/lib/auth/cookies";
import { handleLogin, handleLogout } from "../src/lib/auth/handlers";

// dash_ac_003 (login) and dash_ac_004 (logout) — SPEC-009's second Amendment
// 2026-10-15, the dashboard's login and logout. Where a case asserts what the api did,
// the handler calls the real api in-process, as dash_ac_001 does; where it asserts what
// the handler sends or how it maps an answer, a recording client stands in for the api.

const loadApi = (specifier: string) => import(specifier);

const ORIGIN = "https://soc.example";
const SAME_ORIGIN = { origin: ORIGIN, host: "soc.example", "sec-fetch-site": "same-origin" };

let stop: (() => Promise<void>) | undefined;
let close: (() => Promise<void>) | undefined;
let realApi: AuthApi;
let read: (cookie: string) => Promise<number>;
let helpers: {
  insertUserDirect: (config: unknown, opts: Record<string, unknown>) => Promise<string>;
  currentTotp: () => string;
  seedSession: (config: unknown, s: Record<string, unknown>) => Promise<void>;
  sessionExists: (config: unknown, token: string) => Promise<boolean>;
  DEFAULT_PASSWORD: string;
};
let config: unknown;

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

  realApi = {
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
    (await app.inject({ method: "GET", url: "/v1/incidents", headers: { cookie } })).statusCode;
});

afterAll(async () => {
  await close?.();
  await stop?.();
});

/** A stand-in for the api that records each call and answers as it is told. */
function recording(answer: AuthApiResponse | "no answer") {
  const calls: { path: string; headers: Record<string, string>; body?: string }[] = [];
  const api: AuthApi = {
    async post(path, headers, body) {
      calls.push({ path, headers, body });
      if (answer === "no answer") throw new TypeError("fetch failed");
      return answer;
    },
  };
  return { api, calls };
}

function login(
  body: string | ReadableStream<Uint8Array>,
  headers: Record<string, string> = {},
): Request {
  return new Request(`${ORIGIN}/api/auth/login`, {
    method: "POST",
    headers: { ...SAME_ORIGIN, "content-type": "application/json", ...headers },
    body,
    duplex: "half",
  } as RequestInit);
}

function logout(cookie?: string): Request {
  return new Request(`${ORIGIN}/api/auth/logout`, {
    method: "POST",
    headers: { ...SAME_ORIGIN, ...(cookie === undefined ? {} : { cookie }) },
  });
}

/** A login body of exactly `size` bytes, its password filling it. */
function bodyOfBytes(size: number): string {
  const empty = JSON.stringify({ email: "a@b.test", password: "", totp_code: "1" });
  return JSON.stringify({
    email: "a@b.test",
    password: "x".repeat(size - empty.length),
    totp_code: "1",
  });
}

/** A body whose stream fails partway, as when the client goes away. */
function abortedBody(): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode('{"email":'));
      controller.error(new Error("the client went away"));
    },
  });
}

const credentials = (email: string, code: string, password?: string) =>
  JSON.stringify({ email, password: password ?? helpers.DEFAULT_PASSWORD, totp_code: code });

/** The name and the value of each `Set-Cookie` the handler answered with. */
function cookiesOf(res: Response): Record<string, string> {
  const out: Record<string, string> = {};
  for (const c of res.headers.getSetCookie()) out[c.slice(0, c.indexOf("="))] = c;
  return out;
}

const tokenOf = (setCookie: string | undefined) =>
  setCookie?.slice(setCookie.indexOf("=") + 1, setCookie.indexOf(";"));

const CLEARED = (name: string) => `${name}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0`;

// SPEC-023 §Operational §7 (debt #106): the browser's cookies carry the `__Host-` prefix;
// the api's own cookie keeps the name `cgsess`.
const SESS = "__Host-cgsess";
const CSRF = "__Host-cgcsrf";
const CLEARED_BOTH = { [SESS]: CLEARED(SESS), [CSRF]: CLEARED(CSRF) };

describe("dash_ac_003 — login", () => {
  test("a user with a password and a confirmed TOTP signs in", async () => {
    await helpers.insertUserDirect(config, { email: "signin@dash003.test", role: "analyst" });
    const res = await handleLogin(
      login(credentials("signin@dash003.test", helpers.currentTotp()), {
        "content-type": "application/json; charset=utf-8",
      }),
      realApi,
    );
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ ok: true });
    expect(res.headers.get("cache-control")).toBe("no-store");
    const cookies = cookiesOf(res);
    expect(Object.keys(cookies).sort()).toEqual([CSRF, SESS]);
    for (const name of [SESS, CSRF]) {
      expect(cookies[name]).toMatch(
        new RegExp(`^${name}=[A-Za-z0-9_-]+; HttpOnly; Secure; SameSite=Strict; Path=/$`),
      );
    }
    expect(await read(`cgsess=${tokenOf(cookies[SESS])}`)).toBe(200);
  });

  test("a wrong code answers 401 and sets no cookie", async () => {
    await helpers.insertUserDirect(config, { email: "wrong@dash003.test", role: "analyst" });
    const res = await handleLogin(login(credentials("wrong@dash003.test", "000000")), realApi);
    expect(res.status).toBe(401);
    expect(await res.json()).toEqual({ error: "unauthorized" });
    expect(res.headers.getSetCookie()).toEqual([]);
    expect(res.headers.get("cache-control")).toBe("no-store");
  });

  test.each([
    ["a missing field", () => login(JSON.stringify({ email: "a@b.test", password: "x" }))],
    [
      "an extra field",
      () => login(JSON.stringify({ email: "a@b.test", password: "x", totp_code: "1", org: "o" })),
    ],
    [
      "a field that is not a string",
      () => login(JSON.stringify({ email: "a@b.test", password: 1, totp_code: "1" })),
    ],
    ["a body of 16 KiB and one byte", () => login(bodyOfBytes(16 * 1024 + 1))],
    ["a body the client stops sending", () => login(abortedBody())],
    [
      "another content type",
      () => login(credentials("a@b.test", "1", "x"), { "content-type": "text/plain" }),
    ],
    ["a body that is not JSON", () => login("{")],
  ])("%s answers 400 without calling the api", async (_name, req) => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: {} });
    const res = await handleLogin(req(), api);
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: "bad_request" });
    expect(calls).toEqual([]);
    expect(res.headers.getSetCookie()).toEqual([]);
  });

  test("a body of exactly 16 KiB is read and sent on", async () => {
    const { api, calls } = recording({
      status: 401,
      setCookie: [],
      body: { error: "unauthorized" },
    });
    const res = await handleLogin(login(bodyOfBytes(16 * 1024)), api);
    expect(res.status).toBe(401);
    expect(calls.map((c) => c.path)).toEqual(["/v1/auth/login"]);
  });

  test.each([
    [429, "rate_limited"],
    [503, "unavailable"],
    [400, "bad_request"],
  ])("the api's %i keeps its status and code", async (status, error) => {
    const { api } = recording({ status, setCookie: [], body: { error } });
    const res = await handleLogin(login(credentials("a@b.test", "1")), api);
    expect(res.status).toBe(status);
    expect(await res.json()).toEqual({ error });
  });

  test.each([
    ["a 200 without its session token", { status: 200, setCookie: [], body: { csrf_token: "c" } }],
    [
      "a 200 without its CSRF token",
      { status: 200, setCookie: ["cgsess=t; Path=/; HttpOnly; Secure; SameSite=Strict"], body: {} },
    ],
    ["another status", { status: 500, setCookie: [], body: { error: "internal" } }],
    [
      "a session token that is not a token",
      { status: 200, setCookie: ["cgsess=a.b; Path=/"], body: { csrf_token: "c" } },
    ],
    [
      "a CSRF token that is not a token",
      { status: 200, setCookie: ["cgsess=t; Path=/"], body: { csrf_token: "c$" } },
    ],
  ])("%s answers 502", async (_name, answer) => {
    const { api } = recording(answer);
    const res = await handleLogin(login(credentials("a@b.test", "1")), api);
    expect(res.status).toBe(502);
    expect(await res.json()).toEqual({ error: "upstream" });
    expect(res.headers.getSetCookie()).toEqual([]);
  });

  test("no answer from the api answers 503", async () => {
    const { api } = recording("no answer");
    const res = await handleLogin(login(credentials("a@b.test", "1")), api);
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ error: "unavailable" });
  });

  test("a login from a browser that holds a session revokes it", async () => {
    await helpers.seedSession(config, {
      token: "tok-dash003-old",
      userId: globalThis.crypto.randomUUID(),
      role: "analyst",
      csrfToken: "csrf-dash003-old",
    });
    expect(await read("cgsess=tok-dash003-old")).toBe(200);
    await helpers.insertUserDirect(config, { email: "again@dash003.test", role: "analyst" });
    const res = await handleLogin(
      login(credentials("again@dash003.test", helpers.currentTotp()), {
        cookie: "__Host-cgsess=tok-dash003-old; __Host-cgcsrf=csrf-dash003-old",
      }),
      realApi,
    );
    expect(res.status).toBe(200);
    expect(await read("cgsess=tok-dash003-old")).toBe(401);
    expect(tokenOf(cookiesOf(res)[SESS])).not.toBe("tok-dash003-old");
  });

  test("cookies of another shape are no session: the login revokes nothing", async () => {
    const { api, calls } = recording({
      status: 401,
      setCookie: [],
      body: { error: "unauthorized" },
    });
    const res = await handleLogin(
      login(credentials("a@b.test", "1", "x"), { cookie: "__Host-cgsess=a%0Ab; __Host-cgcsrf=c" }),
      api,
    );
    expect(calls.map((c) => c.path)).toEqual(["/v1/auth/login"]);
    expect(res.status).toBe(401);
    expect(res.headers.getSetCookie()).toEqual([]);
  });

  test("after revoking the browser's session, a failed login clears both cookies", async () => {
    await helpers.seedSession(config, {
      token: "tok-dash003-fail",
      userId: globalThis.crypto.randomUUID(),
      role: "analyst",
      csrfToken: "csrf-dash003-fail",
    });
    const res = await handleLogin(
      login(credentials("nobody@dash003.test", "000000"), {
        cookie: "__Host-cgsess=tok-dash003-fail; __Host-cgcsrf=csrf-dash003-fail",
      }),
      realApi,
    );
    expect(res.status).toBe(401);
    expect(await read("cgsess=tok-dash003-fail")).toBe(401);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });
});

describe("dash_ac_004 — logout", () => {
  test("a signed-in session signs out: the api deletes it and both cookies are cleared", async () => {
    await helpers.seedSession(config, {
      token: "tok-dash004",
      userId: globalThis.crypto.randomUUID(),
      role: "analyst",
      csrfToken: "csrf-dash004",
    });
    const res = await handleLogout(
      logout("__Host-cgsess=tok-dash004; __Host-cgcsrf=csrf-dash004"),
      realApi,
    );
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ ok: true });
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
    expect(await helpers.sessionExists(config, "tok-dash004")).toBe(false);
    expect(await read("cgsess=tok-dash004")).toBe(401);
  });

  test("a login's cookies sign out: cgcsrf holds the api's CSRF token", async () => {
    await helpers.insertUserDirect(config, { email: "roundtrip@dash004.test", role: "analyst" });
    const signedIn = await handleLogin(
      login(credentials("roundtrip@dash004.test", helpers.currentTotp())),
      realApi,
    );
    expect(signedIn.status).toBe(200);
    const session = tokenOf(cookiesOf(signedIn)[SESS]) as string;
    const csrf = tokenOf(cookiesOf(signedIn)[CSRF]) as string;
    expect(csrf).not.toBe(session);
    const res = await handleLogout(
      logout(`__Host-cgsess=${session}; __Host-cgcsrf=${csrf}`),
      realApi,
    );
    expect(res.status).toBe(200);
    expect(await helpers.sessionExists(config, session)).toBe(false);
    expect(await read(`cgsess=${session}`)).toBe(401);
  });

  test("a repeated cgsess means the session the pages read", async () => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: { ok: true } });
    const header = "__Host-cgsess=first; __Host-cgcsrf=c; __Host-cgsess=second";
    await handleLogout(logout(header), api);
    const pages = sessionCookieHeader(new RequestCookies(new Headers({ cookie: header })));
    expect(calls[0]?.headers.cookie).toBe(pages);
    // The api reads its own name.
    expect(pages).toMatch(/^cgsess=/);
  });

  test("a cookie named cgsess, without the prefix, is no session: cleared, 200, the api not called", async () => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: { ok: true } });
    const res = await handleLogout(logout("cgsess=t; cgcsrf=c"), api);
    expect(res.status).toBe(200);
    expect(calls).toHaveLength(0);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });

  test("a session the api no longer holds signs out with 200", async () => {
    const res = await handleLogout(
      logout("__Host-cgsess=tok-dash004-gone; __Host-cgcsrf=c"),
      realApi,
    );
    expect(res.status).toBe(200);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });

  test("a cgcsrf that does not match answers 502, cleared, and the session stays", async () => {
    await helpers.seedSession(config, {
      token: "tok-dash004-csrf",
      userId: globalThis.crypto.randomUUID(),
      role: "analyst",
      csrfToken: "csrf-dash004-right",
    });
    const res = await handleLogout(
      logout("__Host-cgsess=tok-dash004-csrf; __Host-cgcsrf=wrong"),
      realApi,
    );
    expect(res.status).toBe(502);
    expect(await res.json()).toEqual({ error: "upstream" });
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
    expect(await helpers.sessionExists(config, "tok-dash004-csrf")).toBe(true);
  });

  test.each([
    ["the api's 503", { status: 503, setCookie: [], body: { error: "unavailable" } }],
    ["no answer", "no answer"],
  ] as [string, AuthApiResponse | "no answer"][])(
    "%s still clears both cookies and answers 503",
    async (_name, answer) => {
      const { api, calls } = recording(answer);
      const res = await handleLogout(logout("__Host-cgsess=t; __Host-cgcsrf=c"), api);
      expect(calls.length).toBe(1);
      expect(res.status).toBe(503);
      expect(await res.json()).toEqual({ error: "unavailable" });
      expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
    },
  );

  test("another answer of the api is 502, and the cookies are cleared", async () => {
    const { api } = recording({ status: 500, setCookie: [], body: { error: "internal" } });
    const res = await handleLogout(logout("__Host-cgsess=t; __Host-cgcsrf=c"), api);
    expect(res.status).toBe(502);
    expect(await res.json()).toEqual({ error: "upstream" });
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });

  test("a cgsess without cgcsrf clears both and answers 503 without calling the api", async () => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: { ok: true } });
    const res = await handleLogout(logout("__Host-cgsess=t"), api);
    expect(calls).toEqual([]);
    expect(res.status).toBe(503);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });

  test("a cgsess of another shape is no session: cleared, 200, the api not called", async () => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: { ok: true } });
    const res = await handleLogout(logout("__Host-cgsess=a%0Ab; __Host-cgcsrf=c"), api);
    expect(calls).toEqual([]);
    expect(res.status).toBe(200);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });

  test("a cgcsrf of another shape is no cgcsrf: cleared, 503, the api not called", async () => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: { ok: true } });
    const res = await handleLogout(logout("__Host-cgsess=t; __Host-cgcsrf=c%24"), api);
    expect(calls).toEqual([]);
    expect(res.status).toBe(503);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });

  test("no cgsess clears both and answers 200 without calling the api", async () => {
    const { api, calls } = recording({ status: 200, setCookie: [], body: { ok: true } });
    const res = await handleLogout(logout(), api);
    expect(calls).toEqual([]);
    expect(res.status).toBe(200);
    expect(cookiesOf(res)).toEqual(CLEARED_BOTH);
  });
});
