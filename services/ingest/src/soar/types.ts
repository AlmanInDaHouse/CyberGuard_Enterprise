// SPEC-022 / ADR-0021 — the SOAR module's types. The module runs declarative
// playbooks on the incident-create seam of runDetectionCycle, after SPEC-014's
// notification (ADR-0017 §2's fire-and-forget terms).

/** The closed action list of ADR-0021 §3. */
export type SoarAction = "agent.isolate" | "notify.email";

/** How a step runs; `dry_run` by default (SPEC-022 §Data contracts §1). */
export type StepMode = "live" | "dry_run";

/** One step of a loaded playbook. `params` is present for `notify.email` only. */
export interface PlaybookStep {
  id: string;
  action: SoarAction;
  mode: StepMode;
  params?: { subject: string; body: string };
}

/** A playbook as the loader returns it, validated against schema and policy. */
export interface Playbook {
  id: string;
  version: number;
  name: string;
  trustSources: string[];
  severityIdGte: number;
  steps: PlaybookStep[];
  /** The file it came from, for log lines and refusals. */
  file: string;
}

/** Why SOAR is idle, and the level the boot logs it at (SPEC-022 §Operational §1). */
export interface SoarIdle {
  reason: "directory_absent" | "no_playbooks";
  dir: string;
  level: "info" | "warn";
}

/** What the loader returns: the playbooks, or why there are none. */
export interface LoadedPlaybooks {
  dir: string;
  playbooks: Playbook[];
  idle?: SoarIdle;
}

/** A step's outcome (SPEC-022 §Data contracts §2). */
export type StepOutcome = "done" | "would_do" | "skipped" | "failed";

/** One element of an execution record's `steps`, as stored. */
export interface StepRecord {
  step_id: string;
  action: SoarAction;
  mode: StepMode;
  outcome: StepOutcome;
  detail: string;
  started_at: string;
  finished_at: string;
  link: string;
}

/** The log surface the module writes to (pino's `Logger` satisfies it). */
export interface SoarLog {
  info(obj: Record<string, unknown>, msg: string): void;
  warn(obj: Record<string, unknown>, msg: string): void;
  debug(obj: Record<string, unknown>, msg: string): void;
}

/** The step and run bounds (SPEC-022 §Operational §6); tests may shorten them. */
export interface SoarBounds {
  stepMs: number;
  runMs: number;
}

export const DEFAULT_SOAR_BOUNDS: SoarBounds = { stepMs: 15_000, runMs: 30_000 };

/** The minimal Postgres surface a run needs (pg's `Pool` satisfies it). */
export interface SoarPg {
  query<R = unknown>(text: string, values?: unknown[]): Promise<{ rows: R[] }>;
}

/**
 * The SOAR carrier runDetectionCycle takes as its third optional argument
 * (SPEC-022 §Operational §2): built once at boot from the loaded playbooks, or by
 * a test from its own.
 */
export interface SoarConfig {
  playbooks: Playbook[];
  pg: SoarPg;
  log: SoarLog;
  bounds?: Partial<SoarBounds>;
  /** The clock the run's bound and the steps' times read, in ms; `Date.now` unless a
   *  test replaces it to reach the run's bound between two steps (debt #76). */
  now?: () => number;
}

/** What the seam knows of a newly created incident. */
export interface IncidentCreated {
  incidentId: string;
  orgId: string;
  agentId: string;
  title: string;
  severityId: number;
  /** The creating alert's `cg_detection_source`; `rule` for every MVP rule. */
  detectionSource: string;
}
