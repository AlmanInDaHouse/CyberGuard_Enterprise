import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { INTEGRITY_LIMITATION_NOTE, buildReport } from "../src/forensic/report.js";
import { buildServices } from "../src/services.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { norm, pdfText } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-013 report_ac_005 (SC-RPT-005) — the visible limitation note. The rendered PDF carries the
// integrity-not-authenticity note in the integrity block (the provisional mitigation while
// out-of-band anchoring stays deferred — SPEC-012 §Open questions 1). Verified by extracting the
// PDF text and asserting the note's distinctive phrases (robust to PDF line reflow).

let services: Awaited<ReturnType<typeof buildServices>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
});

test("report_ac_005: the PDF contains the integrity-not-authenticity limitation note", async () => {
  const org = "report-ac-005";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const e1 = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  await insertCgesEvent(config, {
    agentId,
    eventId: e1,
    orgId: org,
    time: "2026-06-05 08:00:00.000000000",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [e1] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  const buf = await buildReport(services, org, incidentId);
  expect(buf).not.toBeNull();
  if (!buf) return;

  const t = norm(await pdfText(buf));
  // Distinctive phrases of the canonical note (whitespace-collapsed, lowercased).
  expect(t).toContain("integrity, not authenticity");
  expect(t).toContain("does not");
  expect(t).toContain("out-of-band");
  expect(t).toContain("trusted independently of this server");

  // The note states BOTH what the seal proves and what it does not (ontological precision).
  const note = norm(INTEGRITY_LIMITATION_NOTE);
  expect(note).toContain("proves the evidence");
  expect(note).toContain("does not prove that key is authentic");
});
