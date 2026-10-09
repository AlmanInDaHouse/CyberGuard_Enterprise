import { randomUUID } from "node:crypto";
import { createClient } from "@clickhouse/client";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildDetectConfig } from "../src/detect/driver.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { type DetectCursor, SETTLE_MARGIN_MS } from "../src/detect/types.js";
import { getCursor, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-018 late_ac_005 — settle margin (§Operational §2, NFR-018-001). The read
// takes only rows whose arrived_at is at least the margin older than ClickHouse's
// now64(3). Each row's age is set by giving it an explicit arrived_at computed on
// ClickHouse's clock, so the cases need no waiting: the injected-margin case sits
// hours away from its bounds, and the production case 5 s away from each of its
// own. The production case builds the config as the driver does
// (buildDetectConfig), which leaves the margin unset: the constant, 5000 ms.

const AGENT = "01934abc-def0-7000-89ab-0000000a5001";
const NOTEPAD = "C:\\Windows\\System32\\notepad.exe";
const HOUR_MS = 3_600_000;
const MINUTE_MS = 60_000;

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

/** ClickHouse's now64(3) less `ms`, as a DateTime64(3) literal in UTC. */
async function chNowMinus(ms: number): Promise<string> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: "SELECT toString(subtractMilliseconds(now64(3, 'UTC'), {ms:UInt32})) AS t",
      query_params: { ms },
      format: "JSONEachRow",
    });
    const t = (await rs.json<{ t: string }>())[0]?.t;
    if (t === undefined) throw new Error("ClickHouse returned no now64(3)");
    return t;
  } finally {
    await ch.close();
  }
}

/** Insert one benign Launch into `orgId` with an explicit `arrivedAt`; returns its cursor. */
async function insertAged(orgId: string, arrivedAt: string): Promise<DetectCursor> {
  return { arrivedAt, eventId: await insertRow(orgId, arrivedAt) };
}

/** Insert one benign Launch into `orgId`; `arrivedAt` omitted ⇒ ClickHouse assigns it. */
async function insertRow(orgId: string, arrivedAt?: string): Promise<string> {
  const eventId = randomUUID();
  await insertCgesEvent(config, {
    agentId: AGENT,
    orgId,
    eventId,
    activityId: 1,
    processPid: 5000,
    processName: "notepad.exe",
    imageFileName: NOTEPAD,
    time: "2026-10-09 10:00:00.000000000",
    arrivedAt,
  });
  return eventId;
}

test("late_ac_005: a cycle reads only rows older than its margin, and the rest once the margin has passed", async () => {
  const org = "late-ac-005";
  const older = await insertAged(org, await chNowMinus(2 * HOUR_MS));
  const younger = await insertAged(org, await chNowMinus(30 * MINUTE_MS));
  const withMargin = (settleMarginMs: number) => ({ ...detectConfig(config, org), settleMarginMs });

  // A one-hour margin: the 2 h-old row is read, the 30 min-old one is not.
  const first = await runDetectionCycle(withMargin(HOUR_MS));
  expect(first.eventsEvaluated).toBe(1);
  expect(first.processedThrough).toEqual(older);
  expect(await getCursor(config, org)).toEqual(older);

  // Nothing else has passed that margin: an empty read leaves the cursor on it.
  const again = await runDetectionCycle(withMargin(HOUR_MS));
  expect(again.eventsEvaluated).toBe(0);
  expect(again.processedThrough).toBeNull();
  expect(await getCursor(config, org)).toEqual(older);

  // A ten-minute margin, which the 30 min-old row has passed.
  const second = await runDetectionCycle(withMargin(10 * MINUTE_MS));
  expect(second.eventsEvaluated).toBe(1);
  expect(second.processedThrough).toEqual(younger);
  expect(await getCursor(config, org)).toEqual(younger);
});

test("late_ac_005: the production driver's config reads with the 5000 ms constant", async () => {
  const org = "late-ac-005-prod";
  const cfg = buildDetectConfig(config, org);
  expect(cfg.settleMarginMs).toBeUndefined();
  expect(SETTLE_MARGIN_MS).toBe(5000);

  // 10 s old: past a 5000 ms margin. Just inserted: inside it.
  const past = await insertAged(org, await chNowMinus(10_000));
  await insertRow(org);

  const result = await runDetectionCycle(cfg);
  expect(result.eventsEvaluated).toBe(1);
  expect(result.processedThrough).toEqual(past);
});
