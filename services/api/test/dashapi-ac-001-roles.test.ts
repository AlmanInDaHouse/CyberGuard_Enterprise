import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// dashapi_ac_001 (SPEC-026 §Operational §1; blocks CI) — the roles. For each route of §Data
// contracts §3 and each of the three roles, a request with a valid session and CSRF token answers
// as §Operational §1 says: a capability the role lacks answers 403 with an `access.denied` row and
// changes nothing; every route but the login, the TOTP confirmation, the invitation's two and
// `/health` answers 401 without a session, and every mutation but the login, the TOTP
// confirmation and the invitation's two answers 403 without the CSRF token. Every read route of
// SPEC-009, SPEC-010 and SPEC-022 answers 200 to each of the three roles, but the forensic export
// and report, which answer 200 to an admin and an analyst and 403 to a viewer.
//
// Every route of §3 answers `Cache-Control: no-store`, its 401 and 403 included.
//
// The table grows with each route SPEC-026 lands.

type Role = "admin" | "analyst" | "viewer";
const ROLES: Role[] = ["admin", "analyst", "viewer"];
const ALL: Role[] = ROLES;
const ADMIN: Role[] = ["admin"];
const ORG = "dashapi-001";

interface Route {
  method: "GET" | "POST";
  /** The route's pattern; `:id` is the user routes' target, or the read routes' unknown
   *  incident. */
  path: string;
  roles: Role[];
  mutation: boolean;
  body?: unknown;
  /** For a route whose body here is refused past the role check (a wrong current password),
   *  the denied row that shows the request passed it. */
  refusedPast?: string;
  /** What an allowed role's request answers here. */
  expect?: number;
}

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
let pool: pg.Pool;
const ids: Record<Role, string> = { admin: "", analyst: "", viewer: "" };
/** A user of the org for the `:id` of the user routes. */
let target = "";
let incident = "";

const SECTION_3: Route[] = [
  { method: "GET", path: "/v1/auth/session", roles: ALL, mutation: false, expect: 200 },
  {
    method: "POST",
    path: "/v1/auth/password",
    roles: ALL,
    mutation: true,
    body: { currentPassword: "x", newPassword: "Another-passw0rd-1", totp_code: "000000" },
    refusedPast: "user.password.change",
  },
  { method: "GET", path: "/v1/users", roles: ADMIN, mutation: false, expect: 200 },
  {
    method: "GET",
    path: "/v1/users/assignable",
    roles: ["admin", "analyst"],
    mutation: false,
    expect: 200,
  },
  {
    method: "POST",
    path: "/v1/users",
    roles: ADMIN,
    mutation: true,
    body: { email: `created-${globalThis.crypto.randomUUID()}@dashapi001.test`, role: "viewer" },
    expect: 201,
  },
  {
    method: "POST",
    path: "/v1/users/:id/role",
    roles: ADMIN,
    mutation: true,
    body: { role: "viewer" },
    expect: 200,
  },
  {
    method: "POST",
    path: "/v1/users/:id/status",
    roles: ADMIN,
    mutation: true,
    body: { status: "active" },
    expect: 200,
  },
  {
    method: "POST",
    path: "/v1/users/:id/invitation",
    roles: ADMIN,
    mutation: true,
    body: {},
    expect: 409,
  },
  {
    method: "POST",
    path: "/v1/users/:id/reset",
    roles: ADMIN,
    mutation: true,
    body: {},
    expect: 201,
  },
  {
    method: "POST",
    path: "/v1/users/:id/invitation/withdraw",
    roles: ADMIN,
    mutation: true,
    body: {},
    expect: 204,
  },
];

const READS: Route[] = [
  { method: "GET", path: "/v1/incidents", roles: ALL, mutation: false },
  { method: "GET", path: "/v1/incidents/:id", roles: ALL, mutation: false },
  { method: "GET", path: "/v1/incidents/:id/events", roles: ALL, mutation: false },
  {
    method: "GET",
    path: "/v1/incidents/:id/playbook-executions",
    roles: ALL,
    mutation: false,
  },
  { method: "GET", path: "/v1/alerts", roles: ALL, mutation: false },
  {
    method: "GET",
    path: "/v1/incidents/:id/forensic-export",
    roles: ["admin", "analyst"],
    mutation: false,
  },
  {
    method: "GET",
    path: "/v1/incidents/:id/report",
    roles: ["admin", "analyst"],
    mutation: false,
  },
];

function token(role: Role): string {
  return `tok-dashapi001-${role}`;
}

function urlOf(route: Route): string {
  return route.path.replace(":id", route.path.startsWith("/v1/users") ? target : incident);
}

function send(route: Route, opts: { role?: Role; csrf?: boolean; url?: string }) {
  const cookies = opts.role ? { cgsess: token(opts.role) } : undefined;
  const headers =
    opts.role && opts.csrf !== false ? { "x-csrf-token": `csrf-${token(opts.role)}` } : undefined;
  return apph.app.inject({
    method: route.method,
    url: opts.url ?? urlOf(route),
    cookies,
    headers,
    ...(route.method === "POST" ? { payload: route.body ?? {} } : {}),
  });
}

/** The rows a refusal must leave: the counts of the tables a mutation changes. */
async function snapshot(): Promise<string> {
  const r = await pool.query<{ users: string; invitations: string; changed: string }>(
    `SELECT (SELECT count(*) FROM users)::text AS users,
            (SELECT count(*) FROM invitations)::text AS invitations,
            (SELECT max(updated_at)::text FROM users) AS changed,
            (SELECT string_agg(user_id::text || role || status || totp_enrolled::text, ',' ORDER BY user_id)
               FROM users WHERE org_id = $1) AS org_users,
            (SELECT string_agg(coalesce(withdrawn_at::text, '-') || coalesce(completed_at::text, '-'), ',' ORDER BY invitation_id)
               FROM invitations) AS ends`,
    [ORG],
  );
  return JSON.stringify(r.rows[0]);
}

