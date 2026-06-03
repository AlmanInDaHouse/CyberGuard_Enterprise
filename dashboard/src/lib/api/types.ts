// SPEC-009 §Data contracts — the read-API wire contract the dashboard consumes.
//
// These are the dashboard's VIEW types, deliberately decoupled from
// services/api's internal TS read-models (`services/api/src/read/types.ts`): the
// dashboard talks to the read-API over HTTP, so its contract is the JSON shape,
// not the server's source layout. The dash_ac_001 integration test binds these to
// the REAL read-API (in-process via buildApp/app.inject), so any drift between
// this contract and the server's output surfaces there.

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
