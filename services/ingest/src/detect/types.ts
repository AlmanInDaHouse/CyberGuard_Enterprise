import type { Config } from "../config.js";

/** Inputs for one detection cycle (SPEC-006 §Operational §1). */
export interface DetectConfig {
  ingest: Config;
  orgId: string;
  rulesDir: string;
}

/**
 * The minimal Sigma-subset rule the MVP evaluator understands: a single
 * `selection` matching `|endswith` over `Image` and `ParentImage` (SPEC-006
 * §Data contracts). Carries the CyberGuard `cg:` scoring/severity/MITRE block.
 */
export interface SigmaRule {
  id: string;
  title: string;
  level: string;
  parentImageEndsWith: string[];
  imageEndsWith: string[];
  heuristicScore: number;
  severityId: number;
  cgMitre: { tactics: string[]; techniques: string[] };
}

/** A cges_events row normalized into the Sigma process_creation shape (SPEC-006 §Data contracts). */
export interface NormalizedProcessEvent {
  eventId: string;
  agentId: string;
  activityId: number;
  pid: number;
  uid: string;
  processName: string;
  imageFileName: string;
  parentPid: number | null;
  /** Resolved via the parent-pid self-join; null when the parent Launch was not captured. */
  parentImage: string | null;
  time: string;
}

/** Score signals present for an alert; absent fields renormalize out (SPEC-006 §Operational §3). */
export interface ScoreSignals {
  heuristicScore?: number;
  uebaScore?: number;
  mlScore?: number;
}

/** Result of one detection cycle. */
export interface DetectCycleResult {
  /** Watermark advanced to this max `time` (null when no events were processed). */
  processedThrough: string | null;
  eventsEvaluated: number;
  alertsWritten: number;
}