async function auditSince(id: string) {
  const r = await pool.query<{ action: string; user_id: string; detail: Record<string, string> }>(
    "SELECT action, user_id, detail FROM audit_log WHERE id > $1 ORDER BY id",
    [id],
  );
  return r.rows;
}

async function lastAuditId(): Promise<string> {
  const r = await pool.query<{ id: string | null }>("SELECT max(id)::text AS id FROM audit_log");
  return r.rows[0]?.id ?? "0";
}

beforeAll(async () => {
  config = inject("apiConfig");
  pool = new pg.Pool({ connectionString: config.API_PG_URL });
  apph = await buildTestApp(config);
  for (const role of ROLES) {
    ids[role] = await insertUserDirect(config, {
      email: `${role}@dashapi001.test`,
      role,
      orgId: ORG,
    });
    await seedSession(config, { token: token(role), userId: ids[role], role, orgId: ORG });
  }
  target = await insertUserDirect(config, {
    email: "target@dashapi001.test",
    role: "viewer",
    orgId: ORG,
  });
  incident = globalThis.crypto.randomUUID();
});
afterAll(async () => {
  await apph?.close();
  await pool?.end();
});

describe("the routes of §Data contracts §3", () => {
  for (const route of SECTION_3) {
    const label = `${route.method} ${route.path}`;

    test(`${label}: 401 without a session, no-store`, async () => {
      const res = await send(route, {});
      expect(res.statusCode).toBe(401);
      expect(res.headers["cache-control"]).toBe("no-store");
    });

    if (route.mutation) {
      test(`${label}: 403 without the CSRF token, no-store`, async () => {
        const role = route.roles[0] as Role;
        const before = await snapshot();
        const res = await send(route, { role, csrf: false });
        expect(res.statusCode).toBe(403);
        expect(res.headers["cache-control"]).toBe("no-store");
        expect(await snapshot()).toBe(before);
      });
    }

    for (const role of ROLES) {
      if (route.roles.includes(role)) {
        test(`${label}: ${role} passes the role check, no-store`, async () => {
          const mark = await lastAuditId();
          const res = await send(route, { role });
          expect(res.headers["cache-control"]).toBe("no-store");
          if (route.refusedPast) {
            // Refused for its credentials, after the session, role and CSRF checks.
            expect(res.statusCode).toBe(401);
            expect(await auditSince(mark)).toEqual([
              { action: route.refusedPast, user_id: ids[role], detail: null },
            ]);
            return;
          }
          expect(res.statusCode, res.body).toBe(route.expect);
        });
      } else {
        test(`${label}: ${role} is refused, 403 with access.denied, nothing changed, no-store`, async () => {
          const before = await snapshot();
          const mark = await lastAuditId();
          const url = urlOf(route);
          const res = await send(route, { role, url });
          expect(res.statusCode).toBe(403);
          expect(res.json()).toEqual({ error: "forbidden" });
          expect(res.headers["cache-control"]).toBe("no-store");
          expect(await snapshot()).toBe(before);
          expect(await auditSince(mark)).toEqual([
            {
              action: "access.denied",
              user_id: ids[role],
              detail: { reason: "role", route: `${route.method} ${route.path}` },
            },
          ]);
        });
      }
    }
  }
});

describe("the routes that take no session", () => {
  test.each([
    ["/v1/auth/login", { email: "x@dashapi001.test", password: "x", totp_code: "000000" }],
    ["/v1/auth/totp/confirm", { userId: globalThis.crypto.randomUUID(), totp_code: "000000" }],
    ["/v1/invitations/totp", { token: "C".repeat(43) }],
    [
      "/v1/invitations/complete",
      { token: "C".repeat(43), password: "Valid-passw0rd-001", totp_code: "000000" },
    ],
  ])(
    "%s answers without a session or a CSRF token: 401 for bad credentials, never 403",
    async (url, payload) => {
      const res = await apph.app.inject({
        method: "POST",
        url,
        remoteAddress: "198.51.101.1",
        payload,
      });
      expect(res.statusCode).toBe(401);
      expect(res.headers["cache-control"]).toBe("no-store");
    },
  );

  test("/health answers without a session", async () => {
    const res = await apph.app.inject({ method: "GET", url: "/health" });
    expect(res.statusCode).toBe(200);
  });
});

describe("the read routes of SPEC-009, SPEC-010 and SPEC-022", () => {
  for (const route of READS) {
    const label = `${route.method} ${route.path}`;
    test(`${label}: 401 without a session`, async () => {
      expect((await send(route, {})).statusCode).toBe(401);
    });
    for (const role of ROLES) {
      if (route.roles.includes(role)) {
        test(`${label}: ${role} passes the role check`, async () => {
          const res = await send(route, { role });
          // An unknown incident is a 404 past the role check; the lists answer 200.
          expect(res.statusCode, res.body).toBe(route.path.includes(":id") ? 404 : 200);
        });
      } else {
        test(`${label}: ${role} is refused, 403 with access.denied`, async () => {
          const mark = await lastAuditId();
          const res = await send(route, { role });
          expect(res.statusCode).toBe(403);
          expect((await auditSince(mark)).map((r) => r.action)).toEqual(["access.denied"]);
        });
      }
    }
  }
});
