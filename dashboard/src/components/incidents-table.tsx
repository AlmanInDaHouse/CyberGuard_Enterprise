import type { IncidentListItem } from "@/lib/api/types";

/**
 * Incidents list — presentational (data → DOM, no auth, no fetch). Renders one row
 * per incident with its status, title (link to the detail), MITRE tactics, alert
 * count and event-time window (SPEC-009 §Operational §3).
 *
 * A plain anchor (not next/link) is used deliberately: full-page navigation is
 * sufficient for the MVP scope (no client-side routing required), and it keeps the
 * component free of the App Router context so it renders standalone under jsdom.
 */
export function IncidentsTable({ items }: { items: IncidentListItem[] }) {
  if (items.length === 0) {
    return (
      <p data-testid="incidents-empty" className="text-sm text-slate-500">
        No incidents.
      </p>
    );
  }

  return (
    <table data-testid="incidents-table" className="w-full border-collapse text-sm">
      <thead>
        <tr className="border-b text-left text-slate-500">
          <th className="py-2 pr-4">Status</th>
          <th className="py-2 pr-4">Incident</th>
          <th className="py-2 pr-4">MITRE tactics</th>
          <th className="py-2 pr-4">Alerts</th>
          <th className="py-2 pr-4">Window</th>
        </tr>
      </thead>
      <tbody>
        {items.map((i) => (
          <tr key={i.incident_id} data-testid="incident-row" className="border-b">
            <td className="py-2 pr-4">{i.status}</td>
            <td className="py-2 pr-4">
              <a className="text-slate-900 underline" href={`/incidents/${i.incident_id}`}>
                {i.title}
              </a>
            </td>
            <td className="py-2 pr-4">{(i.cg_mitre?.tactics ?? []).join(", ")}</td>
            <td className="py-2 pr-4">{i.alert_count}</td>
            <td className="py-2 pr-4">{i.window_start}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
