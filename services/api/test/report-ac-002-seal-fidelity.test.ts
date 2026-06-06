import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildForensicExport } from "../src/forensic/export.js";
import { buildReport } from "../src/forensic/report.js";
import { buildServices } from "../src/services.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { pdfText, stripWs } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-013 report_ac_002 (SC-RPT-002) — seal fidelity. The integrity block in the rendered PDF
// equals EXACTLY the chain_root / root_signature / forensic_pubkey that buildForensicExport
// returns for the same incident: transcribed, not re-derived or altered. Binds the report to
// SPEC-012's chain of custody. (Ed25519 is deterministic, so both calls yield the same seal.)
// Asserts on whitespace-stripped extracted text so a wrapped long token still matches.

let services: Awaited<ReturnType<typeof buildServices>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
});

test("report_ac_002: the PDF integrity block transcribes the buildForensicExport seal verbatim", async () => {
  const org = "report-ac-002";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const e1 = globalThis.crypto.randomUUID();
  const e2 = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  await insertCgesEvent(config, {
    agentId,
    eventId: e1,
    orgId: org,
    time: "2026-06-05 09:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: e2,
    orgId: org,
    time: "2026-06-05 09:00:01.000000000",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [e1, e2] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  // The authoritative seal (the same function the report transcribes).
  const exp = await buildForensicExport(services, org, incidentId);
  expect(exp).not.toBeNull();
  if (!exp) return;

  const buf = await buildReport(services, org, incidentId);
  expect(buf).not.toBeNull();
  if (!buf) return;

  const stripped = stripWs(await pdfText(buf));
  // Each seal field appears verbatim in the PDF (whitespace-stripped to survive line reflow).
  expect(stripped).toContain(exp.chain_root);
  expect(stripped).toContain(exp.root_signature);
  expect(stripped).toContain(exp.forensic_pubkey);
  // Sanity: the seal fields are non-empty (the test is not vacuously passing).
  expect(exp.chain_root.length).toBe(64); // SHA-256 hex
  expect(exp.root_signature.length).toBeGreaterThan(0);
  expect(exp.forensic_pubkey.length).toBeGreaterThan(0);
});
