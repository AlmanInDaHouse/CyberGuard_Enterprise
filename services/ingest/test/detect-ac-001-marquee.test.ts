import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { eventUnixSeconds } from "../src/detect/alerts.js";
import { buildDetectConfig } from "../src/detect/driver.js";
import { runDetectionCycle } from "../src/detect/index.js";
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
// IMPORTANT: a green run here does NOT imply production coverage of the
// already-running-Office case — the probe spawns the parent AFTER the agent
// session opens so it is captured (SPEC-006 §Operational §2 production FN).

const OFFICE_RULE = "rule.office_spawns_script_host";

/** A drive-letter (Win32) path: `C:\...` (SPEC-017 §Operational §5). */
const WIN32_PATH = /^[A-Za-z]:\\/;

/** Dedup bucket width (ADR-0012 §5 / §8), as alerts.ts builds the dedup_key. */
const DEDUP_BUCKET_SECONDS = 300;

interface LaunchRow {
  event_id: string;
  process_pid: number;
  process_parent_pid: number | null;
  process_name: string;
  image_file_name: string;
  time: string;
}

/** The agent's captured Launches, with the fields this marquee reports. */
async function capturedLaunches(config: Config, agentId: string): Promise<LaunchRow[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
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

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  // ADR-0012 Amendment 2026-06-07 off-switch: disable the production detection
  // driver for this marquee (INGEST_DETECT_INTERVAL_MS=0). The marquee is the
  // SINGLE producer — it calls runDetectionCycle explicitly below.
  // A live in-process driver would be a second producer racing the same
  // per-org watermark, blurring the deterministic exactly-one-alert signal.
  config = { ...inject("ingestConfig"), INGEST_DETECT_INTERVAL_MS: 0 };
  server = await startIngest(config);
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
    const probe = spawn(winword, ["/c", "powershell -Command exit 0"], { stdio: "ignore" });
    const probePid = probe.pid;
    await new Promise<void>((resolve) => probe.on("exit", () => resolve()));

    const result = await runPromise;
    expect(result.stderr).not.toContain("panic");

    const identity = JSON.parse(
      readFileSync(join(agent.identityDir, "identity.json"), "utf-8"),
    ) as { agent_id: string };
    const agentId = identity.agent_id;

    // Run detection over the captured events, against the whole rule set, as the
    // production driver configures the cycle: with the 5000 ms settle margin
    // (SPEC-018 §Operational §2, late_ac_008). The agent has exited, so its last
    // POST is in; waiting out the margin (plus 1 s for the ClickHouse container's
    // clock) lets this single cycle read everything the agent delivered.
    await new Promise((resolve) => setTimeout(resolve, SETTLE_MARGIN_MS + 1000));
    await runDetectionCycle(buildDetectConfig(config, "default"));

    // The probe (winword.exe stand-in) and its powershell.exe child, as captured.
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

    const alerts = await getAlerts(config, { agentId });
    const others = alerts.filter((a) => a.rule_id !== OFFICE_RULE);
    console.log(
      `detect_ac_001 alerts from other rules (logged, not asserted): ${others.length}${others
        .map((a) => `\n  ${a.rule_id} ${a.dedup_key}`)
        .join("")}`,
    );

    const office = alerts.filter((a) => a.rule_id === OFFICE_RULE);
    expect(office).toHaveLength(1);
    const alert = office[0];
    expect(alert?.cg_detection_source).toBe("rule");
    expect(alert?.final_score).toBe(0.9);
    expect(alert?.status).toBe("new");

    expect(probePid, "probe pid").toBeDefined();
    expect(child, "the probe's powershell.exe child in cges_events").toBeDefined();
    if (child === undefined) return;
    // SPEC-017 capture_ac_012: the captured image paths are in Win32 form.
    expect(probeLaunch?.image_file_name, "probe image path").toMatch(WIN32_PATH);
    expect(child.image_file_name, "child image path").toMatch(WIN32_PATH);
    expect(alert?.source_events).toContain(child.event_id);
    // dedup_key = <agent_id>::<rule_id>::<process_name>::<5-min bucket> (ADR-0012 §5),
    // built from the child's event.
    const bucket = Math.floor(eventUnixSeconds(child.time) / DEDUP_BUCKET_SECONDS);
    expect(alert?.dedup_key).toBe(`${agentId}::${OFFICE_RULE}::${child.process_name}::${bucket}`);
  },
  60_000,
);
