import { createHash } from "node:crypto";
import otplib from "otplib";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// dashapi_ac_004 (SPEC-026 §Operational §3, debt #98) — an invitation. `POST /v1/users`
// answers a token once; the database holds its SHA-256 and no column holds its text;
// `/v1/invitations/totp` answers the email and a provisioning URI, writes `invitation.totp`, and
// a second call answers another secret; `complete` with a code of the first secret answers 401
// and writes a `denied` row, with one of the second 204, after which the user signs in with the
// chosen password and a code of that secret, and the invitation answers 401. An expired,
// withdrawn or completed invitation, and a disabled user's, answer 401; disabling a user with an
// open invitation withdraws it, with an `invitation.withdraw` row; the sixth failure for one
// invitation within 15 minutes answers 429. No route that takes the admin's session answers a
// TOTP secret.
//
// With them, the invitation routes of §Data contracts §3: a new invitation for a user with no
// credentials, which withdraws the open one; its withdrawal; and their refusals.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
let pool: pg.Pool;
const ORG = "default";
const ADMIN = "tok-dashapi004-admin";
let adminId: string;
const asAdmin = { cookies: { cgsess: ADMIN }, headers: { "x-csrf-token": `csrf-${ADMIN}` } };
const PASSWORD = "Invited-passw0rd-004";
let ip = 0;
/** A fresh source address, so that no test fills another's per-address counter. */
function address(): string {
  ip += 1;
  return `198.51.104.${ip}`;
}

/** A code of `secret` `steps` time steps from now. */
function codeAt(secret: string, steps = 0): string {
  return otplib.authenticator.clone({ epoch: Date.now() + steps * 30_000 }).generate(secret);
}

function secretOf(uri: string): string {
  return new URL(uri).searchParams.get("secret") ?? "";
}

function sha256(text: string): Buffer {
  return createHash("sha256").update(text, "ascii").digest();
}

let n = 0;
async function createUser(
  role = "analyst",
): Promise<{ userId: string; token: string; email: string }> {
  n += 1;
  const email = `invitee-${n}@dashapi004.test`;
  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/users",
    payload: { email, role },
    ...asAdmin,
  });
  expect(res.statusCode, res.body).toBe(201);
  const body = res.json() as { user_id: string; invitation: { token: string; expires_at: string } };
  return { userId: body.user_id, token: body.invitation.token, email };
}

function totp(token: string, remoteAddress = address()) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/invitations/totp",
    remoteAddress,
    payload: { token },
  });
}

function complete(token: string, code: string, remoteAddress = address(), password = PASSWORD) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/invitations/complete",
    remoteAddress,
    payload: { token, password, totp_code: code },
  });
}

async function auditOf(target: string) {
  const r = await pool.query<{
    action: string;
    user_id: string | null;
    outcome: string;
    detail: Record<string, string> | null;
  }>("SELECT action, user_id, outcome, detail FROM audit_log WHERE target_id = $1 ORDER BY id", [
    target,
  ]);
  return r.rows;
}

beforeAll(async () => {
  config = inject("apiConfig");
  pool = new pg.Pool({ connectionString: config.API_PG_URL });
  apph = await buildTestApp(config);
  adminId = await insertUserDirect(config, { email: "admin@dashapi004.test", role: "admin" });
  await seedSession(config, { token: ADMIN, userId: adminId, role: "admin", orgId: ORG });
});
afterAll(async () => {
  await apph?.close();
  await pool?.end();
});

