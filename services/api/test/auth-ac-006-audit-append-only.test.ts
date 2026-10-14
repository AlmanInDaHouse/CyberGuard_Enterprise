import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { buildTestApp, getAuditRows, insertUserDirect, seedSession } from "./helpers/db.js";

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

// SPEC-008 Amendment 2026-10-15 item 6: the values of the rows. A role change's row
// names the admin, the session's org, the target, the request's id as it appears on
// the request's log lines, the source address and the roles; a user.create row its
// role; a login row its request id and address. A request id the client sends is
// ignored.
describe("the rows' values", () => {
  const lines: string[] = [];
  let app: Awaited<ReturnType<typeof buildApp>>;
  let services: Awaited<ReturnType<typeof buildServices>>;
  const ORG = "auth-ac-006-org";

  beforeAll(async () => {
    services = await buildServices(config);
    app = buildApp(services, {
      level: "info",
      stream: { write: (line: string) => lines.push(line) },
    });
    await app.ready();
  });
  afterAll(async () => {
    await app?.close();
    await services?.close();
  });

  /** The reqId Fastify logged for the request to `method url`. */
  const reqIdOf = (method: string, url: string) =>
    lines
      .map(
        (l) =>
          JSON.parse(l) as {
            reqId?: string;
            msg?: string;
            req?: { method?: string; url?: string };
          },
      )
      .find((l) => l.msg === "incoming request" && l.req?.method === method && l.req?.url === url)
      ?.reqId;

  test("a role change's row and a user.create row", async () => {
    const adminId = await insertUserDirect(config, {
      email: "admin@ac006.test",
      role: "admin",
      orgId: ORG,
    });
    await seedSession(config, {
      token: "tok-ac006-admin",
      userId: adminId,
      role: "admin",
      orgId: ORG,
    });
    const as = {
      cookies: { cgsess: "tok-ac006-admin" },
      headers: {
        "x-csrf-token": "csrf-tok-ac006-admin",
        "request-id": "forged",
        "x-request-id": "forged",
      },
      remoteAddress: "198.51.100.66",
    };
    const created = await app.inject({
      method: "POST",
      url: "/v1/users",
      payload: { email: "made@ac006.test", password: "Valid-passw0rd-006", role: "viewer" },
      ...as,
    });
    expect(created.statusCode).toBe(201);
    const { userId } = created.json() as { userId: string };
    const changed = await app.inject({
      method: "POST",
      url: `/v1/users/${userId}/role`,
      payload: { role: "analyst" },
      ...as,
    });
    expect(changed.statusCode).toBe(200);

    const rows = await auditRowsFor(userId);
    expect(rows.map((r) => r.action)).toEqual(["user.create", "role.change"]);
    for (const row of rows) {
      expect(row.user_id).toBe(adminId);
      expect(row.org_id).toBe(ORG);
      expect(row.source_ip).toBe("198.51.100.66");
      expect(row.correlation_id).toMatch(UUID);
      expect(row.correlation_id).not.toBe("forged");
    }
    expect(rows[0]?.correlation_id).toBe(reqIdOf("POST", "/v1/users"));
    expect(rows[1]?.correlation_id).toBe(reqIdOf("POST", `/v1/users/${userId}/role`));
    expect(rows[0]?.detail).toEqual({ role: "viewer" });
    expect(rows[1]?.detail).toEqual({ from: "viewer", to: "analyst" });
  });

  test("a login's row carries its request id and address", async () => {
    await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.51.100.67",
      payload: { email: "nobody-006@ac006.test", password: "wrong", totp_code: "000000" },
    });
    const [row] = await auditRowsWhere("source_ip = '198.51.100.67'");
    expect(row?.action).toBe("auth.login.fail");
    expect(row?.correlation_id).toMatch(UUID);
    expect(row?.correlation_id).toBe(reqIdOf("POST", "/v1/auth/login"));
  });
});

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

interface FullAuditRow {
  action: string;
  user_id: string | null;
  org_id: string;
  target_id: string | null;
  source_ip: string | null;
  correlation_id: string | null;
  detail: unknown;
}

async function auditRowsWhere(where: string, values: unknown[] = []): Promise<FullAuditRow[]> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<FullAuditRow>(
      `SELECT action, user_id::text, org_id, target_id, host(source_ip) AS source_ip,
              correlation_id::text, detail
         FROM audit_log WHERE ${where} ORDER BY id`,
      values,
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

function auditRowsFor(targetId: string): Promise<FullAuditRow[]> {
  return auditRowsWhere("target_id = $1", [targetId]);
}
