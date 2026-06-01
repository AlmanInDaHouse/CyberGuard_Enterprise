import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  buildTestApp,
  insertUserDirect,
  revokeSessionDirect,
  seedSession,
  sessionExists,
} from "./helpers/db.js";

// auth_ac_005 — a revoked session is rejected on the VERY NEXT request, with no
// grace. SECURITY CONTROL and the PRODUCT INVARIANT ratified in ADR-0014 §2
// (immediate revocation = Redis DEL, read-through per request) — first-class.
//
// RED BY DESIGN: there is no session-resolution / revocation control; the authed
// handler is a stub → 501. The session is seeded into and deleted from the REAL
// Redis, so the RED is the absent ENFORCEMENT, not broken setup. GREEN: the
// request after revoke is 401.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a revoked session is rejected (401) on the next request — no grace", async () => {
  const adminId = await insertUserDirect(config, { email: "rev@ac005.test", role: "admin" });
  const token = "tok-rev-ac005";
  await seedSession(config, { token, userId: adminId, role: "admin" });
  expect(await sessionExists(config, token)).toBe(true);

  // Revoke = Redis DEL (the invariant). The very next request MUST be 401.
  await revokeSessionDirect(config, token);
  expect(await sessionExists(config, token)).toBe(false);

  const after = await apph.app.inject({
    method: "GET",
    url: "/v1/users",
    cookies: { cgsess: token },
  });
  // RED: 501 (no session-resolution / revocation). GREEN: 401 immediately.
  expect(after.statusCode).toBe(401);
});