test("POST /v1/users answers a token once, kept as its SHA-256 alone, with user.create and invitation.issue", async () => {
  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/users",
    payload: { email: "  First@Dashapi004.test ", role: "viewer" },
    ...asAdmin,
  });
  expect(res.statusCode, res.body).toBe(201);
  expect(res.headers["cache-control"]).toBe("no-store");
  const body = res.json() as { user_id: string; invitation: { token: string; expires_at: string } };
  expect(Object.keys(body).sort()).toEqual(["invitation", "user_id"]);
  expect(Object.keys(body.invitation).sort()).toEqual(["expires_at", "token"]);
  // 32 bytes in base64url.
  expect(body.invitation.token).toMatch(/^[A-Za-z0-9_-]{43}$/);
  const user = await pool.query(
    "SELECT email, role, password_hash, totp_secret, totp_enrolled FROM users WHERE user_id = $1",
    [body.user_id],
  );
  expect(user.rows).toEqual([
    {
      email: "first@dashapi004.test",
      role: "viewer",
      password_hash: null,
      totp_secret: null,
      totp_enrolled: false,
    },
  ]);
  const inv = await pool.query<{
    token_sha256: Buffer;
    purpose: string;
    org_id: string;
    hours: string;
    expires_at: Date;
  }>(
    `SELECT token_sha256, purpose, org_id, expires_at,
            extract(epoch FROM expires_at - issued_at)::text AS hours
       FROM invitations WHERE user_id = $1`,
    [body.user_id],
  );
  expect(inv.rows).toHaveLength(1);
  expect(inv.rows[0]?.token_sha256.equals(sha256(body.invitation.token))).toBe(true);
  expect(inv.rows[0]?.purpose).toBe("invite");
  expect(inv.rows[0]?.org_id).toBe(ORG);
  expect(Number(inv.rows[0]?.hours)).toBe(72 * 3600);
  expect(new Date(body.invitation.expires_at).getTime()).toBe(inv.rows[0]?.expires_at.getTime());
  // No column of the api's tables holds the token's text.
  for (const table of ["users", "invitations", "audit_log"]) {
    const r = await pool.query<{ n: string }>(
      `SELECT count(*) AS n FROM ${table} t WHERE row_to_json(t)::text LIKE '%' || $1 || '%'`,
      [body.invitation.token],
    );
    expect(Number(r.rows[0]?.n), table).toBe(0);
  }
  expect(await auditOf(body.user_id)).toEqual([
    { action: "user.create", user_id: adminId, outcome: "ok", detail: { role: "viewer" } },
    {
      action: "invitation.issue",
      user_id: adminId,
      outcome: "ok",
      detail: { purpose: "invite", expires_at: body.invitation.expires_at },
    },
  ]);
});

test("the invitation's TOTP route answers the email and a new secret each call; complete takes the last one's code", async () => {
  const { userId, token, email } = await createUser();
  const first = await totp(token);
  expect(first.statusCode, first.body).toBe(200);
  expect(first.headers["cache-control"]).toBe("no-store");
  const a = first.json() as { email: string; provisioning_uri: string };
  expect(Object.keys(a).sort()).toEqual(["email", "provisioning_uri"]);
  expect(a.email).toBe(email);
  expect(a.provisioning_uri.startsWith("otpauth://totp/")).toBe(true);
  const second = await totp(token);
  expect(second.statusCode).toBe(200);
  const b = second.json() as { email: string; provisioning_uri: string };
  expect(secretOf(b.provisioning_uri)).not.toBe(secretOf(a.provisioning_uri));
  // The secret is kept encrypted on the invitation, never in clear.
  const stored = await pool.query<{ s: Buffer }>(
    "SELECT pending_totp_secret AS s FROM invitations WHERE user_id = $1",
    [userId],
  );
  expect(stored.rows[0]?.s.toString("latin1")).not.toContain(secretOf(b.provisioning_uri));

  // A code of the first secret: refused, with a denied row.
  const stale = await complete(token, codeAt(secretOf(a.provisioning_uri)));
  expect(stale.statusCode).toBe(401);
  expect(stale.json()).toEqual({ error: "invitation_rejected" });
  expect(stale.headers["cache-control"]).toBe("no-store");
  // A code of the second: the user's password and factor are set.
  const done = await complete(token, codeAt(secretOf(b.provisioning_uri)));
  expect(done.statusCode, done.body).toBe(204);
  expect(done.body).toBe("");
  const user = await pool.query<{
    hashed: boolean;
    totp_enrolled: boolean;
    authority_version: string;
  }>(
    `SELECT password_hash LIKE '$argon2id$%' AS hashed, totp_enrolled, authority_version::text
       FROM users WHERE user_id = $1`,
    [userId],
  );
  expect(user.rows).toEqual([{ hashed: true, totp_enrolled: true, authority_version: "1" }]);
  const inv = await pool.query(
    "SELECT completed_at IS NOT NULL AS done, pending_totp_secret FROM invitations WHERE user_id = $1",
    [userId],
  );
  expect(inv.rows).toEqual([{ done: true, pending_totp_secret: null }]);
  expect(
    (await auditOf(userId)).filter(
      (r) => r.action.startsWith("invitation.") && r.action !== "invitation.issue",
    ),
  ).toEqual([
    { action: "invitation.totp", user_id: userId, outcome: "ok", detail: { purpose: "invite" } },
    { action: "invitation.totp", user_id: userId, outcome: "ok", detail: { purpose: "invite" } },
    {
      action: "invitation.complete",
      user_id: userId,
      outcome: "denied",
      detail: { purpose: "invite" },
    },
    {
      action: "invitation.complete",
      user_id: userId,
      outcome: "ok",
      detail: { purpose: "invite" },
    },
  ]);
  // It issued no session.
  expect(done.headers["set-cookie"]).toBeUndefined();

  // The completion's code is used: it signs in no one.
  const replayed = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: address(),
    payload: { email, password: PASSWORD, totp_code: codeAt(secretOf(b.provisioning_uri)) },
  });
  expect(replayed.statusCode).toBe(401);
  // The user signs in with the chosen password and a code of that secret (the next step's: the
  // completion used the current one).
  const login = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: address(),
    payload: { email, password: PASSWORD, totp_code: codeAt(secretOf(b.provisioning_uri), 1) },
  });
  expect(login.statusCode, login.body).toBe(200);
  expect(login.headers["cache-control"]).toBe("no-store");
  // The completed invitation answers 401 on both routes.
  expect((await totp(token)).statusCode).toBe(401);
  expect((await complete(token, codeAt(secretOf(b.provisioning_uri)))).statusCode).toBe(401);
});

