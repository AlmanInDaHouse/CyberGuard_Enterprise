import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { eventUnixSeconds } from "../src/detect/alerts.js";
import { buildDetectConfig } from "../src/detect/driver.js";
import { BATCH_LIMIT, runDetectionCycle } from "../src/detect/index.js";
import { SETTLE_MARGIN_MS } from "../src/detect/types.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getAlerts, issueToken } from "./helpers/db.js";
import { prepareAgent } from "./helpers/marquee-agent.js";

// SPEC-006 detect_ac_001 — SC001 marquee, polyglot end-to-end. DEVELOPER-LOCAL
// (Windows + Docker + elevated), skipIf non-win32. NOT run in CI: needs real
// ETW capture (no ETW on Linux runners; no container runtime on hosted Windows
// runners per ADR-0010 §Decision part 3). Validated like the SPEC-005 marquee.
//
// A real cg-agent captures a winword.exe stand-in spawning powershell.exe; the
// events land in cges_events; the detection slice reads them against the whole
// rule set, the office rule matches (ParentImage winword.exe -> Image
// powershell.exe), and EXACTLY ONE rule.office_spawns_script_host alert is
// persisted to Postgres for the agent, sourced from the captured powershell.exe
// child (SPEC-016 §Operational §3). Alerts from other rules on the machine's
// background activity are logged, not asserted. The captured image_file_name of
// the probe and its child is logged and asserted in Win32 form (SPEC-017
// capture_ac_012); the agent runs its normal secure path.
//
// SPEC-006 Amendment 2026-10-31 (debts #4, #37): the agent must have run its
// window, and its Launches must be in cges_events, before any detection
// assertion, so a capture failure is never read as a detection one. Detection
// then waits out the settle margin on ClickHouse's clock and drains the org
// with the driver's stop condition: the suite's other marquees wrote to the
// same org earlier in the run, so the agent's rows can lie past one batch.
//
// IMPORTANT: a green run here does NOT imply production coverage of the
// already-running-Office case — the probe spawns the parent AFTER the agent
// session opens so it is captured (SPEC-006 §Operational §2 production FN).

const OFFICE_RULE = "rule.office_spawns_script_host";

/** A drive-letter (Win32) path: `C:\...` (SPEC-017 §Operational §5). */
const WIN32_PATH = /^[A-Za-z]:\\/;

/** Dedup bucket width (ADR-0012 §5 / §8), as alerts.ts builds the dedup_key. */
const DEDUP_BUCKET_SECONDS = 300;

/** The cycles after which an undrained org fails the run (SPEC-006 Amendment 2026-10-31). */
const MAX_DRAIN_CYCLES = 20;

/** NFR-006-005: probe spawn to alert readable in Postgres. */
const MARQUEE_BUDGET_SECONDS = 60;

/** How long the margin may take to pass on ClickHouse's clock before the run fails. */
const SETTLE_WAIT_LIMIT_MS = 15_000;

/** The capture-side causes a missing Launch names (SPEC-006 Amendment 2026-10-31, item 2). */
const CAPTURE_CAUSES =
  "a capture failure, not a detection one (a stale target/release/cg-agent.exe, POSTs the ingest refused, a failed ETW capture)";

interface LaunchRow {
  event_id: string;
  process_pid: number;
  process_parent_pid: number | null;
  process_name: string;
  image_file_name: string;
  time: string;
}

function clickhouse(config: Config): ClickHouseClient {
  return createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
}

/** The agent's captured Launches, with the fields this marquee reports. */
async function capturedLaunches(config: Config, agentId: string): Promise<LaunchRow[]> {
  const ch = clickhouse(config);
  try {
    const rs = await ch.query({
      query: `
        SELECT toString(event_id) AS event_id, process_pid, process_parent_pid, process_name,
               image_file_name, toString(time) AS time
        FROM cges_events FINAL
        WHERE agent_id = {agent_id:String} AND class_uid = 1007 AND activity_id = 1
        ORDER BY time ASC
      `,
      query_params: { agent_id: agentId },
      format: "JSONEachRow",
    });
    return await rs.json<LaunchRow>();
  } finally {
    await ch.close();
  }
}

/**
 * Wait until the settle margin has passed, on ClickHouse's clock, for the
 * agent's last row: a cycle reads only rows that arrived at least the margin
 * before ClickHouse's `now64()` (read-model.ts), whatever the host's clock
 * says. Returns the lag reached, in ms; null if the limit ran out first.
 */
