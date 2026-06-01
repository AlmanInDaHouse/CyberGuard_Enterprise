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
