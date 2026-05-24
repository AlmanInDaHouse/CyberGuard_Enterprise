import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getAgent, getCgesEvents, issueToken } from "./helpers/db.js";
import { prepareAgent } from "./helpers/marquee-agent.js";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// SPEC-005 AC-001 — polyglot marquee end-to-end.
//
// Stands up the full stack via testcontainers (Postgres + ClickHouse +
// Redis via globalSetup) + in-process ingest server; spawns the real
// compiled cg-agent binary against the stack; lets the agent capture
// ETW Kernel-Process events for a probe (cmd.exe /c exit 0) over a
// ~40 s observation window (D7 budget is 45 s wall-clock end-to-end
// per NFR-005-004); queries the cges_events ClickHouse table for the
// two persisted rows (Launch + Terminate) and asserts five conditions
// per SPEC-005 §AC AC-001.
//
// Three-layer RED expected: (i) agent has no ETW code yet (Phase 3.5
// closure); (ii) ingest's OuterEnvelopeSchema rejects envelopes with
// events[] (Phase 3.5 D6 schema acceptance); (iii) cges_events table
// does not exist (Phase 3.5 D6 literal DDL). All three layers clear
// in Phase 3.5 as a coordinated implementation phase.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test(
  "SPEC-005 AC-001 marquee: real cg-agent captures probe Launch + Terminate, persists 2 rows to ClickHouse cges_events with full SPEC-005 shape",
  async () => {
    const marqueeStartMs = Date.now();

    // Issue an enrollment token + start the agent against the stack.
    const token = await issueToken(config);

    const agent = prepareAgent({
      enrollUrl: server.enrollUrl,
      heartbeatUrl: server.heartbeatUrl,
      caCertPem: server.caCertPem,
      token,
    });

    // ~40 s observation window. Within the window: agent enrolls,
    // establishes mTLS, opens its ETW session, captures Launch +
    // Terminate of the probe spawned below, batches both into an
    // outer signed envelope, and POSTs. The 40 s leaves ~5 s headroom
    // against the 45 s D7 marquee budget for cleanup + assertions.
    const runPromise = agent.run(40_000);

    // Brief pause to let the agent reach steady state (enroll + ETW
    // session open) before the probe spawn produces Kernel-Process
    // events the agent must capture.
    await new Promise((resolve) => setTimeout(resolve, 5_000));

    // Probe: a short-lived process whose Launch + Terminate the agent
    // captures. The probe runs on the same host as the test (the
    // test runner's OS). cmd.exe is Windows-only — if the test runs
    // on a non-Windows runner, the probe spawn would need adjustment,
    // but Phase 3.4 (and SPEC-005 generally) is Windows-targeted per
    // ADR-0011 §Out-of-scope (Linux/macOS deferred).
    const { spawn } = await import("node:child_process");
    const probe = spawn("cmd.exe", ["/c", "exit 0"], { stdio: "ignore" });
    const probePid = probe.pid;
    if (probePid === undefined) {
      throw new Error("AC-001: probe spawn must return a PID");
    }
    await new Promise<void>((resolve) => probe.on("exit", () => resolve()));

    // Let the agent finish its observation window + envelope POST.
    const result = await runPromise;

    expect(result.stderr).not.toContain("panic");
    expect(result.stderr).not.toContain("exit_code: 9");

    // Read the agent's identity to recover the agent_id it enrolled as.
    const identityJson = JSON.parse(
      readFileSync(join(agent.identityDir, "identity.json"), "utf-8"),
    ) as { agent_id: string };
    const agentRow = await getAgent(config, identityJson.agent_id);
    expect(agentRow).not.toBeNull();

    // Query cges_events for the probe's two events.
    const events = await getCgesEvents(config, identityJson.agent_id);
    const probeEvents = events.filter((e) => e.process_pid === probePid);
    expect(probeEvents.length).toBe(2);

    const launch = probeEvents.find((e) => e.activity_id === 1);
    const terminate = probeEvents.find((e) => e.activity_id === 2);
    if (!launch || !terminate) {
      throw new Error(
        `AC-001: probe MUST have both Launch (activity_id=1) and Terminate (activity_id=2) events; got ${JSON.stringify(probeEvents)}`,
      );
    }

    // SPEC-005 §AC AC-001 — five assertions on the persisted rows.

    // (1) class_uid = 1007 (OCSF Process Activity) per ADR-0006 + ADR-0011.
    expect(launch.class_uid).toBe(1007);
    expect(terminate.class_uid).toBe(1007);

    // (2) process.uid stability across Launch + Terminate (cache hit per
    //     §Operational §2; recipe per ADR-0011 §6).
    expect(launch.process_uid).toBe(terminate.process_uid);
    expect(launch.process_uid).toMatch(
      /^[0-9a-f-]{36}:\d+:\d+$/,
    );

    // (3) process.created_time emitted as integer nanos UTC (Launch);
    //     same value present on Terminate (cache hit per §Operational §2).
    //     Assertion: both not null + byte-for-byte equal.
    expect(launch.process_created_time).not.toBeNull();
    expect(terminate.process_created_time).toBe(launch.process_created_time);

    // (4) process.exit_code conditional per AC-005 — Launch MUST NOT have
    //     it (null in the ClickHouse column); Terminate MUST have it
    //     (non-null integer; probe exited cleanly so value is 0).
    expect(launch.process_exit_code).toBeNull();
    expect(terminate.process_exit_code).toBe(0);

    // (5) Top-level process.name present + non-empty on both events per
    //     AC-006 (cmd.exe basename).
    expect(launch.process_name).toBe("cmd.exe");
    expect(terminate.process_name).toBe("cmd.exe");

    // D7 marquee budget — wall-clock ≤ 45 s per NFR-005-004.
    const marqueeElapsedSeconds = (Date.now() - marqueeStartMs) / 1000;
    console.info(
      JSON.stringify({
        event: "spec_005_marquee_complete",
        marquee_elapsed_seconds: marqueeElapsedSeconds,
        budget_seconds: 45,
        within_budget: marqueeElapsedSeconds <= 45,
      }),
    );
    expect(marqueeElapsedSeconds).toBeLessThanOrEqual(45);
  },
  60_000, // vitest timeout — generous over the 45 s budget for cleanup.
);
