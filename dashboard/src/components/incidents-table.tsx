import type { IncidentListItem } from "@/lib/api/types";

/**
 * Incidents list — presentational (data → DOM, no auth, no fetch).
 *
 * HARNESS-FIRST RED (PART 2/2): STUB. It mounts cleanly (so a render assertion
 * fails on MISSING CONTENT, not on a crashed component / broken setup) but renders
 * no rows. The GREEN gate renders one row per incident with its status, title,
 * MITRE tactics and alert count (SPEC-009 §Operational §3).
 */
export function IncidentsTable({ items }: { items: IncidentListItem[] }) {
  return <div data-testid="incidents-table-todo" data-count={items.length} />;
}
