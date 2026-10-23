import type pg from "pg";
import type { Config } from "../config.js";
import type { Services } from "../services.js";
import { loadRules } from "./engine.js";
import { BATCH_LIMIT, runDetectionCycle } from "./index.js";
import type { DetectConfig, DetectCycleResult, SigmaRule } from "./types.js";

// ADR-0012 Amendment 2026-06-07 — production detection driver (in-process TS
// scheduler). Adds a PRODUCTION caller for runDetectionCycle, which it hands the
// rule set it loaded at its start (SPEC-016 Amendment 2026-10-16); otherwise the
// cycle's body is its own: the durable per-org detect_watermark row stays the sole cursor (its
// (arrived_at, event_id) arrival cursor since SPEC-018 §Data contracts). This
// does NOT trigger the §1 named-exit (no Go port) — it drives the existing
// transitional TS slice in prod, gated still on the future event-firehose ADR.

/** Per-tick safety cap on drain-loop re-invocations per org (bounds one tick). */
const MAX_ITERATIONS_PER_TICK = 10;

/** Structured log sink (event name + fields). No-op by default. */
export type DriverLog = (event: string, fields?: Record<string, unknown>) => void;

/**
 * Production analog of test/helpers/detect.ts:18 — build one org's DetectConfig
 * from the validated Config. rulesDir comes from INGEST_DETECT_RULES_DIR
 * (config.ts). settleMarginMs is left unset, so production reads with the
 * SETTLE_MARGIN_MS constant (SPEC-018 §Operational §2): there is no env var.
 */
export function buildDetectConfig(config: Config, orgId: string): DetectConfig {
  return {
    ingest: config,
    orgId,
    rulesDir: config.INGEST_DETECT_RULES_DIR,
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
 * Load the rule set once, at driver BOOT, and return it (SPEC-016 Amendment
 * 2026-10-16, debt #91): every cycle evaluates what this returns, so a change to the directory takes the
 * next start. Fail-loud: a missing/unreadable rules dir, a rule outside the contract,
 * or a dir with zero rules must never silently degrade to a 0-alert driver. We throw
 * a clear, actionable error so a bad packaging fails VISIBLY at startup (the service
 * refuses to start, like an invalid env var) rather than running detection-dark. The
 * escape hatch is the off-switch (INGEST_DETECT_INTERVAL_MS=0), which skips the load.
 */
function assertRulesLoadable(rulesDir: string): SigmaRule[] {
  let rules: SigmaRule[];
  try {
    rules = loadRules(rulesDir);
  } catch (err) {
    throw new Error(
      `detection driver: cannot load Sigma rules from "${rulesDir}" — point INGEST_DETECT_RULES_DIR at a valid directory, or disable the driver with INGEST_DETECT_INTERVAL_MS=0: ${String(err)}`,
    );
  }
  if (rules.length === 0) {
    throw new Error(
      `detection driver: 0 rules found in "${rulesDir}" — refusing to start a detection-dark driver. Bundle the rules (Dockerfile COPY rules), set INGEST_DETECT_RULES_DIR, or disable with INGEST_DETECT_INTERVAL_MS=0.`,
    );
  }
  return rules;
}

/**
 * The rule set the production driver evaluates: none with the off-switch
 * (INGEST_DETECT_INTERVAL_MS=0), else the directory's, fail-loud as above. startIngest
 * calls it before it opens a listener (SPEC-023 §Operational §2).
 */
export function loadDetectionRules(config: Config): SigmaRule[] {
  return config.INGEST_DETECT_INTERVAL_MS > 0
    ? assertRulesLoadable(config.INGEST_DETECT_RULES_DIR)
    : [];
}

/**
 * Wire the driver to production dependencies: orgs from the agents table via the
 * shared pool, and the real runDetectionCycle carrying services.notify (null ⇒
 * undefined ⇒ notification disabled cleanly; configured ⇒ SPEC-014 active in
 * production). Started in startIngest after the listeners bind, with the rules it
 * loaded before them; stopped in the server's close() (server.ts). Without `rules`
 * it loads them itself, fail-loud when enabled (interval > 0) on an empty/unreadable
 * rules dir.
 */
export function startProdDetectionDriver(
  config: Config,
  services: Services,
  log?: DriverLog,
  rules: SigmaRule[] = loadDetectionRules(config),
): DetectionDriver {
  return startDetectionDriver({
    intervalMs: config.INGEST_DETECT_INTERVAL_MS,
    listOrgs: () => listEnrolledOrgs(services.pool),
    runCycle: (orgId) =>
      runDetectionCycle(
        { ...buildDetectConfig(config, orgId), rules },
        services.notify ?? undefined,
        services.soar ?? undefined,
      ),
    log,
  });
}
