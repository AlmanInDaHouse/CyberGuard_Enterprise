// @vitest-environment node
import { expect, test } from "vitest";
import type { AuthApi } from "../src/lib/auth/api";
import { handleLogin, handleLogout } from "../src/lib/auth/handlers";

// dash_ac_006 — SPEC-009's second Amendment 2026-10-15, item 5: the login and the
// logout pass the request's X-Forwarded-For on to the api as received, and send none
// when the request carries none.

function recording() {
  const calls: { path: string; headers: Record<string, string> }[] = [];
  const api: AuthApi = {
    async post(path, headers) {
      calls.push({ path, headers });
      return {
        status: 200,
        setCookie: ["cgsess=t; Path=/; HttpOnly; Secure; SameSite=Strict"],
        body: { csrf_token: "c" },
      };
    },
  };
  return { api, calls };
}

function request(path: "login" | "logout", headers: Record<string, string>): Request {
  return new Request(`https://soc.example/api/auth/${path}`, {
    method: "POST",
    headers: {
      origin: "https://soc.example",
      host: "soc.example",
      "content-type": "application/json",
      cookie: "cgsess=held; cgcsrf=held-csrf",
      ...headers,
    },
    body:
      path === "login"
        ? JSON.stringify({ email: "a@b.test", password: "x", totp_code: "1" })
        : undefined,
  });
}

const handlers = { login: handleLogin, logout: handleLogout } as const;

for (const path of ["login", "logout"] as const) {
  test(`${path}: a two-entry X-Forwarded-For is passed on as received`, async () => {
    const { api, calls } = recording();
    const xff = "203.0.113.5, 198.51.100.7";
    await handlers[path](request(path, { "x-forwarded-for": xff }), api);
    // The login revokes the browser's session first: both of its calls carry it.
    expect(calls.map((c) => c.path)).toEqual(
      path === "login" ? ["/v1/auth/logout", "/v1/auth/login"] : ["/v1/auth/logout"],
    );
    for (const c of calls) expect(c.headers["x-forwarded-for"]).toBe(xff);
  });

  test(`${path}: no X-Forwarded-For is sent when the request carries none`, async () => {
    const { api, calls } = recording();
    await handlers[path](request(path, {}), api);
    expect(calls.length).toBeGreaterThan(0);
    for (const c of calls) {
      expect(Object.keys(c.headers).map((k) => k.toLowerCase())).not.toContain("x-forwarded-for");
    }
  });
}
