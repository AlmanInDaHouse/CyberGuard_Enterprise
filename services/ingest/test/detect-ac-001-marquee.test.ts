import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getAlerts, issueToken } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { prepareAgent } from "./helpers/marquee-agent.js";

// SPEC-006 detect_ac_001 — SC001 marquee, polyglot end-to-end. DEVELOPER-LOCAL
// (Windows + Docker + elevated), skipIf non-win32. NOT run in CI: needs real
// ETW capture (no ETW on Linux runners; no container runtime on hosted Windows
// runners per ADR-0010 §Decision part 3). Validated like the SPEC-005 marquee.
//
// A real cg-agent captures a winword.exe stand-in spawning powershell.exe; the
// events land in cges_events; the detection slice reads them, the rule matches
// (ParentImage winword.exe -> Image powershell.exe), and EXACTLY ONE alert is
// persisted to Postgres.
//
// Harness-first RED: runDetectionCycle throws NotImplemented until the Phase-5
// impl. IMPORTANT: a green run here does NOT imply production coverage of the
// already-running-Office case — the probe spawns the parent AFTER the agent
// session opens so it is captured (SPEC-006 §Operational §2 production FN).

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test.skipIf(process.platform !== "win32")(
  "detect_ac_001 marquee: real agent winword->powershell yields exactly 1 rule alert in Postgres",
  async () => {
    const token = await issueToken(config);
    const agent = prepareAgent({
      enrollUrl: server.enrollUrl,
      heartbeatUrl: server.heartbeatUrl,
      caCertPem: server.caCertPem,
      token,
      etwEnabled: true,
    });

    // ~40 s observation window; the agent enrolls, opens its ETW session, then
    // captures the probe's parent + child Launches below.
    const runPromise = agent.run(40_000);
    await new Promise((resolve) => setTimeout(resolve, 5_000));

    // Probe: a winword.exe stand-in (copy of cmd.exe) that spawns powershell.exe.
    // Both Launches occur after the session opens, so the parent is captured and
    // the parent-pid self-join resolves ParentImage = winword.exe.
    const probeDir = mkdtempSync(join(tmpdir(), "cg-detect-probe-"));
    const winword = join(probeDir, "winword.exe");
    copyFileSync(`${process.env.SystemRoot ?? "C:\\Windows"}\\System32\\cmd.exe`, winword);
    const { spawn } = await import("node:child_process");
    const probe = spawn(winword, ["/c", "powershell -Command exit 0"], { stdio: "ignore" });
    await new Promise<void>((resolve) => probe.on("exit", () => resolve()));

    const result = await runPromise;
    expect(result.stderr).not.toContain("panic");

    const identity = JSON.parse(
      readFileSync(join(agent.identityDir, "identity.json"), "utf-8"),
    ) as { agent_id: string };

    // Run detection over the captured events and assert exactly one rule alert.
    await runDetectionCycle(detectConfig(config));

    const alerts = await getAlerts(config, { agentId: identity.agent_id });
    expect(alerts).toHaveLength(1);
    expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
    expect(alerts[0]?.cg_detection_source).toBe("rule");
    expect(alerts[0]?.final_score).toBe(0.9);
  },
  60_000,
);
