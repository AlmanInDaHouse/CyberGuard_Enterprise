import type pg from "pg";
import type { Config } from "../config.js";
import type { Services } from "../services.js";
import { loadRules } from "./engine.js";
import { BATCH_LIMIT, runDetectionCycle } from "./index.js";
import {
  CORRELATION_WINDOW_SECONDS_DEFAULT,
  type DetectConfig,
  type DetectCycleResult,
} from "./types.js";

// ADR-0012 Amendment 2026-06-07 — production detection driver (in-process TS
// scheduler). Adds a PRODUCTION caller for runDetectionCycle without touching
// its body: the durable per-org detect_watermark stays the sole cursor. This
// does NOT trigger the §1 named-exit (no Go port) — it drives the existing
// transitional TS slice in prod, gated still on the future event-firehose ADR.

/** Per-tick safety cap on drain-loop re-invocations per org (bounds one tick). */
const MAX_ITERATIONS_PER_TICK = 10;

/** Structured log sink (event name + fields). No-op by default. */
export type DriverLog = (event: string, fields?: Record<string, unknown>) => void;

/**
 * Production analog of test/helpers/detect.ts:12 — build one org's DetectConfig
 * from the validated Config. rulesDir comes from INGEST_DETECT_RULES_DIR
 * (config.ts); the correlation window keeps the ADR-0012 §8 default.
 */
export function buildDetectConfig(config: Config, orgId: string): DetectConfig {
  return {
    ingest: config,
    orgId,
    rulesDir: config.INGEST_DETECT_RULES_DIR,
    correlationWindowSeconds: CORRELATION_WINDOW_SECONDS_DEFAULT,
  };
}

/**
 * Enumerate the orgs to drive a cycle for: DISTINCT org_id over enrolled agents
 * (schema.ts:17). An org with no enrolled agent emits no events to detect on.
 * Takes a pool so prod reuses services.pool (no per-tick connection churn).
 */
export async function listEnrolledOrgs(pool: pg.Pool): Promise<string[]> {
  const r = await pool.query<{ org_id: string }>(
    "SELECT DISTINCT org_id FROM agents ORDER BY org_id",
  );
  return r.rows.map((row) => row.org_id);
}

export interface DetectionDriverDeps {
  /** Delay between the END of one pass and the START of the next (ms). */
  intervalMs: number;
  /** Enumerate the orgs to process this tick. */
  listOrgs: () => Promise<string[]>;
  /** Run ONE detection cycle for an org and return its result. */
  runCycle: (orgId: string) => Promise<DetectCycleResult>;
  /** Drain-loop cap per org per tick (default MAX_ITERATIONS_PER_TICK). */
  maxIterationsPerTick?: number;
  log?: DriverLog;
}

export interface DetectionDriver {
  /**
   * Cancel the pending tick and await any in-flight pass, then resolve. Bounded:
   * the pass checks the stop flag between orgs and drain iterations, so at most
   * one runDetectionCycle is in flight at stop() — itself bounded (<= BATCH_LIMIT
   * events) — and the shutdown cannot hang. Idempotent enough for one close().
   */
  stop(): Promise<void>;
}

/**
 * Start the self-rescheduling scheduler. SINGLE-FLIGHT BY CONSTRUCTION: the next
 * tick is scheduled only in the `finally` of the current pass (NOT setInterval),
 * so a pass that runs longer than intervalMs can never overlap the next one.
 * Per tick: for each org, drain forward — re-invoke runCycle while it returns a
 * full batch (eventsEvaluated === BATCH_LIMIT), capped by maxIterationsPerTick,
 * then yield to the next org.
 */
export function startDetectionDriver(deps: DetectionDriverDeps): DetectionDriver {
  // Off-switch (ADR-0012 Amendment 2026-06-07): intervalMs <= 0 ⇒ driver DISABLED.
  // No tick is ever scheduled and stop() is a no-op. This is the operator
  // kill-switch (INGEST_DETECT_INTERVAL_MS=0) and the gate the detect-ac-001
  // marquee uses so its explicit runDetectionCycle is the single producer.
  if (deps.intervalMs <= 0) {
    return { stop: () => Promise.resolve() };
  }

  const cap = deps.maxIterationsPerTick ?? MAX_ITERATIONS_PER_TICK;
  const log: DriverLog = deps.log ?? (() => {});
  let stopped = false;
  let timer: NodeJS.Timeout | undefined;
  let inFlight: Promise<void> = Promise.resolve();

  async function onePass(): Promise<void> {
    const orgs = await deps.listOrgs();
    for (const orgId of orgs) {
      if (stopped) return;
      let i = 0;
      for (; i < cap; i++) {
        if (stopped) return;
        const result = await deps.runCycle(orgId);
        if (result.eventsEvaluated < BATCH_LIMIT) break; // org drained for this tick
      }
      if (i === cap) log("detect_driver_drain_cap_reached", { orgId, cap });
    }
  }

  function scheduleNext(): void {
    if (stopped) return;
    timer = setTimeout(runTick, deps.intervalMs);
  }

  function runTick(): void {
    inFlight = onePass()
      .catch((err: unknown) => log("detect_driver_pass_error", { error: String(err) }))
      .finally(scheduleNext);
  }

  scheduleNext();

  return {
    async stop(): Promise<void> {
      stopped = true;
      if (timer) clearTimeout(timer);
      await inFlight;
    },
  };
}

/**
 * Fail-loud rules check at driver BOOT (NOT first tick): a missing/unreadable
 * rules dir, or a dir with zero rules, must never silently degrade to a 0-alert
 * driver. We throw a clear, actionable error so a future bad packaging fails
 * VISIBLY at startup (the service refuses to start, like an invalid env var)
 * rather than running detection-dark. The escape hatch is the off-switch
 * (INGEST_DETECT_INTERVAL_MS=0), which skips this check entirely.
 */
function assertRulesLoadable(rulesDir: string): void {
  let ruleCount: number;
  try {
    ruleCount = loadRules(rulesDir).length;
  } catch (err) {
    throw new Error(
      `detection driver: cannot load Sigma rules from "${rulesDir}" — point INGEST_DETECT_RULES_DIR at a valid directory, or disable the driver with INGEST_DETECT_INTERVAL_MS=0: ${String(err)}`,
    );
  }
  if (ruleCount === 0) {
    throw new Error(
      `detection driver: 0 rules found in "${rulesDir}" — refusing to start a detection-dark driver. Bundle the rules (Dockerfile COPY rules), set INGEST_DETECT_RULES_DIR, or disable with INGEST_DETECT_INTERVAL_MS=0.`,
    );
  }
}

/**
 * Wire the driver to production dependencies: orgs from the agents table via the
 * shared pool, and the real runDetectionCycle carrying services.notify (null ⇒
 * undefined ⇒ notification disabled cleanly; configured ⇒ SPEC-014 active in
 * production). Started in startIngest after the listeners bind; stopped in the
 * server's close() (server.ts). When enabled (interval > 0) it fail-louds at
 * boot on an empty/unreadable rules dir.
 */
export function startProdDetectionDriver(
  config: Config,
  services: Services,
  log?: DriverLog,
): DetectionDriver {
  if (config.INGEST_DETECT_INTERVAL_MS > 0) {
    assertRulesLoadable(config.INGEST_DETECT_RULES_DIR);
  }
  return startDetectionDriver({
    intervalMs: config.INGEST_DETECT_INTERVAL_MS,
    listOrgs: () => listEnrolledOrgs(services.pool),
    runCycle: (orgId) =>
      runDetectionCycle(buildDetectConfig(config, orgId), services.notify ?? undefined),
    log,
  });
}
