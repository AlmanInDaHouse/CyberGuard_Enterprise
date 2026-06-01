import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp } from "./helpers/db.js";

// read_ac_004 — RBAC/auth enforcement on reads. SECURITY, CI-BLOCKING
// (threat model :84). This is a GREEN-GUARD, reported honestly as such (truth over
// theater): SPEC-008's `makeRequireSession` preHandler runs BEFORE the (stubbed)
// read handler, so an unauthenticated or revoked read is already rejected with
// 401 — the enforcement point exists from 008 and these tests LOCK IT IN (they
// fail if anyone removes the session guard). The authenticated-read RED (valid
// session → 501 handler-absent) is covered by read_ac_001/002/003/005.
//
// Production-faithful (Convention #12): the read controls go GREEN by providing a
// valid session + role, NEVER by removing `makeRequireSession`.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("an unauthenticated read is rejected (401) [green-guard]", async () => {
  const res = await apph.app.inject({ method: "GET", url: "/v1/incidents" });
  expect(res.statusCode).toBe(401);
});

test("a read with an unknown/revoked session is rejected (401) [green-guard]", async () => {
  const res = await apph.app.inject({
    method: "GET",
    url: "/v1/incidents",
    cookies: { cgsess: "this-token-was-never-issued" },
  });
  expect(res.statusCode).toBe(401);
});
