import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  DEFAULT_PASSWORD,
  buildTestApp,
  currentTotp,
  insertUserDirect,
  seedSession,
  sessionExists,
} from "./helpers/db.js";

// dashapi_ac_003 (SPEC-026 §Operational §2, debt #102) — a login carrying the cookie
// of a live session deletes that session, whatever user it belonged to, and issues
// another; a failed login carrying it leaves it live.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

/** The new session's token, from the login's Set-Cookie. */
function cookieOf(res: { cookies: Array<{ name: string; value: string }> }): string | undefined {
  return res.cookies.find((c) => c.name === "cgsess")?.value;
}

test("a successful login deletes the cookie's session, another user's too, and issues another", async () => {
  const otherId = await insertUserDirect(config, {
    email: "dashapi003-other@example.test",
    role: "admin",
  });
  await insertUserDirect(config, { email: "dashapi003-ok@example.test", role: "analyst" });
  const prior = "tok-dashapi003-prior";
  await seedSession(config, { token: prior, userId: otherId, role: "admin" });

  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    cookies: { cgsess: prior },
    payload: {
      email: "dashapi003-ok@example.test",
      password: DEFAULT_PASSWORD,
      totp_code: currentTotp(),
    },
  });
  expect(res.statusCode).toBe(200);
  const issued = cookieOf(res);
  expect(issued).toBeTruthy();
  expect(issued).not.toBe(prior);
  expect(await sessionExists(config, prior)).toBe(false);
  expect(await sessionExists(config, issued as string)).toBe(true);

  // The old session is refused on its next request.
  const after = await apph.app.inject({
    method: "GET",
    url: "/v1/users",
    cookies: { cgsess: prior },
  });
  expect(after.statusCode).toBe(401);
});

test("a failed login carrying the cookie leaves its session live", async () => {
  const userId = await insertUserDirect(config, {
    email: "dashapi003-fail@example.test",
    role: "admin",
  });
  const prior = "tok-dashapi003-fail";
  await seedSession(config, { token: prior, userId, role: "admin" });

  const wrongPassword = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    cookies: { cgsess: prior },
    payload: {
      email: "dashapi003-fail@example.test",
      password: "not the password at all",
      totp_code: currentTotp(),
    },
  });
  expect(wrongPassword.statusCode).toBe(401);
  expect(await sessionExists(config, prior)).toBe(true);

  const malformed = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    cookies: { cgsess: prior },
    payload: { email: "dashapi003-fail@example.test" },
  });
  expect(malformed.statusCode).toBe(400);
  expect(await sessionExists(config, prior)).toBe(true);

  // Refused after the password: a disabled account, its password and code right.
  await insertUserDirect(config, {
    email: "dashapi003-disabled@example.test",
    role: "analyst",
    status: "disabled",
  });
  const disabled = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    cookies: { cgsess: prior },
    payload: {
      email: "dashapi003-disabled@example.test",
      password: DEFAULT_PASSWORD,
      totp_code: currentTotp(),
    },
  });
  expect(disabled.statusCode).toBe(401);
  expect(await sessionExists(config, prior)).toBe(true);

  const still = await apph.app.inject({
    method: "GET",
    url: "/v1/users",
    cookies: { cgsess: prior },
  });
  expect(still.statusCode).toBe(200);
});
