import { Document, Page, StyleSheet, Text, View, renderToBuffer } from "@react-pdf/renderer";
import { type ReactElement, createElement as h } from "react";
import { getIncidentDetail } from "../read/queries.js";
import type { IncidentDetail, ResolvedAlert, TimelineEvent } from "../read/types.js";
import type { Services } from "../services.js";
import { type ForensicExport, buildForensicExport } from "./export.js";

// SPEC-013 — the forensic report render (escalón 4). GATE 1: the render module only; the HTTP
// route GET /v1/incidents/:id/report is gate 2. The PDF is BUILT programmatically via
// @react-pdf/renderer using React.createElement (NO JSX — tsconfig is untouched), NOT converted
// from HTML. React is confined to THIS module and consumed only through renderToBuffer. The
// report COMPOSES the read-layer outputs (getIncidentDetail + buildForensicExport) and
// TRANSCRIBES the SPEC-012 seal verbatim — it never re-derives the chain or re-signs. The
// forensic PRIVATE key is never referenced (the export carries only pubkey + signature).

/**
 * SPEC-013 §Data contracts §5 — the canonical integrity-not-authenticity limitation note,
 * rendered VISIBLY in the PDF (report_ac_005). It states what the seal proves (integrity under
 * the shown key) AND what it does not (authenticity — that needs out-of-band confirmation of the
 * pubkey; SPEC-012 §Open questions 1, deferred). Exported so tests assert its substance.
 */
export const INTEGRITY_LIMITATION_NOTE =
  "Integrity, not authenticity — this signature proves the evidence in this report is " +
  "intact under the key whose public half (forensic_pubkey) is shown above; it does not " +
  "prove that key is authentic. A verifier must confirm forensic_pubkey against a source " +
  "trusted independently of this server (out-of-band anchoring; pending a deployment " +
  "decision — SPEC-012 Open questions 1).";

const styles = StyleSheet.create({
  page: {
    paddingVertical: 28,
    paddingHorizontal: 32,
    fontFamily: "Helvetica",
    fontSize: 9,
    color: "#111111",
  },
  h1: { fontSize: 16, marginBottom: 10 },
  h2: { fontSize: 12, marginTop: 10, marginBottom: 4 },
  section: { marginBottom: 8 },
  mono: { fontFamily: "Courier", fontSize: 8 },
  note: { fontSize: 8, marginTop: 6, color: "#444444" },
});

function line(key: string, text: string): ReactElement {
  return h(Text, { key }, text);
}

function section(title: string, children: ReactElement[]): ReactElement {
  return h(View, { style: styles.section }, h(Text, { style: styles.h2 }, title), ...children);
}

function mitreLines(cg: IncidentDetail["cg_mitre"]): ReactElement[] {
  if (!cg || (cg.tactics.length === 0 && cg.techniques.length === 0)) {
    return [line("mitre-none", "none mapped")];
  }
  return [
    line("mitre-ta", `Tactics: ${cg.tactics.join(", ") || "—"}`),
    line("mitre-te", `Techniques: ${cg.techniques.join(", ") || "—"}`),
  ];
}

function eventLine(e: TimelineEvent, i: number): ReactElement {
  const ppid = e.process_parent_pid === null ? "—" : String(e.process_parent_pid);
  return line(
    `ev-${i}`,
    `${e.event_time}  ${e.process_name}  pid=${e.process_pid} ppid=${ppid} act=${e.activity_id}  ${e.image_file_name}`,
  );
}

function alertLine(a: ResolvedAlert, i: number): ReactElement {
  return line(
    `al-${i}`,
    `${a.title}  sev=${a.severity_id}  ${a.status}  rule=${a.rule_id ?? "—"}  score=${a.final_score}  ${a.event_time}`,
  );
}

function reportDocument(detail: IncidentDetail, exp: ForensicExport) {
  const events = exp.events;
  return h(
    Document,
    { title: `Forensic report ${detail.incident_id}` },
    h(
      Page,
      { size: "A4", style: styles.page },
      h(Text, { style: styles.h1 }, `Forensic report — ${detail.title}`),
      section("Incident", [
        line("inc-id", `Incident: ${detail.incident_id}`),
        line("inc-agent", `Agent: ${detail.agent_id}`),
        line("inc-status", `Status: ${detail.status}`),
        line("inc-sev", `Severity OCSF: ${detail.severity_id}`),
        line("inc-assignee", `Assigned to: ${detail.assigned_to ?? "unassigned"}`),
        line("inc-window", `Window start: ${detail.window_start}`),
        line("inc-times", `Created: ${detail.created_at}   Updated: ${detail.updated_at}`),
      ]),
      section("MITRE ATT&CK", mitreLines(detail.cg_mitre)),
      section(
        `Timeline (${events.length} event${events.length === 1 ? "" : "s"})`,
        events.length > 0 ? events.map(eventLine) : [line("ev-none", "No events.")],
      ),
      section(
        `Alerts (${detail.alerts.length})`,
        detail.alerts.length > 0 ? detail.alerts.map(alertLine) : [line("al-none", "No alerts.")],
      ),
      h(
        View,
        { style: styles.section },
        h(Text, { style: styles.h2 }, "Integrity"),
        h(Text, { key: "seal-cr", style: styles.mono }, `chain_root: ${exp.chain_root}`),
        h(Text, { key: "seal-rs", style: styles.mono }, `root_signature: ${exp.root_signature}`),
        h(Text, { key: "seal-pk", style: styles.mono }, `forensic_pubkey: ${exp.forensic_pubkey}`),
        h(Text, { key: "seal-note", style: styles.note }, INTEGRITY_LIMITATION_NOTE),
      ),
    ),
  );
}

/**
 * Render an already-composed report (fetched detail + export) to a PDF Buffer. PURE: no I/O, no
 * DB — so the render itself (including the null-`cg_mitre` degradation) is unit-testable without
 * a backend (report_ac_004 part 2; the seed helper cannot persist a null `cg_mitre`).
 */
export async function renderReport(detail: IncidentDetail, exp: ForensicExport): Promise<Buffer> {
  return renderToBuffer(reportDocument(detail, exp));
}

/**
 * Compose + render a per-incident forensic PDF. Fetches `IncidentDetail` (header / MITRE /
 * alerts) + `ForensicExport` (timeline + transcribed seal) — both org-scoped; returns null when
 * the incident does not exist or is cross-org (the gate-2 route maps null → 404). The seal is
 * transcribed from `buildForensicExport`, never re-derived.
 */
export async function buildReport(
  services: Pick<Services, "pg" | "ch" | "forensicKey">,
  orgId: string,
  incidentId: string,
): Promise<Buffer | null> {
  const detail = await getIncidentDetail(services.pg, orgId, incidentId);
  if (!detail) {
    return null;
  }
  const exp = await buildForensicExport(services, orgId, incidentId);
  if (!exp) {
    return null;
  }
  return renderReport(detail, exp);
}
