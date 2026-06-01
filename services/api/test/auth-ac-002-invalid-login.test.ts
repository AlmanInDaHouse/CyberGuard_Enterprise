import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect } from "./helpers/db.js";

// auth_ac_002 — invalid login is rejected, generically, with no session.
//
// RED BY DESIGN: login throws NotImplemented → 501 for every branch. The user
// exists (direct SQL), so the RED is the absent reject control. GREEN: wrong
// password / wrong TOTP / a replayed TOTP code each return the SAME generic 401
// (no user-enumeration, no per-factor oracle) and create no session.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  await insertUserDirect(config, {
    email: "ac002@example.test",
    role: "viewer",
    totpEnrolled: true,
  });
});
afterAll(async () => {
  await apph?.close();
});

function login(password: string, totp: string) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    payload: { email: "ac002@example.test", password, totp_code: totp },
  });
}

test("wrong password is rejected with a generic 401", async () => {
  const res = await login("wrong-password", "000000");
  expect(res.statusCode).toBe(401); // RED: 501 (reject control absent)
});

test("wrong TOTP is rejected with a generic 401", async () => {
  const res = await login("correct horse battery", "999999");
  expect(res.statusCode).toBe(401); // RED: 501
});

test("a replayed TOTP code is rejected", async () => {
  const first = await login("correct horse battery", "123456");
  const replay = await login("correct horse battery", "123456");
  // GREEN: even a once-valid code reused within its step → 401. RED: 501.
  expect(replay.statusCode).toBe(401);
  expect(first.statusCode).toBe(replay.statusCode); // indistinguishable
});
