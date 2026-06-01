import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, getAuditRows, insertUserDirect } from "./helpers/db.js";

// auth_ac_006 — an authentication event writes an append-only audit_log row.
//
// RED BY DESIGN: no auth flow exists to write audit rows (login is a stub) → the
// audit_log stays empty of login events. The audit_log table itself exists
// (migration 0002), so the RED is the absent audit-writing control, not a
// missing table. GREEN: a login attempt (even a failed one) writes an
// `auth.login.*` row with user_id / org_id / action / outcome / correlation_id.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a login attempt writes an auth.login.* audit_log row", async () => {
  await insertUserDirect(config, {
    email: "audit@ac006.test",
    role: "analyst",
    totpEnrolled: true,
  });
  await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    payload: { email: "audit@ac006.test", password: "whatever", totp_code: "000000" },
  });
  const rows = await getAuditRows(config);
  // RED: 0 login audit rows (no audit control). GREEN: >= 1 auth.login.* row.
  expect(rows.some((r) => r.action.startsWith("auth.login"))).toBe(true);
});
