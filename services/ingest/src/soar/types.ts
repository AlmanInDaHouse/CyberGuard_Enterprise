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
