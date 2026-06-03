import type { IncidentDetail } from "@/lib/api/types";

/**
 * Incident detail — the teachable view: grouped alerts + the MITRE mapping
 * (presentational; data → DOM, no auth, no fetch).
 *
 * HARNESS-FIRST RED (PART 2/2): STUB. It mounts cleanly but renders neither the
 * grouped alerts nor the MITRE techniques, so dash_ac_002 fails on MISSING CONTENT
 * (render logic absent), not on a crash. The GREEN gate renders each resolved
 * alert and the incident's MITRE tactics/techniques (SPEC-009 §Operational §3,
 * §Acceptance criteria dash_ac_002).
 */
export function IncidentDetailView({ incident }: { incident: IncidentDetail }) {
  return <div data-testid="incident-detail-todo" data-incident-id={incident.incident_id} />;
}