/** The rows written from one source address. */
async function rowsFrom(remoteAddress: string) {
  const r = await pool.query(
    "SELECT action, user_id, target_id, outcome, detail FROM audit_log WHERE source_ip = $1 ORDER BY id",
    [remoteAddress],
  );
  return r.rows;
}

/** A secret shown for `token`, and the token: an invitation its holder has opened. */
async function opened(): Promise<{ userId: string; token: string; secret: string }> {
  const user = await createUser();
  const shown = await totp(user.token);
  expect(shown.statusCode).toBe(200);
  const uri = (shown.json() as { provisioning_uri: string }).provisioning_uri;
  return { userId: user.userId, token: user.token, secret: secretOf(uri) };
}

/** Both routes refuse `inv`, the completion with a code of its secret, and write a denied row
 *  naming its user, and the user keeps no credentials. */
async function refusedOnBothRoutes(inv: { userId: string; token: string; secret: string }) {
  const t = await totp(inv.token);
  expect(t.statusCode).toBe(401);
  expect(t.json()).toEqual({ error: "invitation_rejected" });
  expect(t.headers["cache-control"]).toBe("no-store");
  const marker = address();
  const c = await complete(inv.token, codeAt(inv.secret), marker);
  expect(c.statusCode).toBe(401);
  expect(c.json()).toEqual({ error: "invitation_rejected" });
  expect(await rowsFrom(marker)).toEqual([
    {
      action: "invitation.complete",
      user_id: inv.userId,
      target_id: inv.userId,
      outcome: "denied",
      detail: { purpose: "invite" },
    },
  ]);
  const u = await pool.query("SELECT password_hash, totp_enrolled FROM users WHERE user_id = $1", [
    inv.userId,
  ]);
  expect(u.rows).toEqual([{ password_hash: null, totp_enrolled: false }]);
}

