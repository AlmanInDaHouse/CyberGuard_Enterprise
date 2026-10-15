// @vitest-environment node
import { expect, test } from "vitest";
import type { AuthApi } from "../src/lib/auth/api";
import { handleLogin, handleLogout } from "../src/lib/auth/handlers";

// dash_ac_005 — SPEC-009's second Amendment 2026-10-15, item 2: the login and the
// logout answer 403 to a request that is not same-origin, before they read its body or
// a cookie, without calling the api or changing a cookie.

function recording() {
  const calls: string[] = [];
  const api: AuthApi = {
    async post(path) {
      calls.push(path);
      return {
        status: 200,
        setCookie: ["cgsess=t; Path=/; HttpOnly; Secure; SameSite=Strict"],
        body: { csrf_token: "c" },
      };
    },
  };
  return { api, calls };
}

const BODY = JSON.stringify({ email: "a@b.test", password: "x", totp_code: "1" });

function request(
  path: "login" | "logout",
  headers: Record<string, string>,
  body: string | undefined = path === "login" ? BODY : undefined,
): Request {
  return new Request(`https://soc.example/api/auth/${path}`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      cookie: "cgsess=held; cgcsrf=held-csrf",
      ...headers,
    },
    body,
  });
}

const handlers = { login: handleLogin, logout: handleLogout } as const;

const REFUSED: [string, Record<string, string>][] = [
  ["an Origin of another host", { origin: "https://evil.example", host: "soc.example" }],
  ["an Origin of another port", { origin: "https://soc.example:8443", host: "soc.example" }],
  ["no Origin", { host: "soc.example" }],
  ["an Origin of null", { origin: "null", host: "soc.example" }],
  [
    "Sec-Fetch-Site cross-site",
    { origin: "https://soc.example", host: "soc.example", "sec-fetch-site": "cross-site" },
  ],
  [
    "Sec-Fetch-Site same-site",
    { origin: "https://soc.example", host: "soc.example", "sec-fetch-site": "same-site" },
  ],
  [
    "Sec-Fetch-Site none",
    { origin: "https://soc.example", host: "soc.example", "sec-fetch-site": "none" },
  ],
  ["port 443 on an http Origin", { origin: "http://soc.example:443", host: "soc.example" }],
  ["port 80 on an https Origin", { origin: "https://soc.example:80", host: "soc.example" }],
  [
    "an Origin that matches Host but not the first X-Forwarded-Host",
    { origin: "https://soc.example", host: "soc.example", "x-forwarded-host": "other.example" },
  ],
];

for (const path of ["login", "logout"] as const) {
  test.each(REFUSED)(`${path}: %s answers 403 without calling the api`, async (_name, headers) => {
    const { api, calls } = recording();
    const res = await handlers[path](request(path, headers), api);
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: "forbidden" });
    expect(calls).toEqual([]);
    expect(res.headers.getSetCookie()).toEqual([]);
  });
}

test("a malformed login body behind a refused origin is refused as 403, not 400", async () => {
  const { api, calls } = recording();
  const res = await handleLogin(
    request("login", { origin: "https://evil.example", host: "soc.example" }, "{"),
    api,
  );
  expect(res.status).toBe(403);
  expect(calls).toEqual([]);
});

const PASSED: [string, Record<string, string>][] = [
  ["the Origin's host is Host", { origin: "https://soc.example", host: "soc.example" }],
  [
    "the Origin's host is the first X-Forwarded-Host entry",
    {
      origin: "https://soc.example",
      host: "dashboard:3000",
      "x-forwarded-host": "soc.example, proxy.internal",
    },
  ],
  [
    "its scheme's default port on the Origin",
    { origin: "https://soc.example:443", host: "soc.example" },
  ],
  ["port 80 on an http Origin", { origin: "http://soc.example:80", host: "soc.example" }],
  [
    "an empty X-Forwarded-Host",
    { origin: "https://soc.example", host: "soc.example", "x-forwarded-host": "" },
  ],
  [
    "an empty first X-Forwarded-Host entry",
    { origin: "https://soc.example", host: "soc.example", "x-forwarded-host": ", other.example" },
  ],
  ["a default port on Host", { origin: "https://soc.example", host: "soc.example:443" }],
  [
    "a default port on the first X-Forwarded-Host entry",
    {
      origin: "https://soc.example",
      host: "dashboard:3000",
      "x-forwarded-host": "soc.example:443",
    },
  ],
  ["port 80 on Host", { origin: "http://soc.example", host: "soc.example:80" }],
  ["a host in another case", { origin: "https://SOC.example", host: "soc.EXAMPLE" }],
  [
    "Sec-Fetch-Site same-origin",
    { origin: "https://soc.example", host: "soc.example", "sec-fetch-site": "same-origin" },
  ],
];

for (const path of ["login", "logout"] as const) {
  test.each(PASSED)(`${path}: %s passes`, async (_name, headers) => {
    const { api, calls } = recording();
    const res = await handlers[path](request(path, headers), api);
    expect(res.status).toBe(200);
    expect(calls.at(-1)).toBe(`/v1/auth/${path}`);
  });
}
