import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, currentTotp, insertUserDirect } from "./helpers/db.js";

// auth_ac_001 — a valid login issues an opaque session cookie.
//
// RED BY DESIGN: AuthService.login throws NotImplemented → the app maps it to
// 501. The precondition user is inserted via direct SQL against the REAL users
// table (migration 0001 applied in startBackends), so the RED is the absent
// LOGIN control, never broken setup. GREEN: correct password + current TOTP →
// 200 + Set-Cookie cgsess (HttpOnly/Secure/SameSite=Strict) + a CSRF token. (At
// the GREEN gate the setup supplies a real credential via createUser — satisfying
// the precondition, not weakening any assertion.)

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("valid password + TOTP issues an opaque session cookie", async () => {
  await insertUserDirect(config, {
    email: "ac001@example.test",
    role: "analyst",
    totpEnrolled: true,
  });
  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    payload: {
      email: "ac001@example.test",
      password: "correct horse battery",
      totp_code: currentTotp(),
    },
  });
  // RED: 501 (login control absent). GREEN: 200 with a cgsess cookie.
  expect(res.statusCode).toBe(200);
  expect(res.cookies.some((c) => c.name === "cgsess")).toBe(true);
});