test("an unknown, expired, withdrawn or disabled user's invitation answers 401, one answer", async () => {
  const unknown = await totp("A".repeat(43));
  expect(unknown.statusCode).toBe(401);
  expect(unknown.json()).toEqual({ error: "invitation_rejected" });
  const unknownComplete = await complete("A".repeat(43), "123456");
  expect(unknownComplete.statusCode).toBe(401);
  expect(unknownComplete.json()).toEqual({ error: "invitation_rejected" });

  // Each opened first, so that the completion's code is one of the secret it holds.
  const expired = await opened();
  await pool.query(
    "UPDATE invitations SET issued_at = now() - interval '73 hours', expires_at = now() - interval '1 hour' WHERE user_id = $1",
    [expired.userId],
  );
  await refusedOnBothRoutes(expired);

  const withdrawn = await opened();
  const w = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${withdrawn.userId}/invitation/withdraw`,
    payload: {},
    ...asAdmin,
  });
  expect(w.statusCode, w.body).toBe(204);
  await refusedOnBothRoutes(withdrawn);

  // A disabled user's invitation left open (disabling through the route withdraws it, below).
  const disabled = await opened();
  await pool.query("UPDATE users SET status = 'disabled' WHERE user_id = $1", [disabled.userId]);
  await refusedOnBothRoutes(disabled);
});

test("a text that is not base64url is no invitation, even one that folds onto a real one", async () => {
  const { token } = await createUser();
  // Each character moved up by 256: as "ascii", Node would hash it as the real token.
  const folded = Array.from(token, (c) => String.fromCharCode(c.charCodeAt(0) + 256)).join("");
  for (const text of [folded, `${token} `, `${token}=`]) {
    const res = await totp(text);
    expect(res.statusCode, JSON.stringify(text)).toBe(401);
    expect(res.json()).toEqual({ error: "invitation_rejected" });
  }
  expect((await totp(token)).statusCode).toBe(200);
});

test("an unknown token's failed completion writes a denied row naming no user", async () => {
  const marker = address();
  const res = await complete("B".repeat(43), "123456", marker);
  expect(res.statusCode).toBe(401);
  const r = await pool.query(
    "SELECT action, user_id, target_id, outcome FROM audit_log WHERE source_ip = $1",
    [marker],
  );
  expect(r.rows).toEqual([
    { action: "invitation.complete", user_id: null, target_id: null, outcome: "denied" },
  ]);
});

test("disabling a user with an open invitation withdraws it, with an invitation.withdraw row", async () => {
  const { userId, token } = await createUser();
  await totp(token);
  const res = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/status`,
    payload: { status: "disabled" },
    ...asAdmin,
  });
  expect(res.statusCode, res.body).toBe(200);
  const inv = await pool.query(
    "SELECT withdrawn_at IS NOT NULL AS withdrawn, pending_totp_secret FROM invitations WHERE user_id = $1",
    [userId],
  );
  expect(inv.rows).toEqual([{ withdrawn: true, pending_totp_secret: null }]);
  const rows = (await auditOf(userId)).filter((r) => r.action === "invitation.withdraw");
  expect(rows).toEqual([
    {
      action: "invitation.withdraw",
      user_id: adminId,
      outcome: "ok",
      detail: { purpose: "invite" },
    },
  ]);
  expect((await totp(token)).statusCode).toBe(401);
});

test("the sixth failure for one invitation within 15 minutes answers 429, from any address", async () => {
  const { token } = await createUser();
  const shown = await totp(token);
  const secret = secretOf((shown.json() as { provisioning_uri: string }).provisioning_uri);
  for (let i = 0; i < 5; i += 1) {
    const res = await complete(token, codeAt(secret, 5));
    expect(res.statusCode, `failure ${i + 1}`).toBe(401);
  }
  const sixth = await complete(token, codeAt(secret));
  expect(sixth.statusCode).toBe(429);
  expect((await totp(token)).statusCode).toBe(429);
});

