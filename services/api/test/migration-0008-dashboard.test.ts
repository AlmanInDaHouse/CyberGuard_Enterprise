import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// SPEC-026 §Data contracts §2 — the api's migration `0008_dashboard`: a user's credentials
// that can be cleared, both or neither; the invitations; the audit view's index. And the last
// active admin of SPEC-008 Amendment 2026-10-15 item 10, which counts only an admin with a
// password and a confirmed factor.
//
// The structural assertions run inside BEGIN/ROLLBACK, so nothing persists in the Postgres
// the suite shares.

let client: pg.Client;
let config: Config;
const PASSPHRASE = "test-api-passphrase"; // backends.ts API_DB_ENC_PASSPHRASE

let n = 0;
function uid(): string {
  n += 1;
  return `01934abc-def0-7000-8b08-${String(n).padStart(12, "0")}`;
}

/** The SQLSTATE `body` fails with inside a rolled-back transaction, or null if it passes. */
async function sqlstate(body: () => Promise<unknown>): Promise<string | null> {
  await client.query("BEGIN");
  try {
    await body();
    return null;
  } catch (err) {
    return String((err as { code?: unknown }).code);
  } finally {
    await client.query("ROLLBACK");
  }
}

async function insertUser(opts: {
  userId?: string;
  password?: string | null;
  secret?: string | null;
  enrolled?: boolean;
}): Promise<string> {
  const userId = opts.userId ?? uid();
  await client.query(
    `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
     VALUES ($1, 'default', $2, $3, CASE WHEN $4::text IS NULL THEN NULL ELSE pgp_sym_encrypt($4, $5) END, $6, 'analyst')`,
    [
      userId,
      `m0008-${userId}@example.test`,
      opts.password === undefined ? "argon2-placeholder" : opts.password,
      opts.secret === undefined ? "JBSWY3DPEHPK3PXP" : opts.secret,
      PASSPHRASE,
      opts.enrolled ?? false,
    ],
  );
  return userId;
}

async function insertInvitation(
  userId: string,
  opts: {
    purpose?: string;
    token?: string;
    expires?: string;
    completed?: boolean;
    withdrawn?: boolean;
  } = {},
): Promise<void> {
  await client.query(
    `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, expires_at, completed_at, withdrawn_at)
     VALUES (gen_random_uuid(), $1, 'default', $2, sha256(convert_to($3, 'UTF8')), now() + $4::interval,
             CASE WHEN $5 THEN now() END, CASE WHEN $6 THEN now() END)`,
    [
      userId,
      opts.purpose ?? "invite",
      opts.token ?? `tok-${uid()}`,
      opts.expires ?? "72 hours",
      opts.completed ?? false,
      opts.withdrawn ?? false,
    ],
  );
}

beforeAll(async () => {
  config = inject("apiConfig");
  client = new pg.Client({ connectionString: config.API_PG_URL });
  await client.connect();
});
afterAll(async () => {
  await client?.end();
});

test("a user with neither a password nor a secret is accepted, unconfirmed", async () => {
  expect(await sqlstate(() => insertUser({ password: null, secret: null }))).toBeNull();
});

test("a user with one credential and not the other is refused", async () => {
  expect(await sqlstate(() => insertUser({ password: null }))).toBe("23514");
  expect(await sqlstate(() => insertUser({ secret: null }))).toBe("23514");
});

test("a confirmed factor without a secret is refused", async () => {
  expect(await sqlstate(() => insertUser({ password: null, secret: null, enrolled: true }))).toBe(
    "23514",
  );
});

test("an invitation: its purposes, its 72 hours, one end, a unique hash, a known user", async () => {
  expect(await sqlstate(async () => insertInvitation(await insertUser({})))).toBeNull();
  expect(
    await sqlstate(async () => insertInvitation(await insertUser({}), { purpose: "reset" })),
  ).toBeNull();
  expect(
    await sqlstate(async () => insertInvitation(await insertUser({}), { purpose: "welcome" })),
  ).toBe("23514");
  expect(
    await sqlstate(async () =>
      insertInvitation(await insertUser({}), { expires: "72 hours 1 second" }),
    ),
  ).toBe("23514");
  expect(
    await sqlstate(async () =>
      insertInvitation(await insertUser({}), { completed: true, withdrawn: true }),
    ),
  ).toBe("23514");
  expect(
    await sqlstate(async () => {
      await insertInvitation(await insertUser({}), { token: "same" });
      await insertInvitation(await insertUser({}), { token: "same" });
    }),
  ).toBe("23505");
  expect(await sqlstate(() => insertInvitation(uid()))).toBe("23503");
});

