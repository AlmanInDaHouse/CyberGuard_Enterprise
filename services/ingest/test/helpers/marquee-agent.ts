import { spawn } from "node:child_process";
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

export interface AgentRunResult {
  exitCode: number | null;
  stdout: string;
  stderr: string;
}

export interface MarqueeAgent {
  identityDir: string;
  agentTomlPath: string;
  /** Run the agent (enroll + heartbeat), then SIGINT after `runMs`. */
  run(runMs?: number): Promise<AgentRunResult>;
}

/**
 * Prepare a real cg-agent: write its `agent.toml` with `server.url` (plain
 * enroll), `server.heartbeat_url` (mTLS), the server CA as `trust_anchor_path`,
 * and an `[enrollment]` block carrying the issued token. One `main()` run does
 * enroll-then-heartbeat (SPEC-003 Amendment 2026-05-22).
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
      return new Promise<AgentRunResult>((resolveRun, rejectRun) => {
        const child = spawn(agentBinaryPath(), ["--config", tomlPath], {
          stdio: ["ignore", "pipe", "pipe"],
        });
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
          rejectRun(e);
        });
        child.on("exit", (code) => {
          clearTimeout(timer);
          resolveRun({ exitCode: code, stdout, stderr });
        });
      });
    },
  };
}
