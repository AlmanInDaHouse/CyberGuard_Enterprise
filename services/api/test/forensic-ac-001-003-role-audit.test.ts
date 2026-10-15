import { createHash } from "node:crypto";
import type { FastifyInstance } from "fastify";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// forensic_ac_001 to 003 — SPEC-009's third Amendment 2026-10-15 (debt #90): the forensic
// export and report answer only an admin and an analyst; each document produced writes one
// audit row, the export's with its event count and chain head; and none is produced when
// its row cannot be written.

const ORG = "forensic-ac";
const PEER = "198.51.100.230";

let config: Config;
const closers: (() => Promise<void>)[] = [];
const logLines: string[] = [];

/** The reqId the app logged for the request to `url`, among the lines from `from` on. */
function reqIdOf(url: string, from: number): string | undefined {
  return logLines
    .slice(from)
    .map((l) => JSON.parse(l) as { reqId?: string; msg?: string; req?: { url?: string } })
    .find((l) => l.msg === "incoming request" && l.req?.url === url)?.reqId;
}
let app: FastifyInstance;
let auditRefused: FastifyInstance;
let incidentId: string;

async function appWith(refuseAudit: boolean): Promise<FastifyInstance> {
  const services = await buildServices(config);
  const pool = services.pg;
  // A pool that fails the audit row alone: every read the documents need still answers.
  const refusing = new Proxy(pool, {
    get(target, prop, receiver) {
      if (prop !== "query") return Reflect.get(target, prop, receiver);
      return (text: unknown, ...rest: unknown[]) =>
        typeof text === "string" && text.trimStart().startsWith("INSERT INTO audit_log")
          ? Promise.reject(new Error("the audit row is refused"))
          : (target.query as (...a: unknown[]) => unknown).call(target, text, ...rest);
    },
  });
  const built = buildApp(refuseAudit ? { ...services, pg: refusing } : services, {
    level: "info",
    stream: { write: (line: string) => logLines.push(line) },
  });
  await built.ready();
  closers.push(async () => {
    await built.close();
    await services.close();
  });
  return built;
}

async function session(
  role: "admin" | "analyst" | "viewer",
): Promise<{ token: string; userId: string }> {
  const token = `tok-forensic-${role}-${globalThis.crypto.randomUUID()}`;
  const userId = globalThis.crypto.randomUUID();
  await seedSession(config, { token, userId, role, orgId: ORG });
  return { token, userId };
}

function get(target: FastifyInstance, token: string, path: string) {
  return target.inject({
    method: "GET",
    url: path,
    cookies: { cgsess: token },
    remoteAddress: PEER,
  });
}

interface ForensicRow {
  user_id: string | null;
  org_id: string;
  action: string;
  target_id: string | null;
  outcome: string;
  source_ip: string | null;
  correlation_id: string | null;
  detail: Record<string, string> | null;
}

async function forensicRows(userId: string): Promise<ForensicRow[]> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<ForensicRow>(
      `SELECT user_id::text, org_id, action, target_id, outcome, host(source_ip) AS source_ip,
              correlation_id::text, detail
         FROM audit_log WHERE action LIKE 'forensic.%' AND user_id = $1 ORDER BY id`,
      [userId],
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

/** Every audit row of a user, whatever its action. */
async function anyRows(userId: string): Promise<number> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query("SELECT 1 FROM audit_log WHERE user_id = $1", [userId]);
    return r.rowCount ?? 0;
  } finally {
    await pool.end();
  }
}

beforeAll(async () => {
  config = inject("apiConfig");
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  incidentId = globalThis.crypto.randomUUID();
  const events = [globalThis.crypto.randomUUID(), globalThis.crypto.randomUUID()];
  await ensureAgent(config, agentId, ORG);
  for (const [i, eventId] of events.entries()) {
    await insertCgesEvent(config, {
      agentId,
      eventId,
      orgId: ORG,
      time: `2026-06-05 11:00:0${i}.000000000`,
      processPid: 6000 + i,
    });
  }
  await insertAlertRow(config, { alertId, agentId, orgId: ORG, sourceEvents: events });
  await insertIncidentRow(config, { incidentId, agentId, orgId: ORG, alertIds: [alertId] });
  app = await appWith(false);
  auditRefused = await appWith(true);
});
afterAll(async () => {
  for (const close of closers) await close();
});

const EXPORT = () => `/v1/incidents/${incidentId}/forensic-export`;
const REPORT = () => `/v1/incidents/${incidentId}/report`;

describe("forensic_ac_001 — the role", () => {
  test.each([
    ["the export of an incident of its org", EXPORT],
    ["the report of an incident of its org", REPORT],
    [
      "the export of an id no incident has",
      () => `/v1/incidents/${globalThis.crypto.randomUUID()}/forensic-export`,
    ],
    [
      "the report of an id no incident has",
      () => `/v1/incidents/${globalThis.crypto.randomUUID()}/report`,
    ],
    ["the export of a malformed id", () => "/v1/incidents/not-an-id/forensic-export"],
    ["the report of a malformed id", () => "/v1/incidents/not-an-id/report"],
  ])("a viewer's request for %s answers 403 and writes no row", async (_name, path) => {
    const viewer = await session("viewer");
    const res = await get(app, viewer.token, path());
    expect(res.statusCode).toBe(403);
    expect(res.json()).toEqual({ error: "forbidden" });
    expect(await anyRows(viewer.userId)).toBe(0);
  });

  test.each(["admin", "analyst"] as const)("an %s's export and report answer 200", async (role) => {
    const who = await session(role);
    expect((await get(app, who.token, EXPORT())).statusCode).toBe(200);
    const report = await get(app, who.token, REPORT());
    expect(report.statusCode).toBe(200);
    expect(report.headers["content-type"]).toContain("application/pdf");
  });

  test.each([
    ["export", EXPORT],
    ["report", REPORT],
  ])("the %s does not answer HEAD with 200, and writes no row for it", async (_name, path) => {
    const analyst = await session("analyst");
    const res = await app.inject({
      method: "HEAD",
      url: path(),
      cookies: { cgsess: analyst.token },
      remoteAddress: PEER,
    });
    expect(res.statusCode).toBe(404);
    expect(await anyRows(analyst.userId)).toBe(0);
  });
});

