import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import pg from "pg";
import { afterAll, afterEach, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { startProdDetectionDriver } from "../src/detect/driver.js";
import type { Services } from "../src/services.js";
import { clickHousePresent, enrollTestAgent, getAlerts, insertCgesEvent } from "./helpers/db.js";
import { RULES_WINDOWS_DIR } from "./helpers/detect.js";

// rules_ac_007 — SPEC-016 Amendment 2026-10-16 (debt #91): the production driver loads
// the rule set once, when it starts. A valid change to the directory after the start
// changes nothing; a `.yml` file that fails to load, written after the start, changes
// nothing either, and no cycle fails; a start over a directory that holds that file
// refuses to start.

const ID_PREFIX = globalThis.crypto.randomUUID().slice(0, 8);
const ORG = "rules-ac-007";
const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
/** A rule file the loader reads (`.yml`) and refuses (not a rule). */
const BROKEN = "title: [this is not a rule\n";

let config: Config;
let pool: pg.Pool;
const dirs: string[] = [];

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

async function waitUntil(pred: () => Promise<boolean>, timeoutMs = 20_000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await pred()) return true;
    await delay(100);
  }
  return false;
}

/** A copy of rules/windows/ the test may change. */
function rulesCopy(): string {
  const dir = mkdtempSync(join(tmpdir(), "cg-rules-ac-007-"));
  cpSync(RULES_WINDOWS_DIR, dir, { recursive: true });
  dirs.push(dir);
  return dir;
}

/** What startProdDetectionDriver reads of the services: the pool, and no notifier. */
function services(): Services {
  return { pool, notify: null, soar: null } as unknown as Services;
}

/** A winword.exe parent and its powershell.exe child, for an agent of its own. */
async function officeSpawn(seq: number): Promise<string> {
  const agentId = globalThis.crypto.randomUUID();
  const arrivedAt = await clickHousePresent(config);
  const pid = 7100 + seq * 2;
  await insertCgesEvent(config, {
    agentId,
    orgId: ORG,
    eventId: `${ID_PREFIX}-def0-4000-89ab-${String(seq * 2).padStart(12, "0")}`,
    activityId: 1,
    processPid: pid,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-06-07 09:00:00.000000000",
    arrivedAt,
  });
  await insertCgesEvent(config, {
    agentId,
    orgId: ORG,
    eventId: `${ID_PREFIX}-def0-4000-89ab-${String(seq * 2 + 1).padStart(12, "0")}`,
    activityId: 1,
    processPid: pid + 1,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: pid,
    time: "2026-06-07 09:00:01.000000000",
    arrivedAt,
  });
  await enrollTestAgent(config, agentId, ORG);
  return agentId;
}

async function raised(agentId: string): Promise<boolean> {
  return waitUntil(async () =>
    (await getAlerts(config, { agentId })).some(
      (a) => a.rule_id === "rule.office_spawns_script_host",
    ),
  );
}

beforeAll(() => {
  config = inject("ingestConfig");
  pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
});
afterAll(async () => {
  await pool?.end();
});
afterEach(() => {
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

test("a valid change, then a broken file, written after the start change nothing", async () => {
  const dir = rulesCopy();
  const failures: Record<string, unknown>[] = [];
  const driver = startProdDetectionDriver(
    { ...config, INGEST_DETECT_RULES_DIR: dir, INGEST_DETECT_INTERVAL_MS: 50 },
    services(),
    (event, fields) => {
      // The driver's only failure event: a pass whose cycle threw (driver.ts).
      if (event === "detect_driver_pass_error") failures.push({ event, ...fields });
    },
  );
  try {
    // 1. A valid change alone: the rule's file removed.
    rmSync(join(dir, "office_spawns_script_host.yml"));
    expect(await raised(await officeSpawn(1))).toBe(true);
    // 2. A file that fails to load.
    writeFileSync(join(dir, "broken.yml"), BROKEN);
    expect(await raised(await officeSpawn(2))).toBe(true);
    expect(failures).toEqual([]);
  } finally {
    await driver.stop();
  }
});

test("with the off switch, nothing is loaded: a directory that does not exist starts", async () => {
  const driver = startProdDetectionDriver(
    {
      ...config,
      INGEST_DETECT_RULES_DIR: join(tmpdir(), `cg-rules-ac-007-absent-${ID_PREFIX}`),
      INGEST_DETECT_INTERVAL_MS: 0,
    },
    services(),
  );
  await driver.stop();
});

test("a start over a directory that holds a broken file refuses to start", () => {
  const dir = rulesCopy();
  writeFileSync(join(dir, "broken.yml"), BROKEN);
  expect(() =>
    startProdDetectionDriver(
      { ...config, INGEST_DETECT_RULES_DIR: dir, INGEST_DETECT_INTERVAL_MS: 50 },
      services(),
    ),
  ).toThrow(/cannot load Sigma rules/);
});
