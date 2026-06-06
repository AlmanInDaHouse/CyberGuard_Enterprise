import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildReport } from "../src/forensic/report.js";
import { buildServices } from "../src/services.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { norm, pdfText } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-013 report_ac_001 (SC-RPT-001) — complete composition. An incident seeded with events +
// alerts + severity + MITRE renders to a PDF whose extracted text carries every section: header
// (title/status/severity/assignee), MITRE, timeline, alerts, and the integrity block. Asserts by
// extracted content (unpdf), not bytes. CI-able, throwaway-DB, no marquee. Gate 1 (module-direct,
// no HTTP).

let services: Awaited<ReturnType<typeof buildServices>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
});

test("report_ac_001: the PDF composes every section", async () => {
  const org = "report-ac-001";
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
    time: "2026-06-05 10:00:00.000000000",
    processName: "winword.exe",
    processPid: 4444,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: e2,
    orgId: org,
    time: "2026-06-05 10:00:03.000000000",
    processName: "powershell.exe",
    processPid: 5555,
  });
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: org,
    title: "Office spawned a script host",
    severityId: 5,
    sourceEvents: [e1, e2],
    cgMitre: { tactics: ["execution"], techniques: ["T1059.001"] },
  });
  await insertIncidentRow(config, {
    incidentId,
    agentId,
    orgId: org,
    status: "investigating",
    assignedTo: "analyst-x",
    severityId: 5,
    title: "Execution on host",
    alertIds: [alertId],
    cgMitre: { tactics: ["execution"], techniques: ["T1059.001"] },
  });

  const buf = await buildReport(services, org, incidentId);
  expect(buf).not.toBeNull();
  if (!buf) return;

  // A valid PDF.
  expect(buf.subarray(0, 5).toString("latin1")).toBe("%PDF-");

  const t = norm(await pdfText(buf));
  // header
  expect(t).toContain("execution on host"); // title
  expect(t).toContain("status: investigating");
  expect(t).toContain("severity ocsf: 5");
  expect(t).toContain("analyst-x"); // assignee
  // MITRE
  expect(t).toContain("execution");
  expect(t).toContain("t1059.001");
  // timeline (both events, in the section)
  expect(t).toContain("timeline (2 events)");
  expect(t).toContain("winword.exe");
  expect(t).toContain("powershell.exe");
  // alerts
  expect(t).toContain("alerts (1)");
  expect(t).toContain("office spawned a script host");
  // integrity block labels
  expect(t).toContain("chain_root");
  expect(t).toContain("root_signature");
  expect(t).toContain("forensic_pubkey");
});