describe("forensic_ac_002 — the audit row", () => {
  test("an export writes one forensic.export row with what the key signed", async () => {
    const analyst = await session("analyst");
    const mark = logLines.length;
    const res = await get(app, analyst.token, EXPORT());
    expect(res.statusCode).toBe(200);
    const body = res.json() as { events: unknown[]; chain_root: string };
    const rows = await forensicRows(analyst.userId);
    expect(rows[0]?.correlation_id).toBe(reqIdOf(EXPORT(), mark));
    expect(rows).toEqual([
      {
        user_id: analyst.userId,
        org_id: ORG,
        action: "forensic.export",
        target_id: incidentId,
        outcome: "ok",
        source_ip: PEER,
        correlation_id: expect.stringMatching(/^[0-9a-f-]{36}$/),
        detail: { events: String(body.events.length), chain_root: body.chain_root },
      },
    ]);
    expect(body.events.length).toBe(2);
  });

  test("a report writes one forensic.report row with the seal and the PDF's SHA-256", async () => {
    const admin = await session("admin");
    const res = await get(app, admin.token, REPORT());
    expect(res.statusCode).toBe(200);
    const sha256 = createHash("sha256").update(res.rawPayload).digest("hex");
    const exported = (await get(app, admin.token, EXPORT())).json() as { chain_root: string };
    expect((await forensicRows(admin.userId))[0]).toEqual({
      user_id: admin.userId,
      org_id: ORG,
      action: "forensic.report",
      target_id: incidentId,
      outcome: "ok",
      source_ip: PEER,
      correlation_id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      detail: { events: "2", chain_root: exported.chain_root, report_sha256: sha256 },
    });
  });

  test("the rows of two requests carry two correlation ids", async () => {
    const analyst = await session("analyst");
    await get(app, analyst.token, EXPORT());
    await get(app, analyst.token, REPORT());
    const rows = await forensicRows(analyst.userId);
    expect(rows.map((r) => r.action)).toEqual(["forensic.export", "forensic.report"]);
    expect(rows[0]?.correlation_id).not.toBe(rows[1]?.correlation_id);
  });

  test("an :id in upper case is recorded lower-cased", async () => {
    const analyst = await session("analyst");
    const upper = incidentId.toUpperCase();
    expect(
      (await get(app, analyst.token, `/v1/incidents/${upper}/forensic-export`)).statusCode,
    ).toBe(200);
    expect((await get(app, analyst.token, `/v1/incidents/${upper}/report`)).statusCode).toBe(200);
    expect((await forensicRows(analyst.userId)).map((r) => r.target_id)).toEqual([
      incidentId,
      incidentId,
    ]);
  });

  test.each([
    ["export of a malformed id", "/v1/incidents/not-an-id/forensic-export"],
    ["report of a malformed id", "/v1/incidents/not-an-id/report"],
  ])("the %s answers 404 and writes no row", async (_name, path) => {
    const analyst = await session("analyst");
    expect((await get(app, analyst.token, path)).statusCode).toBe(404);
    expect(await anyRows(analyst.userId)).toBe(0);
  });

  test("another org's incident answers 404 and writes no row", async () => {
    const token = `tok-forensic-other-${globalThis.crypto.randomUUID()}`;
    const userId = globalThis.crypto.randomUUID();
    await seedSession(config, { token, userId, role: "analyst", orgId: "forensic-ac-other" });
    for (const path of [EXPORT(), REPORT()]) {
      expect((await get(app, token, path)).statusCode, path).toBe(404);
    }
    expect(await anyRows(userId)).toBe(0);
  });

  test.each([
    ["export", (id: string) => `/v1/incidents/${id}/forensic-export`],
    ["report", (id: string) => `/v1/incidents/${id}/report`],
  ])("a %s that answers 404 writes no row", async (_name, path) => {
    const analyst = await session("analyst");
    const res = await get(app, analyst.token, path(globalThis.crypto.randomUUID()));
    expect(res.statusCode).toBe(404);
    expect(await anyRows(analyst.userId)).toBe(0);
  });
});

describe("forensic_ac_003 — no document without its row", () => {
  test("the app that refuses the row still reads the incident", async () => {
    const analyst = await session("analyst");
    expect((await get(auditRefused, analyst.token, `/v1/incidents/${incidentId}`)).statusCode).toBe(
      200,
    );
  });

  test.each([
    ["export", EXPORT],
    ["report", REPORT],
  ])("when the row cannot be written, the %s answers 503 with no document", async (_name, path) => {
    const analyst = await session("analyst");
    const res = await get(auditRefused, analyst.token, path());
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
    expect(res.headers["content-type"]).not.toContain("application/pdf");
    expect(res.body).not.toContain("chain_root");
    expect(await anyRows(analyst.userId)).toBe(0);
  });
});
