import { NotImplementedError } from "./errors.js";
import type { DetectConfig, DetectCycleResult } from "./types.js";

/**
 * Run one detection cycle: poll `cges_events` forward by the per-org `time`
 * watermark with `FINAL` (ADR-0012 §7), normalize each Launch event (resolve
 * `ParentImage` via the parent-pid self-join, SPEC-006 §Operational §2),
 * evaluate the MVP Sigma rule, score it (renormalized, §Operational §3), and
 * upsert alerts into Postgres (`ON CONFLICT (dedup_key) DO NOTHING`, §6).
 *
 * Harness-first RED stub: the typed entry point exists; the logic lands in the
 * Phase-5 implementation. Every detect_ac_* test calls this (or `scoreAlert`)
 * before touching the alerts table, so the RED is `NotImplementedError`, never
 * "alerts table missing".
 */
export async function runDetectionCycle(config: DetectConfig): Promise<DetectCycleResult> {
  throw new NotImplementedError(`runDetectionCycle(org=${config.orgId})`);
}
