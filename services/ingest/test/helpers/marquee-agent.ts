import { type ChildProcess, execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// services/ingest/test/helpers -> repo root is four levels up.
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..");

/** Locate the compiled cg-agent binary (built by `cargo build -p cg-agent`). */
export function agentBinaryPath(): string {
  const exe = process.platform === "win32" ? "cg-agent.exe" : "cg-agent";
  for (const profile of ["release", "debug"]) {
    const p = join(REPO_ROOT, "target", profile, exe);
    if (existsSync(p)) {
      return p;
    }
  }
  throw new Error(
    "cg-agent binary not found under target/{release,debug}; run `cargo build -p cg-agent` first",
  );
}

/**
 * True on Windows when this process is elevated. On Windows the agent's
 * secure path opens the ETW session and an unelevated agent exits with
 * code 9 before its first heartbeat (SPEC-017 §Operational §1), so a
 * suite that needs the agent's heartbeats runs there only elevated.
 * `net session` succeeds only for an elevated process.
 */
export function isElevatedWindows(): boolean {
  if (process.platform !== "win32") {
    return false;
  }
  try {
    execFileSync("net", ["session"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** The ETW sessions the agent opens, by name (`SESSION_NAME`, agent/cg-agent/src/etw/mod.rs). */
export const AGENT_ETW_SESSIONS: readonly string[] = ["CGAgent-KernelProcess"];

/** `logman`'s exit status when no session of the name exists (PLA_E_DCS_NOT_FOUND). */
const PLA_E_DCS_NOT_FOUND = 0x80300002;
/** `logman`'s exit status for a session this process may not query or stop (E_ACCESSDENIED). */
const E_ACCESSDENIED = 0x80070005;

/** Runs `logman` with `args` and returns its exit status (`null` if it did not run). */
export type Logman = (args: readonly string[]) => number | null;

const runLogman: Logman = (args) => spawnSync("logman", [...args], { stdio: "ignore" }).status;

/**
 * Whether a session of the name exists. A query answered with access denied
 * means it exists and this process may not query it: for a name with no
 * session the answer is PLA_E_DCS_NOT_FOUND, elevated or not.
 */
function etwSessionRunning(name: string, logman: Logman): boolean {
  const status = logman(["query", name, "-ets"]);
  if (status === 0 || status === E_ACCESSDENIED) {
    return true;
  }
  if (status === PLA_E_DCS_NOT_FOUND) {
    return false;
  }
  throw new Error(
    `logman query ${name} -ets failed: ${status === null ? "it did not run" : `exit status 0x${status.toString(16)}`}`,
  );
}

/**
 * Stop each named ETW session that is running (Windows). Returns the names
 * still running afterwards: an unelevated process cannot stop a session an
 * elevated agent started (debt #63, docs/handoff-session-37.md).
 */
export function stopEtwSessions(names: readonly string[], logman: Logman = runLogman): string[] {
  const left: string[] = [];
  for (const name of names) {
    if (!etwSessionRunning(name, logman)) {
      continue;
    }
    logman(["stop", name, "-ets"]);
    if (etwSessionRunning(name, logman)) {
      left.push(name);
    } else {
      console.log(JSON.stringify({ event: "marquee_agent_etw_session_stopped", session: name }));
    }
  }
  return left;
}

/**
 * The marquee agents still running. vitest runs each test file in a
 * process of its own (`vitest.config.ts`), so the set holds one file's
 * agents; it lives on `globalThis`, whatever the number of times the file
 * evaluates this module.
 */
function liveAgents(): Set<ChildProcess> {
  const g = globalThis as { __cgMarqueeLiveAgents?: Set<ChildProcess> };
  g.__cgMarqueeLiveAgents ??= new Set();
  return g.__cgMarqueeLiveAgents;
}

/**
 * Once a marquee's agent has exited (Windows): stop the agent's ETW
 * sessions and return the names still running. While another marquee's
 * agent runs — this one outlived a test that failed before it awaited the
 * run — the name is that agent's, whose start reclaimed this one's
 * session, so nothing is stopped.
 */
export function stopSessionsAfterExit(
  othersRunning: boolean,
  logman: Logman = runLogman,
): string[] {
  if (othersRunning) {
    console.log(
      JSON.stringify({
        event: "marquee_agent_etw_stop_skipped",
        reason: "another marquee agent is running",
      }),
    );
    return [];
  }
  return stopEtwSessions(AGENT_ETW_SESSIONS, logman);
}

export interface AgentRunResult {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  /** The agent process's PID (SPEC-019 net_ac_010: its own connections are not reported). */
  pid: number | undefined;
}

export interface MarqueeAgent {
  identityDir: string;
  agentTomlPath: string;
  /**
   * Run the agent (enroll + heartbeat), then SIGINT after `runMs`. On
   * Windows it then stops the agent's ETW sessions unless another marquee
   * agent is running, and rejects if one is still running.
   */
  run(runMs?: number): Promise<AgentRunResult>;
}

/**
 * Prepare a real cg-agent: write its `agent.toml` with `server.url` (plain
 * enroll), `server.heartbeat_url` (mTLS), the server CA as `trust_anchor_path`,
 * and an `[enrollment]` block carrying the issued token. One `main()` run does
 * enroll-then-heartbeat (SPEC-003 Amendment 2026-05-22) on the secure path,
 * which on Windows also captures processes (SPEC-017).
 */
export function prepareAgent(args: {
  enrollUrl: string;
  heartbeatUrl: string;
  caCertPem: string;
  token: string;
}): MarqueeAgent {
  const dir = mkdtempSync(join(tmpdir(), "cg-agent-marquee-"));
  const caPath = join(dir, "server-ca.pem");
  writeFileSync(caPath, args.caCertPem);
  const tomlPath = join(dir, "agent.toml");
  const toml = [
    "[server]",
    `url = ${JSON.stringify(args.enrollUrl)}`,
    `heartbeat_url = ${JSON.stringify(args.heartbeatUrl)}`,
    `trust_anchor_path = ${JSON.stringify(caPath)}`,
    "",
    "[agent]",
    'id = "01934abc-def0-7000-89ab-0000000000bb"',
    'hostname = "MARQUEE-PC"',
    "",
    "[heartbeat]",
    "interval_seconds = 1",
    "request_timeout_seconds = 5",
    "max_retries = 3",
    "backoff_initial_ms = 100",
    "backoff_factor = 2.0",
    "backoff_max_ms = 1000",
    "",
    "[enrollment]",
    `token = ${JSON.stringify(args.token)}`,
    `cert_path = ${JSON.stringify(join(dir, "cert.pem"))}`,
    `key_path = ${JSON.stringify(join(dir, "key.dat"))}`,
    `identity_path = ${JSON.stringify(join(dir, "identity.json"))}`,
    "timeout_seconds = 30",
    "max_retries = 3",
    "backoff_initial_ms = 100",
    "backoff_factor = 2.0",
    "",
    "[tls]",
    'minimum_version = "1.3"',
    "",
    "[envelope]",
    'canonical_form = "JCS"',
    "",
  ].join("\n");
  writeFileSync(tomlPath, toml);

  return {
    identityDir: dir,
    agentTomlPath: tomlPath,
    run(runMs = 4000): Promise<AgentRunResult> {
      const running = new Promise<AgentRunResult>((resolveRun, rejectRun) => {
        const child = spawn(agentBinaryPath(), ["--config", tomlPath], {
          stdio: ["ignore", "pipe", "pipe"],
        });
        const live = liveAgents();
        live.add(child);
        let stdout = "";
        let stderr = "";
        child.stdout.on("data", (d: Buffer) => {
          stdout += d.toString("utf8");
        });
        child.stderr.on("data", (d: Buffer) => {
          stderr += d.toString("utf8");
        });
        const timer = setTimeout(() => child.kill("SIGINT"), runMs);
        child.on("error", (e) => {
          clearTimeout(timer);
          live.delete(child);
          rejectRun(e);
        });
        child.on("exit", (code) => {
          clearTimeout(timer);
          live.delete(child);
          // On Windows `kill("SIGINT")` is a forced termination: the agent's
          // shutdown, which stops its ETW session, does not run (debt #63).
          // A rejection names the agent's exit code, not its stderr: the
          // marquees print their output redacted (SPEC-020 §Operational §11).
          const agentExit = `agent exit code ${code}`;
          let left: string[];
          try {
            left = process.platform === "win32" ? stopSessionsAfterExit(live.size > 0) : [];
          } catch (e) {
            rejectRun(new Error(`${(e as Error).message}; ${agentExit}`, { cause: e }));
            return;
          }
          if (left.length > 0) {
            rejectRun(
              new Error(
                `the agent's ETW session ${left.join(", ")} is still running after the run and could not be stopped; a session an elevated agent started stops only elevated (logman stop <name> -ets); ${agentExit}`,
              ),
            );
            return;
          }
          resolveRun({ exitCode: code, stdout, stderr, pid: child.pid });
        });
      });
      // A marquee awaits the run seconds after it starts; a rejection that
      // comes first must reach that await, not count as unhandled.
      running.catch(() => {});
      return running;
    },
  };
}
