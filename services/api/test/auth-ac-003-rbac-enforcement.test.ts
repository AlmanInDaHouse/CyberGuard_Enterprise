import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// auth_ac_003 — RBAC is enforced server-side. SECURITY CONTROL — THIS TEST
// BLOCKS CI (threat model § cg-api: "RBAC tests that block CI on missing
// checks").
//
// RED BY DESIGN: there is no RBAC preHandler and the admin handler is a stub →
// every call returns 501. Grey-box setup: a viewer session and an admin session
// are seeded directly into the REAL Redis keyspace (SPEC-008 §Data contracts §3
// format), so the principals exist and the RED is the absent ENFORCEMENT, not
// broken setup.
//
// GREEN MUST pass because the control BLOCKS the viewer (403) and admits the
// admin — NEVER because the test was relaxed. This is Convention #12 applied to
// authorization: a security test that trips the check is fixed by giving the
// principal the right role, never by weakening the check.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a viewer is DENIED an admin-only capability (server-side 403)", async () => {
  const viewerId = await insertUserDirect(config, { email: "viewer@ac003.test", role: "viewer" });
  await seedSession(config, { token: "tok-viewer-ac003", userId: viewerId, role: "viewer" });

  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/users",
    cookies: { cgsess: "tok-viewer-ac003" },
    headers: { "x-csrf-token": "csrf-tok-viewer-ac003" },
    payload: { email: "made-by-viewer@ac003.test", password: "x", role: "viewer" },
  });
  // SECURITY (CI-blocking). RED: 501 (enforcement absent). GREEN: 403 (blocked).
  expect(res.statusCode).toBe(403);
});

test("an admin IS allowed the same capability", async () => {
  const adminId = await insertUserDirect(config, { email: "admin@ac003.test", role: "admin" });
  await seedSession(config, { token: "tok-admin-ac003", userId: adminId, role: "admin" });

  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/users",
    cookies: { cgsess: "tok-admin-ac003" },
    headers: { "x-csrf-token": "csrf-tok-admin-ac003" },
    payload: { email: "made-by-admin@ac003.test", password: "x", role: "viewer" },
  });
  // RED: 501. GREEN: a 2xx create.
  expect(res.statusCode).toBeLessThan(400);
});
