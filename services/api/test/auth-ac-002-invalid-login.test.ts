import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, currentTotp, insertUserDirect } from "./helpers/db.js";

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

test("a once-accepted TOTP code cannot be replayed", async () => {
  // SECURITY: a valid code is single-use. The first use is accepted; the SAME
  // code reused within its window is rejected — replay detection, not wrong-code.
  const code = currentTotp();
  const first = await login("correct horse battery", code);
  const replay = await login("correct horse battery", code);
  expect(first.statusCode).toBe(200); // valid code accepted once
  expect(replay.statusCode).toBe(401); // same code reused → replay rejected
});
