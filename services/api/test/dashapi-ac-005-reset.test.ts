import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  DEFAULT_PASSWORD,
  buildTestApp,
  completeInvitationAsHolder,
  currentTotp,
  insertUserDirect,
  seedSession,
  sessionExists,
  totpAt,
} from "./helpers/db.js";

// dashapi_ac_005 (SPEC-026 §Operational §4, ADR-0014 Amendment 2026-11-20) — the reset. A
// reset clears the user's credentials, raises their `authority_version`, deletes their sessions,
// withdraws their open invitation and answers a new one, with `user.reset`,
// `invitation.withdraw` and `invitation.issue`; a login with the old password fails; the user
// completes the new invitation and signs in. An admin's own reset answers
// `409 cannot_reset_self`, a disabled user's `409 user_disabled`.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
let pool: pg.Pool;
const ADMIN = "tok-dashapi005-admin";
let adminId: string;
const asAdmin = { cookies: { cgsess: ADMIN }, headers: { "x-csrf-token": `csrf-${ADMIN}` } };
const NEW_PASSWORD = "Reset-passw0rd-005";

function reset(userId: string, as = asAdmin) {
  return apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/reset`,
    payload: {},
    ...as,
  });
}

async function auditOf(target: string) {
  const r = await pool.query<{ action: string; user_id: string; detail: unknown }>(
    "SELECT action, user_id, detail FROM audit_log WHERE target_id = $1 ORDER BY id",
    [target],
  );
  return r.rows;
}

beforeAll(async () => {
  config = inject("apiConfig");
  pool = new pg.Pool({ connectionString: config.API_PG_URL });
  apph = await buildTestApp(config);
  adminId = await insertUserDirect(config, { email: "admin@dashapi005.test", role: "admin" });
  await seedSession(config, { token: ADMIN, userId: adminId, role: "admin" });
});
afterAll(async () => {
  await apph?.close();
  await pool?.end();
});

test("a reset clears the credentials, ends the sessions, replaces the invitation, and the user signs in anew", async () => {
  const email = "analyst@dashapi005.test";
  const userId = await insertUserDirect(config, { email, role: "analyst" });
  await seedSession(config, { token: "tok-dashapi005-a", userId, role: "analyst" });
  await seedSession(config, { token: "tok-dashapi005-b", userId, role: "analyst" });
  // An open invitation the reset withdraws (left, say, by an earlier reset never completed).
  await pool.query(
    `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, expires_at, pending_totp_secret)
     VALUES (gen_random_uuid(), $1, 'default', 'reset', '\\x0005', now() + interval '1 hour', '\\x01')`,
    [userId],
  );
  const before = await pool.query<{ v: string }>(
    "SELECT authority_version::text AS v FROM users WHERE user_id = $1",
    [userId],
  );

  const res = await reset(userId);
  expect(res.statusCode, res.body).toBe(201);
  expect(res.headers["cache-control"]).toBe("no-store");
  const body = res.json() as { token: string; expires_at: string };
  expect(Object.keys(body).sort()).toEqual(["expires_at", "token"]);
  expect(res.body).not.toMatch(/otpauth|secret/i);

  const user = await pool.query(
    `SELECT password_hash, totp_secret, totp_enrolled, authority_version::text AS v
       FROM users WHERE user_id = $1`,
    [userId],
  );
  expect(user.rows).toEqual([
    {
      password_hash: null,
      totp_secret: null,
      totp_enrolled: false,
      v: String(Number(before.rows[0]?.v) + 1),
    },
  ]);
  expect(await sessionExists(config, "tok-dashapi005-a")).toBe(false);
  expect(await sessionExists(config, "tok-dashapi005-b")).toBe(false);
  const inv = await pool.query<{ purpose: string; open: boolean; secret: boolean }>(
    `SELECT purpose, completed_at IS NULL AND withdrawn_at IS NULL AS open,
            pending_totp_secret IS NOT NULL AS secret
       FROM invitations WHERE user_id = $1 ORDER BY issued_at`,
    [userId],
  );
  expect(inv.rows).toEqual([
    { purpose: "reset", open: false, secret: false },
    { purpose: "reset", open: true, secret: false },
  ]);
  expect(await auditOf(userId)).toEqual([
    { action: "user.reset", user_id: adminId, detail: null },
    { action: "invitation.withdraw", user_id: adminId, detail: { purpose: "reset" } },
    {
      action: "invitation.issue",
      user_id: adminId,
      detail: { purpose: "reset", expires_at: body.expires_at },
    },
  ]);

  // The old password and factor no longer sign in.
  const old = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: "198.51.105.1",
    payload: { email, password: DEFAULT_PASSWORD, totp_code: currentTotp() },
  });
  expect(old.statusCode).toBe(401);

  // The user completes the new invitation and signs in.
  const secret = await completeInvitationAsHolder(
    apph.app,
    body.token,
    NEW_PASSWORD,
    "198.51.105.2",
  );
  const login = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: "198.51.105.3",
    payload: { email, password: NEW_PASSWORD, totp_code: totpAt(secret, 1) },
  });
  expect(login.statusCode, login.body).toBe(200);
});

test("an admin's own reset is 409 cannot_reset_self, a disabled user's 409 user_disabled, and change nothing", async () => {
  const self = await reset(adminId);
  expect(self.statusCode).toBe(409);
  expect(self.json()).toEqual({ error: "cannot_reset_self" });
  const disabled = await insertUserDirect(config, {
    email: "disabled@dashapi005.test",
    role: "viewer",
    status: "disabled",
  });
  const d = await reset(disabled);
  expect(d.statusCode).toBe(409);
  expect(d.json()).toEqual({ error: "user_disabled" });
  for (const id of [adminId, disabled]) {
    const r = await pool.query<{ hashed: boolean }>(
      "SELECT password_hash IS NOT NULL AS hashed FROM users WHERE user_id = $1",
      [id],
    );
    expect(r.rows).toEqual([{ hashed: true }]);
    expect(await auditOf(id)).toEqual([]);
  }
  expect(await sessionExists(config, ADMIN)).toBe(true);
});

test("a user of another org, or none, is a 404", async () => {
  const other = await insertUserDirect(config, {
    email: "other@dashapi005.test",
    role: "viewer",
    orgId: "dashapi-005-other",
  });
  for (const id of [other, globalThis.crypto.randomUUID()]) {
    const res = await reset(id);
    expect(res.statusCode).toBe(404);
  }
});

test("a reset admin no longer counts as the last active admin's peer", async () => {
  const org = "dashapi-005-admins";
  const a = await insertUserDirect(config, {
    email: "a@dashapi005.test",
    role: "admin",
    orgId: org,
  });
  const b = await insertUserDirect(config, {
    email: "b@dashapi005.test",
    role: "admin",
    orgId: org,
  });
  await seedSession(config, { token: "tok-dashapi005-orga", userId: a, role: "admin", orgId: org });
  const asA = {
    cookies: { cgsess: "tok-dashapi005-orga" },
    headers: { "x-csrf-token": "csrf-tok-dashapi005-orga" },
  };
  expect((await reset(b, asA)).statusCode).toBe(201);
  // b is an active admin whose credentials were cleared: a may not remove themself.
  const demote = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${a}/role`,
    payload: { role: "analyst" },
    ...asA,
  });
  expect(demote.statusCode).toBe(409);
});
