import { randomUUID } from "node:crypto";
import { createClient } from "@clickhouse/client";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { BATCH_LIMIT } from "../src/detect/index.js";
import { advanceCursor, getCursor, readNewEvents } from "../src/detect/read-model.js";
import { insertCgesEvents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-018 late_ac_003 — ties across the limit. All the rows of one INSERT share
// one arrived_at (§Context 6); with more of them than the read limit, the cursor
// (arrived_at, event_id) cuts inside the tie and the next read resumes after the
// cut (§Data contracts). Every event_id is read exactly once. The event_ids are
// random UUIDs, so the order ClickHouse gives them — not the insert order — is
// what the reads follow; the cursor goes through the Postgres row between reads,
// as in the cycle.

const ORG = "late-ac-003";
const AGENT = "01934abc-def0-7000-89ab-0000000a3001";
const NOTEPAD = "C:\\Windows\\System32\\notepad.exe";
const ROWS = 2 * BATCH_LIMIT + BATCH_LIMIT / 2;

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

/** How many distinct arrived_at values the org's rows carry. */
async function distinctArrivedAt(orgId: string): Promise<number> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query:
        "SELECT toUInt32(uniqExact(arrived_at)) AS n FROM cges_events WHERE org_id = {org:String}",
      query_params: { org: orgId },
      format: "JSONEachRow",
    });
    return (await rs.json<{ n: number }>())[0]?.n ?? 0;
  } finally {
    await ch.close();
  }
}

test("late_ac_003: one INSERT larger than the read limit, sharing one arrived_at, is read whole, every event_id once", async () => {
  const ids = Array.from({ length: ROWS }, () => randomUUID());
  await insertCgesEvents(
    config,
    ids.map((eventId, i) => ({
      agentId: AGENT,
      orgId: ORG,
      eventId,
      activityId: 1,
      processPid: 30_000 + i,
      processName: "notepad.exe",
      imageFileName: NOTEPAD,
      time: "2026-10-09 10:00:00.000000000",
    })),
  );
  // The precondition: one INSERT, one arrived_at.
  expect(await distinctArrivedAt(ORG)).toBe(1);

  const cfg = detectConfig(config, ORG);
  const seen: string[] = [];
  const sizes: number[] = [];
  for (let read = 0; read < 5; read++) {
    const batch = await readNewEvents(cfg, await getCursor(cfg), BATCH_LIMIT);
    sizes.push(batch.events.length);
    if (batch.cursor === null) break;
    seen.push(...batch.events.map((e) => e.eventId));
    await advanceCursor(cfg, batch.cursor);
  }

  expect(sizes).toEqual([BATCH_LIMIT, BATCH_LIMIT, BATCH_LIMIT / 2, 0]);
  expect(seen).toHaveLength(ROWS);
  expect(new Set(seen).size).toBe(ROWS);
  expect([...seen].sort()).toEqual([...ids].sort());
});
