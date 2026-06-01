import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, currentTotp, getUserByEmail, insertUserDirect } from "./helpers/db.js";

// auth_ac_009 — confirming a TOTP code flips totp_enrolled false→true (the
// no-delivery, on-screen first-enrollment ADR-0014 keeps in scope).
//
// RED BY DESIGN: confirmTotpEnrollment is a stub → 501, so the flag never flips.
// The user exists with totp_enrolled=false (direct SQL), so the RED is the
// absent enrollment-confirmation control — and it drives the totp_enrolled state
// §Data contracts §1 introduces, so the column is not an unreachable state field.
// GREEN: a valid current code → 2xx + totp_enrolled = true.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("confirming a code flips totp_enrolled false→true", async () => {
  const userId = await insertUserDirect(config, {
    email: "enroll@ac009.test",
    role: "analyst",
    totpEnrolled: false,
  });
  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/totp/confirm",
    payload: { userId, totp_code: currentTotp() },
  });
  // RED: 501 (enrollment-confirmation absent). GREEN: 2xx.
  expect(res.statusCode).toBeLessThan(300);

  const row = await getUserByEmail(config, "enroll@ac009.test");
  expect(row?.totp_enrolled).toBe(true);
});