test("a user has at most one open invitation, and any number of ended ones", async () => {
  expect(
    await sqlstate(async () => {
      const user = await insertUser({});
      await insertInvitation(user);
      await insertInvitation(user);
    }),
  ).toBe("23505");
  expect(
    await sqlstate(async () => {
      const user = await insertUser({});
      await insertInvitation(user, { completed: true });
      await insertInvitation(user, { withdrawn: true });
      await insertInvitation(user, { withdrawn: true });
      await insertInvitation(user);
    }),
  ).toBeNull();
});

test("an invitation's issue time is the database's", async () => {
  await client.query("BEGIN");
  try {
    const user = await insertUser({});
    await insertInvitation(user);
    const r = await client.query<{ same: boolean }>(
      "SELECT issued_at = now() AS same FROM invitations WHERE user_id = $1",
      [user],
    );
    expect(r.rows).toEqual([{ same: true }]);
  } finally {
    await client.query("ROLLBACK");
  }
});

test("audit_log has an index on (org_id, id)", async () => {
  const r = await client.query<{ def: string }>(
    "SELECT indexdef AS def FROM pg_indexes WHERE tablename = 'audit_log' AND indexname = 'audit_log_org_id'",
  );
  expect(r.rows).toHaveLength(1);
  expect(r.rows[0]?.def).toMatch(/\(org_id, id\)$/);
});

// SPEC-026 §Data contracts §2: the last active admin counts only an admin with a password and
// a confirmed factor, so that an admin whose credentials were cleared does not count.
test("an active admin without credentials does not count as the last admin's peer", async () => {
  const org = "m0008-last-admin";
  const apph = await buildTestApp(config);
  try {
    const admin = await insertUserDirect(config, {
      email: "counted@m0008.test",
      role: "admin",
      orgId: org,
    });
    const cleared = uid();
    await client.query(
      `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
       VALUES ($1, $2, 'cleared@m0008.test', NULL, NULL, false, 'admin')`,
      [cleared, org],
    );
    // An unconfirmed admin with a password counts no more than one with neither.
    const unconfirmed = await insertUserDirect(config, {
      email: "unconfirmed@m0008.test",
      role: "admin",
      orgId: org,
      totpEnrolled: false,
    });
    await seedSession(config, {
      token: "tok-m0008-admin",
      userId: admin,
      role: "admin",
      orgId: org,
    });
    const as = {
      cookies: { cgsess: "tok-m0008-admin" },
      headers: { "x-csrf-token": "csrf-tok-m0008-admin" },
    };
    for (const [route, payload] of [
      ["role", { role: "analyst" }],
      ["status", { status: "disabled" }],
    ] as const) {
      const res = await apph.app.inject({
        method: "POST",
        url: `/v1/users/${admin}/${route}`,
        payload,
        ...as,
      });
      expect(res.statusCode, route).toBe(409);
    }
    // Removing an admin who does not count removes no admin that counts.
    for (const target of [cleared, unconfirmed]) {
      const res = await apph.app.inject({
        method: "POST",
        url: `/v1/users/${target}/status`,
        payload: { status: "disabled" },
        ...as,
      });
      expect(res.statusCode, target).toBe(200);
    }
    const r = await client.query<{ email: string; role: string; status: string }>(
      "SELECT email, role, status FROM users WHERE org_id = $1 ORDER BY email",
      [org],
    );
    expect(r.rows).toEqual([
      { email: "cleared@m0008.test", role: "admin", status: "disabled" },
      { email: "counted@m0008.test", role: "admin", status: "active" },
      { email: "unconfirmed@m0008.test", role: "admin", status: "disabled" },
    ]);
  } finally {
    await apph.close();
  }
});
