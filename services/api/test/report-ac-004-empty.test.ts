import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type ForensicExport, buildForensicExport } from "../src/forensic/export.js";
import { computeChain, toHex } from "../src/forensic/hashchain.js";
import { buildReport, renderReport } from "../src/forensic/report.js";
import type { IncidentDetail } from "../src/read/types.js";
import { buildServices } from "../src/services.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { norm, pdfText, stripWs } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-013 report_ac_004 (SC-RPT-004) — empty-evidence edge. Two facets:
//  (1) An incident that resolves to ZERO events (helpers: alert with source_events pointing at no
//      cges_events row) → buildReport produces a valid, non-crashing PDF whose integrity block
//      carries the empty-evidence seal over chain_0 = SHA-256("") (coherent with SPEC-012 N=0).
//  (2) A null cg_mitre degrades cleanly ("none mapped"). The seed helper cannot persist a null
//      cg_mitre (it `?? `-defaults), so this facet drives renderReport directly with a hand-built
//      detail — the pure render path, no DB.

let services: Awaited<ReturnType<typeof buildServices>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
});

test("report_ac_004 (1): empty-events incident → valid PDF with the chain_0 seal", async () => {
  const org = "report-ac-004";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  // Alert whose source_events point at a cges_events.event_id that does NOT exist → empty timeline.
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: org,
    sourceEvents: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  const exp = await buildForensicExport(services, org, incidentId);
  expect(exp).not.toBeNull();
  expect(exp?.events.length).toBe(0);

  const chain0Hex = toHex(await computeChain([]));
  expect(exp?.chain_root).toBe(chain0Hex); // N=0 → chain_0 = SHA-256("")

  const buf = await buildReport(services, org, incidentId);
  expect(buf).not.toBeNull();
  if (!buf) return;
  expect(buf.subarray(0, 5).toString("latin1")).toBe("%PDF-"); // valid, no crash

  expect(stripWs(await pdfText(buf))).toContain(chain0Hex); // chain_0 seal rendered
  expect(norm(await pdfText(buf))).toContain("no events."); // empty timeline rendered
});

test('report_ac_004 (2): null cg_mitre degrades cleanly to "none mapped"', async () => {
  const detail: IncidentDetail = {
    incident_id: globalThis.crypto.randomUUID(),
    agent_id: globalThis.crypto.randomUUID(),
    status: "open",
    title: "Empty incident",
    severity_id: 0,
    cg_mitre: null,
    window_start: "2026-06-05T10:00:00.000Z",
    assigned_to: null,
    created_at: "2026-06-05T10:00:00.000Z",
    updated_at: "2026-06-05T10:00:00.000Z",
    alerts: [],
  };
  const chain0Hex = toHex(await computeChain([]));
  const exp: ForensicExport = {
    incident_id: detail.incident_id,
    events: [],
    chain_root: chain0Hex,
    root_signature: "c2lnbmF0dXJl", // placeholder base64url; not under test here
    forensic_pubkey: "cHVibGlja2V5",
  };

  const buf = await renderReport(detail, exp);
  expect(buf.subarray(0, 5).toString("latin1")).toBe("%PDF-");
  const t = norm(await pdfText(buf));
  expect(t).toContain("none mapped"); // MITRE null degraded
  expect(t).toContain("no events.");
  expect(t).toContain("no alerts.");
  expect(t).toContain("unassigned"); // null assigned_to degraded
});
