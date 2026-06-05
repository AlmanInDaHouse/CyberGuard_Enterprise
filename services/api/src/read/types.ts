// SPEC-009 §Data contracts — read-models (response shapes). Declared here; the
// GREEN gate projects them (org-scoped) from the Postgres incidents/alerts the
// slice reads. Not populated at the RED gate.

export interface ResolvedAlert {
  alert_id: string;
  title: string;
  severity_id: number;
  status: string;
  rule_id: string | null;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  event_time: string;
  final_score: number;
}

export interface IncidentListItem {
  incident_id: string;
  agent_id: string;
  status: string;
  title: string;
  // SPEC-011 — the incident's aggregated OCSF severity (MAX over member alerts, 0–6).
  severity_id: number;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  alert_count: number;
  window_start: string;
  updated_at: string;
}

export interface IncidentDetail {
  incident_id: string;
  agent_id: string;
  status: string;
  title: string;
  // SPEC-011 — the incident's aggregated OCSF severity (MAX over member alerts, 0–6).
  severity_id: number;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  window_start: string;
  assigned_to: string | null;
  created_at: string;
  updated_at: string;
  alerts: ResolvedAlert[];
}

export type AlertListItem = ResolvedAlert;

export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

// SPEC-010 — the forensic event drill (step 1). `TimelineEvent` is the
// populated-in-v0.1 projection of `cges_events` (the `process_command_line` /
// `subject_user_sid` columns are `DEFAULT ''` and structurally empty in v0.1, so
// they are not projected — read-model.ts:57-60). `EventTimeline` is the
// `GET /v1/incidents/:id/events` response. These are additive: the SPEC-009
// `IncidentDetail` / `ResolvedAlert` contracts are unchanged by the drill.
export interface TimelineEvent {
  event_id: string;
  agent_id: string;
  activity_id: number;
  process_pid: number;
  process_uid: string;
  process_name: string;
  image_file_name: string;
  process_parent_pid: number | null;
  event_time: string;
}

export interface EventTimeline {
  incident_id: string;
  events: TimelineEvent[];
}
