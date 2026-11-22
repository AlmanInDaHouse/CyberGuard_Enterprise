import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// dashapi_ac_006 (SPEC-026 §Data contracts §3, *Users and invitations*) — the users' lists.
// `GET /v1/users` pages through 205 users, 200 and 5, in the order `(created_at, user_id)`, each
// with its id, email, role, status, `totp_enrolled` and its open invitation; `/assignable` lists
// the org's active admins and analysts with a confirmed factor, in the order `(email, user_id)`,
// not a viewer, a disabled user or one with an open invitation.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
let pool: pg.Pool;
const ORG = "dashapi-006";
const ADMIN = "tok-dashapi006-admin";
const ANALYST = "tok-dashapi006-analyst";
const VIEWER = "tok-dashapi006-viewer";
const as = (t: string) => ({ cookies: { cgsess: t }, headers: { "x-csrf-token": `csrf-${t}` } });

interface UserItem {
  user_id: string;
  email: string;
  role: string;
  status: string;
  totp_enrolled: boolean;
  invitation: { purpose: string; expires_at: string; expired: boolean } | null;
}

async function page(url: string, token = ADMIN) {
  const res = await apph.app.inject({ method: "GET", url, ...as(token) });
  expect(res.statusCode, res.body).toBe(200);
  expect(res.headers["cache-control"]).toBe("no-store");
  return res.json() as { items: UserItem[]; next_cursor: string | null };
}

/** Insert `count` users of `org` in one statement: the same created_at, ordered by user_id. */
async function bulk(org: string, count: number, prefix: string, role = "viewer"): Promise<void> {
  await pool.query(
    `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
     SELECT gen_random_uuid(), $1, $2 || g || '@dashapi006.test', 'argon2-placeholder',
            pgp_sym_encrypt('JBSWY3DPEHPK3PXP', $3), true, $4
       FROM generate_series(1, $5) AS g`,
    [org, prefix, config.API_DB_ENC_PASSPHRASE, role, count],
  );
}

beforeAll(async () => {
  config = inject("apiConfig");
  pool = new pg.Pool({ connectionString: config.API_PG_URL });
  apph = await buildTestApp(config);
  const admin = await insertUserDirect(config, {
    email: "admin@dashapi006.test",
    role: "admin",
    orgId: ORG,
  });
  await seedSession(config, { token: ADMIN, userId: admin, role: "admin", orgId: ORG });
  const analyst = await insertUserDirect(config, {
    email: "analyst@dashapi006.test",
    role: "analyst",
    orgId: ORG,
  });
  await seedSession(config, { token: ANALYST, userId: analyst, role: "analyst", orgId: ORG });
  const viewer = await insertUserDirect(config, {
    email: "viewer@dashapi006.test",
    role: "viewer",
    orgId: ORG,
  });
  await seedSession(config, { token: VIEWER, userId: viewer, role: "viewer", orgId: ORG });
  // 205 users in all.
  await bulk(ORG, 202, "u");
});
afterAll(async () => {
  await apph?.close();
  await pool?.end();
});

test("GET /v1/users pages through 205 users, 200 and 5, in (created_at, user_id) order", async () => {
  const first = await page("/v1/users?limit=200");
  expect(first.items).toHaveLength(200);
  expect(first.next_cursor).not.toBeNull();
  const second = await page(`/v1/users?limit=200&cursor=${first.next_cursor}`);
  expect(second.items).toHaveLength(5);
  expect(second.next_cursor).toBeNull();
  const expected = await pool.query<{ user_id: string }>(
    "SELECT user_id FROM users WHERE org_id = $1 ORDER BY created_at, user_id",
    [ORG],
  );
  expect([...first.items, ...second.items].map((u) => u.user_id)).toEqual(
    expected.rows.map((r) => r.user_id),
  );
  // The default page is 50, and a limit above 200 reads 200.
  expect((await page("/v1/users")).items).toHaveLength(50);
  expect((await page("/v1/users?limit=500")).items).toHaveLength(200);
});