async function waitForSettle(config: Config, agentId: string): Promise<number | null> {
  const ch = clickhouse(config);
  try {
    const deadline = Date.now() + SETTLE_WAIT_LIMIT_MS;
    for (;;) {
      const rs = await ch.query({
        query: `
          SELECT toString(toUnixTimestamp64Milli(now64(3)) - toUnixTimestamp64Milli(max(arrived_at))) AS lag_ms
          FROM cges_events
          WHERE agent_id = {agent_id:String}
        `,
        query_params: { agent_id: agentId },
        format: "JSONEachRow",
      });
      const [row] = await rs.json<{ lag_ms: string }>();
      const lag = Number(row?.lag_ms);
      if (lag > SETTLE_MARGIN_MS) {
        return lag;
      }
      if (Date.now() > deadline) {
        return null;
      }
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  } finally {
    await ch.close();
  }
}

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  // ADR-0012 Amendment 2026-06-07 off-switch: disable the production detection
  // driver for this marquee (INGEST_DETECT_INTERVAL_MS=0). The marquee is the
  // SINGLE producer — it calls runDetectionCycle explicitly below.
  // A live in-process driver would be a second producer racing the same
  // per-org watermark, blurring the deterministic exactly-one-alert signal.
  config = { ...inject("ingestConfig"), INGEST_DETECT_INTERVAL_MS: 0 };
  // Enrollment over TLS, as the agent requires it with [enrollment] (SPEC-023 §Operational §6).
  server = await startIngest({ ...config, INGEST_ENROLL_TLS: true });
});

afterAll(async () => {
  await server?.close();
});

