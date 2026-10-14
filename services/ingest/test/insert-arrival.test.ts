import { createClient } from "@clickhouse/client";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { getCursor, readNewEvents } from "../src/detect/read-model.js";
import { TEST_ARRIVAL_LEAD_MS, insertCgesEvents, nextArrivalMs } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// Debt #41. The detection tests read with a settle margin of 0 (helpers/detect.ts),
// on ClickHouse's clock, the one that stamps `arrived_at` (SPEC-018 §Operational
// §2). On Docker Desktop that clock steps back now and then, and rows ClickHouse
// stamped just before a step lie in its future, where a margin-0 read misses them.
// The insert helpers therefore stamp `arrived_at` themselves, TEST_ARRIVAL_LEAD_MS
// before ClickHouse's present and after the last value they stamped.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const ORG = "debt-41-arrival";
const AGENT = globalThis.crypto.randomUUID();

function row(eventId: string) {
  return {
    agentId: AGENT,
    orgId: ORG,
    eventId,
    activityId: 1,
    processPid: 41,
    processName: "notepad.exe",
    imageFileName: "C:\\Windows\\System32\\notepad.exe",
    time: "2026-05-31 12:00:00.000000000",
  };
}

async function arrivals(eventIds: string[]): Promise<{ eventId: string; ageMs: number }[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: `SELECT toString(event_id) AS eventId,
                     toUInt64(toUnixTimestamp64Milli(now64(3, 'UTC')) - toUnixTimestamp64Milli(arrived_at)) AS ageMs
                FROM cges_events WHERE event_id IN {ids:Array(UUID)}
               ORDER BY arrived_at, event_id`,
      query_params: { ids: eventIds },
      format: "JSONEachRow",
    });
    const rows = await rs.json<{ eventId: string; ageMs: string | number }>();
    return rows.map((r) => ({ eventId: r.eventId, ageMs: Number(r.ageMs) }));
  } finally {
    await ch.close();
  }
}

test("a helper's rows arrive the lead before ClickHouse's present, past any shorter step back", async () => {
  const first = globalThis.crypto.randomUUID();
  const second = globalThis.crypto.randomUUID();
  await insertCgesEvents(config, [row(first), row(second)]);

  const seen = await arrivals([first, second]);
  expect(seen).toHaveLength(2);
  for (const { ageMs } of seen) {
    // Stamped TEST_ARRIVAL_LEAD_MS back; read a moment later, give or take a step.
    expect(ageMs).toBeGreaterThanOrEqual(TEST_ARRIVAL_LEAD_MS - 5_000);
    expect(ageMs).toBeLessThan(TEST_ARRIVAL_LEAD_MS + 60_000);
  }

  // A read that takes only rows 25 s old finds them: so does a margin-0 read
  // after ClickHouse's clock steps back by up to 25 s.
  const cfg = { ...detectConfig(config, ORG), settleMarginMs: 25_000 };
  const { events } = await readNewEvents(cfg, await getCursor(cfg), 100);
  expect(events.map((e) => e.eventId).sort()).toEqual([first, second].sort());
});

test("successive calls arrive in order, and one call's rows share one arrival", async () => {
  const ids = Array.from({ length: 4 }, () => globalThis.crypto.randomUUID());
  await insertCgesEvents(config, [row(ids[0] as string), row(ids[1] as string)]);
  await insertCgesEvents(config, [row(ids[2] as string)]);
  await insertCgesEvents(config, [row(ids[3] as string)]);

  const seen = await arrivals(ids);
  const firstTwo = seen.filter((r) => r.eventId === ids[0] || r.eventId === ids[1]);
  expect(new Set(firstTwo.map((r) => r.ageMs)).size).toBe(1);
  // Ordered by arrival: the first call's rows, then the second's, then the third's.
  expect(seen.slice(2).map((r) => r.eventId)).toEqual([ids[2], ids[3]]);
  const ages = seen.map((r) => r.ageMs);
  expect(ages[1]).toBeGreaterThan(ages[2] as number);
  expect(ages[2]).toBeGreaterThan(ages[3] as number);
});

test("nextArrivalMs keeps the order when ClickHouse's clock steps back", () => {
  const now = 1_791_977_941_171;
  const first = nextArrivalMs(now, 0);
  expect(first).toBe(now - TEST_ARRIVAL_LEAD_MS);
  // The clock steps back 2351 ms: the next arrival still follows the first.
  expect(nextArrivalMs(now - 2351, first)).toBe(first + 1);
  // Later, the clock has moved past it: the arrival follows the clock again.
  expect(nextArrivalMs(now + 5000, first + 1)).toBe(now + 5000 - TEST_ARRIVAL_LEAD_MS);
});