test("each user's fields, and its open invitation with whether it expired", async () => {
  const org = "dashapi-006-fields";
  const admin = await insertUserDirect(config, {
    email: "admin@fields.dashapi006.test",
    role: "admin",
    orgId: org,
  });
  await seedSession(config, { token: "tok-006-fields", userId: admin, role: "admin", orgId: org });
  const ids: string[] = [];
  for (const name of ["live", "expired", "completed"]) {
    const id = globalThis.crypto.randomUUID();
    ids.push(id);
    await pool.query(
      `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
       VALUES ($1, $2, $3, NULL, NULL, false, 'analyst')`,
      [id, org, `${name}@fields.dashapi006.test`],
    );
  }
  await pool.query(
    `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, issued_at, expires_at, completed_at)
     VALUES (gen_random_uuid(), $1, $4, 'invite', '\\x060001', now(), now() + interval '72 hours', NULL),
            (gen_random_uuid(), $2, $4, 'reset', '\\x060002', now() - interval '80 hours', now() - interval '8 hours', NULL),
            (gen_random_uuid(), $3, $4, 'invite', '\\x060003', now(), now() + interval '72 hours', now())`,
    [ids[0], ids[1], ids[2], org],
  );
  const res = await page("/v1/users", "tok-006-fields");
  const byEmail = new Map(res.items.map((u) => [u.email, u]));
  const live = byEmail.get("live@fields.dashapi006.test");
  expect(Object.keys(live ?? {}).sort()).toEqual([
    "email",
    "invitation",
    "role",
    "status",
    "totp_enrolled",
    "user_id",
  ]);
  expect(live).toMatchObject({
    user_id: ids[0],
    role: "analyst",
    status: "active",
    totp_enrolled: false,
    invitation: { purpose: "invite", expired: false },
  });
  expect(Object.keys(live?.invitation ?? {}).sort()).toEqual(["expired", "expires_at", "purpose"]);
  expect(byEmail.get("expired@fields.dashapi006.test")?.invitation).toMatchObject({
    purpose: "reset",
    expired: true,
  });
  expect(byEmail.get("completed@fields.dashapi006.test")?.invitation).toBeNull();
  expect(byEmail.get("admin@fields.dashapi006.test")).toMatchObject({
    totp_enrolled: true,
    invitation: null,
  });
  expect(res.next_cursor).toBeNull();
});

test("/assignable lists the active admins and analysts with a confirmed factor, by email", async () => {
  const org = "dashapi-006-assignable";
  const make = (email: string, role: "admin" | "analyst" | "viewer", extra = {}) =>
    insertUserDirect(config, {
      email: `${email}@assign.dashapi006.test`,
      role,
      orgId: org,
      ...extra,
    });
  const analyst = await make("c-analyst", "analyst");
  await seedSession(config, {
    token: "tok-006-assign",
    userId: analyst,
    role: "analyst",
    orgId: org,
  });
  const admin = await make("a-admin", "admin");
  await make("b-viewer", "viewer");
  await make("d-disabled", "analyst", { status: "disabled" });
  await make("e-unconfirmed", "analyst", { totpEnrolled: false });
  const invited = globalThis.crypto.randomUUID();
  await pool.query(
    `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
     VALUES ($1, $2, 'f-invited@assign.dashapi006.test', NULL, NULL, false, 'analyst')`,
    [invited, org],
  );
  await pool.query(
    `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, expires_at)
     VALUES (gen_random_uuid(), $1, $2, 'invite', '\\x060004', now() + interval '1 hour')`,
    [invited, org],
  );
  const res = await apph.app.inject({
    method: "GET",
    url: "/v1/users/assignable",
    ...as("tok-006-assign"),
  });
  expect(res.statusCode, res.body).toBe(200);
  expect(res.headers["cache-control"]).toBe("no-store");
  expect(res.json()).toEqual({
    items: [
      { user_id: admin, email: "a-admin@assign.dashapi006.test" },
      { user_id: analyst, email: "c-analyst@assign.dashapi006.test" },
    ],
    next_cursor: null,
  });
});

test("/assignable pages in (email, user_id) order", async () => {
  const org = "dashapi-006-assign-pages";
  const admin = await insertUserDirect(config, {
    email: "zz-admin@pages.dashapi006.test",
    role: "admin",
    orgId: org,
  });
  await seedSession(config, { token: "tok-006-pages", userId: admin, role: "admin", orgId: org });
  await bulk(org, 4, "pg", "analyst");
  const seen: string[] = [];
  let cursor: string | null = null;
  do {
    const url: string = `/v1/users/assignable?limit=2${cursor ? `&cursor=${cursor}` : ""}`;
    const res = await apph.app.inject({ method: "GET", url, ...as("tok-006-pages") });
    expect(res.statusCode, res.body).toBe(200);
    const body = res.json() as { items: { email: string }[]; next_cursor: string | null };
    expect(body.items.length).toBeLessThanOrEqual(2);
    seen.push(...body.items.map((i) => i.email));
    cursor = body.next_cursor;
  } while (cursor);
  expect(seen).toEqual([
    "pg1@dashapi006.test",
    "pg2@dashapi006.test",
    "pg3@dashapi006.test",
    "pg4@dashapi006.test",
    "zz-admin@pages.dashapi006.test",
  ]);
});

test("a cursor that does not decode is a 400", async () => {
  for (const url of ["/v1/users?cursor=not-a-cursor", "/v1/users/assignable?cursor=e30"]) {
    const res = await apph.app.inject({ method: "GET", url, ...as(ADMIN) });
    expect(res.statusCode, url).toBe(400);
  }
});

test("a viewer is refused /assignable; an analyst is refused the list", async () => {
  const v = await apph.app.inject({ method: "GET", url: "/v1/users/assignable", ...as(VIEWER) });
  expect(v.statusCode).toBe(403);
  const a = await apph.app.inject({ method: "GET", url: "/v1/users", ...as(ANALYST) });
  expect(a.statusCode).toBe(403);
});
