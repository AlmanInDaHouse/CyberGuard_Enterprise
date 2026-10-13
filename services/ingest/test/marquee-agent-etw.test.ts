import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, test } from "vitest";
import { AGENT_ETW_SESSIONS, stopEtwSessions } from "./helpers/marquee-agent.js";

// Debt #63 (docs/handoff-session-37.md): on Windows Node ends the marquee's
// agent with a forced termination, so the agent's shutdown never stops its
// ETW session; the harness stops it by name. A session with no provider,
// which an unelevated process may start and stop, stands in for the agent's.

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const PROBE = "CgS38HarnessProbe";

function sessionRunning(name: string): boolean {
  return spawnSync("logman", ["query", name, "-ets"], { stdio: "ignore" }).status === 0;
}

describe("the harness's list of the agent's ETW sessions", () => {
  test("names the session the agent opens (agent/cg-agent/src/etw/mod.rs)", () => {
    const source = readFileSync(
      join(REPO_ROOT, "agent", "cg-agent", "src", "etw", "mod.rs"),
      "utf8",
    );
    const name = /pub const SESSION_NAME: &str = "([^"]+)";/.exec(source)?.[1];
    expect(name, "SESSION_NAME found in the agent's source").toBeDefined();
    expect(AGENT_ETW_SESSIONS).toEqual([name]);
  });
});

describe.skipIf(process.platform !== "win32")("stopEtwSessions", () => {
  afterEach(() => {
    spawnSync("logman", ["stop", PROBE, "-ets"], { stdio: "ignore" });
  });

  test("stops a running session and reports none left", () => {
    const start = spawnSync("logman", ["start", PROBE, "-ets", "-rt"], { stdio: "ignore" });
    expect(start.status, "the probe session started").toBe(0);
    expect(sessionRunning(PROBE)).toBe(true);

    expect(stopEtwSessions([PROBE])).toEqual([]);
    expect(sessionRunning(PROBE), "no session of the name after the stop").toBe(false);
  });

  test("a name with no session is not an error", () => {
    expect(sessionRunning(PROBE)).toBe(false);
    expect(stopEtwSessions([PROBE])).toEqual([]);
  });
});
