import type { IncidentDetail } from "@/lib/api/types";

/**
 * Incident detail — the teachable view: the MITRE mapping + the grouped (resolved)
 * alerts the read-API returns (SPEC-009 §Operational §3, §AC dash_ac_002).
 * Presentational: data → DOM, no auth, no fetch.
 *
 * The MITRE technique set is rendered once, at the incident level (the canonical
 * grouping mapping, SPEC-007); each grouped alert shows its title, rule, severity
 * and score.
 */
export function IncidentDetailView({ incident }: { incident: IncidentDetail }) {
  const tactics = incident.cg_mitre?.tactics ?? [];
  const techniques = incident.cg_mitre?.techniques ?? [];

  return (
    <section data-testid="incident-detail" className="flex flex-col gap-6">
      <header className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold">{incident.title}</h2>
        <p className="text-sm text-slate-500">
          Status: {incident.status} · Window: {incident.window_start}
        </p>
      </header>

      <div data-testid="incident-mitre" className="flex flex-col gap-1">
        <h3 className="font-medium">MITRE ATT&amp;CK</h3>
        <p className="text-sm">Tactics: {tactics.join(", ")}</p>
        <p className="text-sm">Techniques: {techniques.join(", ")}</p>
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="font-medium">Grouped alerts ({incident.alerts.length})</h3>
        <ul className="flex flex-col gap-2">
          {incident.alerts.map((a) => (
            <li key={a.alert_id} data-testid="resolved-alert" className="rounded-md border p-3">
              <p className="font-medium">{a.title}</p>
              <p className="text-sm text-slate-500">
                {a.rule_id ?? "—"} · severity {a.severity_id} · score {a.final_score}
              </p>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
