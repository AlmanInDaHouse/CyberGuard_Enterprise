import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// dashapi_ac_002 (SPEC-026 §Data contracts §3, *Session and credentials*) —
// `GET /v1/auth/session` answers the session's user's id, email and role, and `401`
// without one; like every route of §3, it is `Cache-Control: no-store`.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("the session's user's id, email and role", async () => {
  for (const role of ["admin", "analyst", "viewer"] as const) {
    const email = `dashapi002-${role}@example.test`;
    const userId = await insertUserDirect(config, { email, role });
    const token = `tok-dashapi002-${role}`;
    await seedSession(config, { token, userId, role });
    const res = await apph.app.inject({
      method: "GET",
      url: "/v1/auth/session",
      cookies: { cgsess: token },
    });
    expect(res.statusCode, role).toBe(200);
    expect(res.json()).toEqual({ user_id: userId, email, role });
    expect(res.headers["cache-control"]).toBe("no-store");
  }
});

test("401 without a session, and with a session that names none", async () => {
  const none = await apph.app.inject({ method: "GET", url: "/v1/auth/session" });
  expect(none.statusCode).toBe(401);
  const unknown = await apph.app.inject({
    method: "GET",
    url: "/v1/auth/session",
    cookies: { cgsess: "tok-dashapi002-unknown" },
  });
  expect(unknown.statusCode).toBe(401);
  // A live session whose user is not in its org.
  await seedSession(config, {
    token: "tok-dashapi002-nouser",
    userId: globalThis.crypto.randomUUID(),
    role: "admin",
  });
  const nouser = await apph.app.inject({
    method: "GET",
    url: "/v1/auth/session",
    cookies: { cgsess: "tok-dashapi002-nouser" },
  });
  expect(nouser.statusCode).toBe(401);
});