test.skipIf(process.platform !== "win32")(
  "detect_ac_001 marquee: real agent winword->powershell yields exactly 1 office_spawns_script_host alert in Postgres",
  async () => {
    const token = await issueToken(config);
    const agent = prepareAgent({
      enrollUrl: server.enrollUrl,
      heartbeatUrl: server.heartbeatUrl,
      caCertPem: server.caCertPem,
      token,
    });

    // ~40 s observation window; the agent enrolls, opens its ETW session, then
    // captures the probe's parent + child Launches below.
    const runPromise = agent.run(40_000);
    await new Promise((resolve) => setTimeout(resolve, 5_000));

    // Probe: a winword.exe stand-in (copy of cmd.exe) that spawns powershell.exe.
    // Both Launches occur after the session opens, so the parent is captured and
    // the per-child parent resolution yields ParentImage = winword.exe.
    const probeDir = mkdtempSync(join(tmpdir(), "cg-detect-probe-"));
    const winword = join(probeDir, "winword.exe");
    copyFileSync(`${process.env.SystemRoot ?? "C:\\Windows"}\\System32\\cmd.exe`, winword);
    const { spawn } = await import("node:child_process");
    const probeSpawnedAt = Date.now();
    const seconds = () => (Date.now() - probeSpawnedAt) / 1000;
    let elapsedSeconds: number | undefined;
    try {
      const probe = spawn(winword, ["/c", "powershell -Command exit 0"], { stdio: "ignore" });
      const probePid = probe.pid;
      await new Promise<void>((resolve) => probe.on("exit", () => resolve()));

      const result = await runPromise;
      // (1) The agent ran its window; one that exited on its own captured nothing after.
      expect(
        result.endedByHarness,
        `the agent exited on its own before its window ended, exit code ${result.exitCode} (9: a capture source refused for privilege at startup, as for an unelevated agent)`,
      ).toBe(true);
      // The agent's output is not printed: the marquees keep it redacted (SPEC-020 §Operational §11).
      expect(result.stderr.includes("panic"), "the agent panicked (its stderr is withheld)").toBe(
        false,
      );

      const identity = JSON.parse(
        readFileSync(join(agent.identityDir, "identity.json"), "utf-8"),
      ) as { agent_id: string };
      const agentId = identity.agent_id;

      // (2) Capture evidence, before the settle wait and any detection
      // assertion: the probe (winword.exe stand-in) and its powershell.exe
      // child, as captured, with their image paths in Win32 form.
      const launches = await capturedLaunches(config, agentId);
      const probeLaunch = launches.find((l) => l.process_pid === probePid);
      const child = launches.find(
        (l) =>
          l.process_parent_pid === probePid &&
          l.image_file_name.toLowerCase().endsWith("\\powershell.exe"),
      );
      console.log(
        `detect_ac_001 captured image_file_name — probe: ${probeLaunch?.image_file_name ?? "(not captured)"}; child: ${child?.image_file_name ?? "(not captured)"}`,
      );
      expect(
        launches.length,
        `no Launch event of agent ${agentId} in cges_events: ${CAPTURE_CAUSES}`,
      ).toBeGreaterThan(0);
      expect(probePid, "probe pid").toBeDefined();
      expect(probeLaunch, `the probe's Launch in cges_events: ${CAPTURE_CAUSES}`).toBeDefined();
      expect(
        child,
        `the probe's powershell.exe child in cges_events: ${CAPTURE_CAUSES}`,
      ).toBeDefined();
      if (probeLaunch === undefined || child === undefined) return;
      // SPEC-017 capture_ac_012: the captured image paths are in Win32 form.
      expect(probeLaunch.image_file_name, "probe image path").toMatch(WIN32_PATH);
      expect(child.image_file_name, "child image path").toMatch(WIN32_PATH);
      const capturedAt = seconds();

      // (3) The settle margin (SPEC-018 §Operational §2, late_ac_008), on
      // ClickHouse's clock; then drain with the driver's stop condition, each
      // cycle on buildDetectConfig with no carrier, the cycle loading the rule
      // set itself, and the driver off.
      const lag = await waitForSettle(config, agentId);
      expect(
        lag,
        `the settle margin did not pass on ClickHouse's clock within ${SETTLE_WAIT_LIMIT_MS} ms`,
      ).not.toBeNull();
      const detectConfig = buildDetectConfig(config, "default");
      let cycles = 0;
      let rows = 0;
      let drained = false;
      while (!drained && cycles < MAX_DRAIN_CYCLES) {
        const cycle = await runDetectionCycle(detectConfig);
        cycles += 1;
        rows += cycle.eventsEvaluated;
        drained = cycle.eventsEvaluated < BATCH_LIMIT;
      }
      console.log(
        `detect_ac_001 drained ${rows} rows in ${cycles} cycles; capture checked at ${capturedAt} s, drained at ${seconds()} s`,
      );
      expect(drained, `org default not drained after ${MAX_DRAIN_CYCLES} cycles`).toBe(true);

      const alerts = await getAlerts(config, { agentId });
      elapsedSeconds = seconds();
      const others = alerts.filter((a) => a.rule_id !== OFFICE_RULE);
      console.log(
        `detect_ac_001 alerts from other rules (logged, not asserted): ${others.length}${others
          .map((a) => `\n  ${a.rule_id} ${a.dedup_key}`)
          .join("")}`,
      );

      // (4) The alert criteria (detect_ac_001; SPEC-016 §Operational §3), unchanged.
      const office = alerts.filter((a) => a.rule_id === OFFICE_RULE);
      expect(office).toHaveLength(1);
      const alert = office[0];
      expect(alert?.cg_detection_source).toBe("rule");
      expect(alert?.final_score).toBe(0.9);
      expect(alert?.status).toBe("new");
      expect(alert?.source_events).toContain(child.event_id);
      // dedup_key = <agent_id>::<rule_id>::<process_name>::<5-min bucket> (ADR-0012 §5),
      // built from the child's event.
      const bucket = Math.floor(eventUnixSeconds(child.time) / DEDUP_BUCKET_SECONDS);
      expect(alert?.dedup_key).toBe(`${agentId}::${OFFICE_RULE}::${child.process_name}::${bucket}`);
      // NFR-006-005: probe spawn to alert readable in Postgres.
      expect(elapsedSeconds, "NFR-006-005 budget").toBeLessThanOrEqual(MARQUEE_BUDGET_SECONDS);
    } finally {
      // NFR-006-005: logged at info on every run, a failed one included; the
      // measured value once the alerts were read, else the time to the failure.
      console.log(
        JSON.stringify({
          level: "info",
          detect_marquee_elapsed_seconds: elapsedSeconds ?? seconds(),
          alerts_read: elapsedSeconds !== undefined,
        }),
      );
    }
  },
  // NFR-006-005's 60 s run from the probe's spawn, asserted above; this also
  // covers enrollment and the 5 s before the probe.
  90_000,
);
