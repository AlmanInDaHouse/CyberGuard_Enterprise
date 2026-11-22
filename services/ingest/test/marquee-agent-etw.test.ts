import { spawnSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, test } from "vitest";
import {
  AGENT_ETW_SESSIONS,
  type Logman,
  stopEtwSessions,
  stopSessionsAfterExit,
} from "./helpers/marquee-agent.js";

// Debt #63 (docs/handoff-session-37.md): on Windows Node ends the marquee's
// agent with a forced termination, so the agent's shutdown never stops its
// ETW session; the harness stops it by name. A session with no provider,
// which an unelevated process may start and stop, stands in for the agent's.

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const PROBE = "CgS38HarnessProbe";
const PLA_E_DCS_NOT_FOUND = 0x80300002;
const E_ACCESSDENIED = 0x80070005;
const WMI_GUID_NOT_FOUND = 0x80071068;
/** No real wait in the fakes' tests. */
const noWait = () => {};

/** A `logman` whose queries answer `queries` in turn (the last repeated), stops `stop`. */
function scriptedLogman(queries: (number | null)[], stop: number | null) {
  const calls: string[][] = [];
  let next = 0;
  const logman: Logman = (args) => {
    calls.push([...args]);
    if (args[0] === "stop") return stop;
    const answer = queries[Math.min(next, queries.length - 1)] ?? null;
    next += 1;
    return answer;
  };
  return { logman, calls };
}

function sessionRunning(name: string): boolean {
  return spawnSync("logman", ["query", name, "-ets"], { stdio: "ignore" }).status === 0;
}

/**
 * A `logman` that answers each verb with a fixed status, a query after a
 * stop with `queryAfterStop` when given, and records its calls.
 */
function fakeLogman(answers: {
  query: number | null;
  stop: number | null;
  queryAfterStop?: number | null;
}): {
  logman: Logman;
  calls: string[][];
} {
  const calls: string[][] = [];
  let stopped = false;
  const logman: Logman = (args) => {
    calls.push([...args]);
    if (args[0] === "stop") {
      stopped = true;
      return answers.stop;
    }
    return stopped && answers.queryAfterStop !== undefined ? answers.queryAfterStop : answers.query;
  };
  return { logman, calls };
}

describe("the harness's list of the agent's ETW sessions", () => {
  test("names every session constant in the agent's source", () => {
    // A tripwire, not a proof: a `&str` constant whose name says SESSION or
    // LOGGER, or whose value starts with the agent's prefix.
    const src = join(REPO_ROOT, "agent", "cg-agent", "src");
    const names = new Set<string>();
    for (const file of readdirSync(src, { recursive: true, encoding: "utf8" })) {
      if (!file.endsWith(".rs")) {
        continue;
      }
      const source = readFileSync(join(src, file), "utf8");
      for (const m of source.matchAll(/const (\w+): &str = "([^"]+)";/g)) {
        const [, constName, value] = m as unknown as [string, string, string];
        if (/SESSION|LOGGER/.test(constName) || value.startsWith("CGAgent")) {
          names.add(value);
        }
      }
    }
    expect(names.size, "a session constant found in the agent's source").toBeGreaterThan(0);
    expect([...AGENT_ETW_SESSIONS].sort()).toEqual([...names].sort());
  });
});

describe("stopEtwSessions, against a logman that answers as told", () => {
  test("a session this process may not stop is reported as still running", () => {
    const { logman, calls } = fakeLogman({ query: E_ACCESSDENIED, stop: E_ACCESSDENIED });
    expect(stopEtwSessions(["X"], logman)).toEqual(["X"]);
    expect(calls).toContainEqual(["stop", "X", "-ets"]);
  });

  test("a name with no session is neither stopped nor reported", () => {
    const { logman, calls } = fakeLogman({ query: PLA_E_DCS_NOT_FOUND, stop: 0 });
    expect(stopEtwSessions(["X"], logman)).toEqual([]);
    expect(calls).not.toContainEqual(["stop", "X", "-ets"]);
  });

  test("a session being torn down is asked again, and its end awaited (debt #161)", () => {
    // Running, then stopped: the provider gone first, then the session.
    const { logman, calls } = scriptedLogman(
      [0, WMI_GUID_NOT_FOUND, WMI_GUID_NOT_FOUND, PLA_E_DCS_NOT_FOUND],
      0,
    );
    expect(stopEtwSessions(["X"], logman, noWait)).toEqual([]);
    expect(calls.filter((c) => c[0] === "query")).toHaveLength(4);
  });

  test("a stopped session still running is awaited, with a bound, and then reported", () => {
    const awaited = scriptedLogman([0, 0, 0, PLA_E_DCS_NOT_FOUND], 0);
    expect(stopEtwSessions(["X"], awaited.logman, noWait)).toEqual([]);
    const stuck = scriptedLogman([0], 0);
    expect(stopEtwSessions(["X"], stuck.logman, noWait)).toEqual(["X"]);
    // One query before the stop, twenty after it.
    expect(stuck.calls.filter((c) => c[0] === "query")).toHaveLength(21);
  });

  test("a session that keeps answering as torn down past the bound is an error", () => {
    const { logman } = scriptedLogman([WMI_GUID_NOT_FOUND], 0);
    expect(() => stopEtwSessions(["X"], logman, noWait)).toThrow(
      "logman query X -ets failed: exit status 0x80071068",
    );
  });

  test("any other answer to the query is an error", () => {
    expect(() => stopEtwSessions(["X"], fakeLogman({ query: 1, stop: 0 }).logman)).toThrow(
      "logman query X -ets failed: exit status 0x1",
    );
    expect(() => stopEtwSessions(["X"], fakeLogman({ query: null, stop: 0 }).logman)).toThrow(
      "logman query X -ets failed: it did not run",
    );
  });
});

describe("stopSessionsAfterExit", () => {
  test("while another marquee agent runs, nothing is stopped", () => {
    const { logman, calls } = fakeLogman({ query: 0, stop: 0 });
    expect(stopSessionsAfterExit(true, logman)).toEqual([]);
    expect(calls).toEqual([]);
  });

  test("otherwise each running session of the agent's is stopped", () => {
    const { logman, calls } = fakeLogman({
      query: 0,
      stop: 0,
      queryAfterStop: PLA_E_DCS_NOT_FOUND,
    });
    expect(stopSessionsAfterExit(false, logman)).toEqual([]);
    expect(calls).toEqual(
      AGENT_ETW_SESSIONS.flatMap((name) => [
        ["query", name, "-ets"],
        ["stop", name, "-ets"],
        ["query", name, "-ets"],
      ]),
    );
  });
});

describe.skipIf(process.platform !== "win32")("stopEtwSessions, against logman", () => {
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
