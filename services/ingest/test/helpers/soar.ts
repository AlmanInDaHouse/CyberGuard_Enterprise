import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import type { Config } from "../../src/config.js";
import { loadPlaybooks } from "../../src/soar/loader.js";
import type { Playbook, SoarBounds, SoarConfig, StepRecord } from "../../src/soar/types.js";
import { insertCgesEvent } from "./db.js";

const here = dirname(fileURLToPath(import.meta.url));

/** The repository's playbooks/ directory (SPEC-022 §Operational §1). */
export const REPO_PLAYBOOKS_DIR = join(here, "..", "..", "..", "..", "playbooks");

/** The shipped playbooks, loaded through the loader. */
export function shippedPlaybooks(): Playbook[] {
  return loadPlaybooks(REPO_PLAYBOOKS_DIR).playbooks;
}

export interface SoarLogLine {
  level: "info" | "warn" | "debug";
  obj: Record<string, unknown>;
  msg: string;
}

export interface SoarHarness {
  soar: SoarConfig;
  logs: SoarLogLine[];
  close(): Promise<void>;
}

/** A SOAR carrier over the test Postgres, capturing its log lines. */
export function soarHarness(
  config: Config,
  playbooks: Playbook[],
  bounds?: Partial<SoarBounds>,
): SoarHarness {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  const logs: SoarLogLine[] = [];
  const line = (level: SoarLogLine["level"]) => (obj: Record<string, unknown>, msg: string) => {
    logs.push({ level, obj, msg });
  };
  return {
    soar: {
      playbooks,
      pg: pool,
      log: { info: line("info"), warn: line("warn"), debug: line("debug") },
      ...(bounds ? { bounds } : {}),
    },
    logs,
    close: () => pool.end(),
  };
}

export interface ExecutionRow {
  execution_id: string;
  org_id: string;
  incident_id: string;
  agent_id: string;
  playbook_id: string;
  playbook_version: number;
  status: string;
  finished_at: Date | null;
  steps: StepRecord[];
  chain_head: string | null;
}

/** The execution records of an incident, oldest first. */
export async function getExecutions(config: Config, incidentId: string): Promise<ExecutionRow[]> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const r = await pool.query<ExecutionRow>(
      `SELECT execution_id, org_id, incident_id, agent_id, playbook_id, playbook_version,
              status, finished_at, steps, chain_head
         FROM playbook_executions WHERE incident_id = $1
        ORDER BY started_at, execution_id`,
      [incidentId],
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

/** A playbook built in a test, for the cases the shipped one does not cover. */
export function testPlaybook(overrides: Partial<Playbook> = {}): Playbook {
  return {
    id: "pb_test",
    version: 1,
    name: "Test playbook",
    trustSources: ["rule"],
    severityIdGte: 4,
    steps: [
      { id: "isolate", action: "agent.isolate", mode: "dry_run" },
      {
        id: "notify",
        action: "notify.email",
        mode: "live",
        params: { subject: "Pending: {{incident.title}}", body: "{{steps}}" },
      },
    ],
    file: "test",
    ...overrides,
  };
}

export const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
export const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
export const CMD = "C:\\Windows\\System32\\cmd.exe";

/**
 * Seed the events of a High incident for `agentId` in `orgId`: winword.exe spawning
 * powershell.exe, which `office_spawns_script_host` (high → severity 4) matches.
 * `second` shifts the pids and times and spawns cmd.exe instead, for a correlated
 * alert on the same incident: an alert's dedup key holds the child's process name
 * and the incident window's 5-minute bucket (alerts.ts), so only another child
 * writes a second alert, which joins the same incident.
 */
export async function seedOfficeSpawn(
  config: Config,
  orgId: string,
  agentId: string,
  second = 0,
): Promise<void> {
  const pid = 7100 + second * 10;
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: pid,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: `2026-05-31 12:0${second}:00.000000000`,
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: pid + 1,
    processName: second === 0 ? "powershell.exe" : "cmd.exe",
    imageFileName: second === 0 ? POWERSHELL : CMD,
    processParentPid: pid,
    time: `2026-05-31 12:0${second}:01.000000000`,
  });
}

/** Seed one event that only `exec_from_startup_folder` (medium → severity 3) matches. */
export async function seedStartupFolderExec(
  config: Config,
  orgId: string,
  agentId: string,
): Promise<void> {
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: 7300,
    processName: "updater.exe",
    imageFileName:
      "C:\\Users\\u\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\updater.exe",
    time: "2026-05-31 13:00:00.000000000",
  });
}