test("a new invitation for a user with no credentials withdraws the open one", async () => {
  const { userId, token } = await createUser();
  await totp(token);
  // An earlier invitation that expired keeps its secret until the user's next invitation.
  await pool.query(
    `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, issued_at, expires_at, pending_totp_secret, withdrawn_at)
     VALUES (gen_random_uuid(), $1, 'default', 'invite', '\\x0004', now() - interval '80 hours', now() - interval '8 hours', '\\x01', now())`,
    [userId],
  );
  const res = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/invitation`,
    payload: {},
    ...asAdmin,
  });
  expect(res.statusCode, res.body).toBe(201);
  expect(res.headers["cache-control"]).toBe("no-store");
  const body = res.json() as { token: string; expires_at: string };
  expect(Object.keys(body).sort()).toEqual(["expires_at", "token"]);
  expect(body.token).not.toBe(token);
  expect((await totp(token)).statusCode).toBe(401);
  expect((await totp(body.token)).statusCode).toBe(200);
  const inv = await pool.query<{ open: boolean; secret: boolean }>(
    `SELECT completed_at IS NULL AND withdrawn_at IS NULL AS open, pending_totp_secret IS NOT NULL AS secret
       FROM invitations WHERE user_id = $1 ORDER BY issued_at`,
    [userId],
  );
  // Every earlier invitation's secret is cleared; the new one holds the secret just shown.
  expect(inv.rows).toEqual([
    { open: false, secret: false },
    { open: false, secret: false },
    { open: true, secret: true },
  ]);
  const actions = (await auditOf(userId)).map((r) => [r.action, r.detail]);
  expect(actions.slice(-4)).toEqual([
    ["invitation.totp", { purpose: "invite" }],
    ["invitation.withdraw", { purpose: "invite" }],
    ["invitation.issue", { purpose: "invite", expires_at: body.expires_at }],
    ["invitation.totp", { purpose: "invite" }],
  ]);
});

test("a new invitation is refused to a user with credentials, or disabled; a withdrawal to one with none open", async () => {
  const withCredentials = await insertUserDirect(config, {
    email: "credentialed@dashapi004.test",
    role: "analyst",
  });
  const unconfirmed = await insertUserDirect(config, {
    email: "unconfirmed@dashapi004.test",
    role: "analyst",
    totpEnrolled: false,
  });
  for (const id of [withCredentials, unconfirmed]) {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${id}/invitation`,
      payload: {},
      ...asAdmin,
    });
    expect(res.statusCode).toBe(409);
    expect(res.json()).toEqual({ error: "user_has_credentials" });
  }
  const disabled = await createUser();
  await pool.query("UPDATE users SET status = 'disabled' WHERE user_id = $1", [disabled.userId]);
  const d = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${disabled.userId}/invitation`,
    payload: {},
    ...asAdmin,
  });
  expect(d.statusCode).toBe(409);
  expect(d.json()).toEqual({ error: "user_disabled" });

  const none = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${withCredentials}/invitation/withdraw`,
    payload: {},
    ...asAdmin,
  });
  expect(none.statusCode).toBe(409);
  expect(none.json()).toEqual({ error: "no_open_invitation" });
  // A user of another org, or none: 404, and the other org's user is left as it was.
  const foreign = globalThis.crypto.randomUUID();
  await pool.query(
    `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
     VALUES ($1, 'dashapi-004-other', 'foreign@dashapi004.test', NULL, NULL, false, 'viewer')`,
    [foreign],
  );
  for (const id of [foreign, globalThis.crypto.randomUUID()]) {
    for (const route of ["invitation", "invitation/withdraw"]) {
      const res = await apph.app.inject({
        method: "POST",
        url: `/v1/users/${id}/${route}`,
        payload: {},
        ...asAdmin,
      });
      expect(res.statusCode, route).toBe(404);
    }
  }
  expect(
    (await pool.query("SELECT 1 FROM invitations WHERE user_id = $1", [foreign])).rows,
  ).toEqual([]);
  expect(await auditOf(foreign)).toEqual([]);
});

test("ten failures from one source address within 15 minutes answer 429 to any invitation", async () => {
  const from = address();
  for (let i = 0; i < 10; i += 1) {
    const res = await totp(`${String(i).padStart(2, "0")}${"E".repeat(41)}`, from);
    expect(res.statusCode, `failure ${i + 1}`).toBe(401);
  }
  const { token } = await createUser();
  expect((await totp(token, from)).statusCode).toBe(429);
  // Another address reaches it.
  expect((await totp(token)).statusCode).toBe(200);
});

test("no route that takes the admin's session answers a TOTP secret", async () => {
  const { userId } = await createUser();
  const other = await insertUserDirect(config, {
    email: `answers-${globalThis.crypto.randomUUID()}@dashapi004.test`,
    role: "analyst",
  });
  const post = (url: string, payload: object) =>
    apph.app.inject({ method: "POST", url, payload, ...asAdmin });
  const answers = [
    await apph.app.inject({ method: "GET", url: "/v1/users", ...asAdmin }),
    await apph.app.inject({
      method: "POST",
      url: "/v1/users",
      payload: { email: "nosecret@dashapi004.test", role: "analyst" },
      ...asAdmin,
    }),
    await apph.app.inject({
      method: "POST",
      url: `/v1/users/${userId}/invitation`,
      payload: {},
      ...asAdmin,
    }),
    await post(`/v1/users/${userId}/invitation/withdraw`, {}),
    await apph.app.inject({ method: "GET", url: "/v1/users/assignable", ...asAdmin }),
    await apph.app.inject({ method: "GET", url: "/v1/auth/session", ...asAdmin }),
    await post(`/v1/users/${other}/role`, { role: "viewer" }),
    await post(`/v1/users/${other}/reset`, {}),
    await post(`/v1/users/${other}/status`, { status: "disabled" }),
  ];
  for (const res of answers) {
    expect(res.statusCode, res.body).toBeLessThan(300);
    expect(res.body).not.toMatch(/otpauth|secret/i);
  }
});
