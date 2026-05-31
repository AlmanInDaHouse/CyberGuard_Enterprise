import type { Config } from "../config.js";

/**
 * ADR-0012 §8 default correlation window (seconds). One tunable shared by the
 * dedup bucket AND the parent-pid self-join look-back (SPEC-006 §Operational §2).
 */
export const CORRELATION_WINDOW_SECONDS_DEFAULT = 300;

/**
 * SPEC-007 / ADR-0013 §2 incident correlation window (seconds). Incident grouping's
 * OWN tunable — distinct from and wider than the 300 s dedup bucket (ADR-0013 §2),
 * event-time windowed (ADR-0013 §1), per-org configurable (NFR-007-001). 1800 s
 * (30 min) groups a multi-step intrusion of distinct alerts while staying under an
 * hour.
 */
export const INCIDENT_CORRELATION_WINDOW_SECONDS = 1800;

/** Inputs for one detection cycle (SPEC-006 §Operational §1). */
export interface DetectConfig {
  ingest: Config;
  orgId: string;
  rulesDir: string;
  /**
   * ADR-0012 §8 correlation window (seconds), per-org configurable. Bounds the
   * parent-pid self-join look-back (SPEC-006 §Operational §2): a parent launched
   * more than this many seconds before the child — or never captured — resolves
   * to parent_image = null (the documented production false-negative).
   */
  correlationWindowSeconds: number;
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

/**
 * The 5c evaluator's output: the matching rule's contribution to an alert,
 * BEFORE scoring (5d) and alert assembly/persist (5e). Carries no dedup_key,
 * no final_score, no alert_id — the scorer (5d) consumes heuristicScore and the
 * persist step (5e) builds the alert from this + the final score.
 */
export interface RuleMatch {
  ruleId: string;
  heuristicScore: number;
  severityId: number;
  cgMitre: { tactics: string[]; techniques: string[] };
  sourceEvent: NormalizedProcessEvent;
}

/**
 * The persisted-alert fields the SPEC-007 incident-grouping step (§Operational §6)
 * consumes — enough to compute the grouping_key (org + agent + canonical tactic-set
 * + event-time window, §Data contracts §4) and append the alert to its incident.
 */
export interface IncidentGroupingInput {
  alertId: string;
  orgId: string;
  agentId: string;
  cgMitre: { tactics: string[]; techniques: string[] };
  /** The alert's `event_time` (event-occurrence; ADR-0013 §1) — the windowing basis. */
  eventTime: string;
}
