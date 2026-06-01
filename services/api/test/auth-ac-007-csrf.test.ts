import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// auth_ac_007 — a session-authenticated mutation requires a session-bound CSRF
// token. SECURITY CONTROL (threat model § cg-api / § Dashboard: "Fastify-issued
// CSRF tokens on every mutation"; SameSite=Strict is only partial mitigation).
//
// RED BY DESIGN: there is no CSRF check; the mutation handler is a stub → 501.
// The session is seeded into the REAL Redis, so the RED is the absent CSRF
// control. GREEN: a mutation WITHOUT the session-bound token → 403; WITH it →
// not 403.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a mutation without a valid CSRF token is rejected (403)", async () => {
  const userId = await insertUserDirect(config, {
    email: "csrf@ac007.test",
    role: "analyst",
    totpEnrolled: true,
  });
  const token = "tok-csrf-ac007";
  await seedSession(config, { token, userId, role: "analyst", csrfToken: "the-csrf-007" });

  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/password",
    cookies: { cgsess: token },
    // No x-csrf-token header.
    payload: { userId, currentPassword: "x", newPassword: "y", totp_code: "000000" },
  });
  // SECURITY. RED: 501 (CSRF control absent). GREEN: 403 (missing CSRF token).
  expect(res.statusCode).toBe(403);
});
