import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";

// SPEC-006 / ADR-0012 §6 — migration 0002_alerts gate (Phase-5 5a).
//
// Verifies the alerts table EXISTS and its CHECK / UNIQUE / FK constraints
// actually REJECT bad data — stronger than "runMigrations() did not throw".
// This is 5a's own verifiable GREEN: it passes in CI (Linux + testcontainers)
// even while detect_ac_002..006 stay RED (NotImplemented) — 5a is the infra
// that enables the rest, it does NOT clear the Known CI debt.
//
// Every assertion runs inside a BEGIN/ROLLBACK so nothing persists in the
// shared (singleFork) Postgres. Inserts that need a valid FK create their agent
// row inside the same transaction (also rolled back).

let client: pg.Client;

const AGENT = "01934abc-def0-7000-89ab-0000000000a1";
const SRC = "01934abc-def0-4000-89ab-0000000000b1";

interface AlertInput {
  alertId: string;
  agentId: string;
  severityId: number;
  source: string;
  ruleId: string | null;
  modelId: string | null;
  sourceEvents: string[];
  heuristic: number | null;
  ueba: number | null;
  ml: number | null;
  finalScore: number;
  status: string;
  dedupKey: string;
}

let n = 0;
function baseline(): AlertInput {
  n += 1;
  return {
    alertId: `01934abc-def0-4000-89ab-${String(n).padStart(12, "0")}`,
    agentId: AGENT,
    severityId: 4,
    source: "rule",
    ruleId: "rule.office_spawns_script_host",
    modelId: null,
    sourceEvents: [SRC],
    heuristic: 0.9,
    ueba: null,
    ml: null,
    finalScore: 0.9,
    status: "new",
    dedupKey: `${AGENT}::rule.office_spawns_script_host::winword.exe::${n}`,
  };
}

async function insertAgent(): Promise<void> {
  await client.query(
    "INSERT INTO agents (agent_id, pubkey, cert_pem, expires_at) VALUES ($1, $2, $3, now())",
    [AGENT, Buffer.from([0]), "test-cert"],
  );
}

async function insertAlert(a: AlertInput): Promise<void> {
  await client.query(
    `INSERT INTO alerts
       (alert_id, agent_id, title, severity_id, cg_detection_source, rule_id, model_id,
        source_events, heuristic_score, ueba_score, ml_score, final_score, status, dedup_key)
     VALUES ($1, $2, 'detection-test', $3, $4, $5, $6, $7::uuid[], $8, $9, $10, $11, $12, $13)`,
    [
      a.alertId,
      a.agentId,
      a.severityId,
      a.source,
      a.ruleId,
      a.modelId,
      a.sourceEvents,
      a.heuristic,
      a.ueba,
      a.ml,
      a.finalScore,
      a.status,
      a.dedupKey,
    ],
  );
}

/**
 * Run `body` inside a rolled-back transaction; return whether it completed
 * without a DB error. `withAgent` inserts a valid agents row first so FK is
 * satisfied and the INTENDED constraint is what rejects.
 */
async function attempt(withAgent: boolean, body: () => Promise<void>): Promise<boolean> {
  await client.query("BEGIN");
  let ok = false;
  try {
    if (withAgent) await insertAgent();
    await body();
    ok = true;
  } catch {
    ok = false;
  }
  await client.query("ROLLBACK");
  return ok;
}

beforeAll(async () => {
  const config: Config = inject("ingestConfig");
  client = new pg.Client({ connectionString: config.INGEST_PG_URL });
  await client.connect();
});

afterAll(async () => {
  await client?.end();
});

test("alerts table exists", async () => {
  await expect(client.query("SELECT count(*) FROM alerts")).resolves.toBeDefined();
});

test("accepts a valid rule alert", async () => {
  const ok = await attempt(true, async () => {
    await insertAlert(baseline());
  });
  expect(ok).toBe(true);
});

test("rejects final_score out of range (1.5)", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.finalScore = 1.5;
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects an alert with no score present (all of heuristic/ueba/ml null)", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.heuristic = null;
    a.ueba = null;
    a.ml = null;
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects cg_detection_source='rule' without rule_id", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.ruleId = null;
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects cg_detection_source='ml' without model_id", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.source = "ml";
    a.ruleId = null;
    a.modelId = null;
    a.heuristic = null;
    a.ml = 0.8; // keep a score present so the ONLY violation is the missing model_id
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects empty source_events", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.sourceEvents = [];
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects an out-of-enum status", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.status = "bogus";
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects severity_id outside 0..6 (the hardening CHECK)", async () => {
  const ok = await attempt(true, async () => {
    const a = baseline();
    a.severityId = 99;
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});

test("rejects a duplicate dedup_key (UNIQUE)", async () => {
  const ok = await attempt(true, async () => {
    const first = baseline();
    const second = baseline();
    second.dedupKey = first.dedupKey; // collide
    await insertAlert(first);
    await insertAlert(second);
  });
  expect(ok).toBe(false);
});

test("rejects a FK to a nonexistent agent", async () => {
  const ok = await attempt(false, async () => {
    const a = baseline();
    a.agentId = "01934abc-def0-7000-89ab-0000000fffff"; // not inserted into agents
    await insertAlert(a);
  });
  expect(ok).toBe(false);
});
