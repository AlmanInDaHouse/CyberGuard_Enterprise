import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { computeChain, toHex } from "../src/forensic/hashchain.js";
import { composeReport } from "../src/forensic/report.js";
import { buildServices } from "../src/services.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { pdfText, stripWs } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-013 report_ac_006 — the Amendment 2026-10-16 (debt #9): the report reads the
// incident and its alerts once, and seals the timeline of that read. An alert that joins
// the incident after the report has read it is in neither the alert table nor the seal.

let services: Awaited<ReturnType<typeof buildServices>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
});

/** Verifies with the server's own forensic public key, never the one the document embeds
 *  (SPEC-012 §Compliance). */
async function verifies(chainRootHex: string, signatureB64: string) {
  const key = await globalThis.crypto.subtle.importKey(
    "raw",
    services.forensicKey.publicKeyRaw,
    { name: "Ed25519" },
    false,
    ["verify"],
  );
  return globalThis.crypto.subtle.verify(
    { name: "Ed25519" },
    key,
    Buffer.from(signatureB64, "base64url"),
    Buffer.from(chainRootHex, "hex"),
  );
}

test("report_ac_006: an alert that joins after the report's read is in neither its table nor its seal", async () => {
  const org = "report-ac-006";
  const agentId = globalThis.crypto.randomUUID();
  const first = globalThis.crypto.randomUUID();
  const late = globalThis.crypto.randomUUID();
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
  await insertAlertRow(config, {
    alertId: first,
    agentId,
    orgId: org,
    sourceEvents: [e1],
    title: "The first alert",
  });
  await insertAlertRow(config, {
    alertId: late,
    agentId,
    orgId: org,
    sourceEvents: [e2],
    title: "The late alert",
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [first] });

  // A pool whose every read of the incident's row is counted, and after whose first one
  // a detection cycle adds the late alert to the incident, as upsertIncident appends.
  const writer = new pg.Pool({ connectionString: config.API_PG_URL });
  let incidentReads = 0;
  let alertReads = 0;
  const racing = new Proxy(services.pg, {
    get(target, prop, receiver) {
      if (prop !== "query") return Reflect.get(target, prop, receiver);
      return async (text: unknown, ...rest: unknown[]) => {
        const result = await (target.query as (...a: unknown[]) => Promise<unknown>).call(
          target,
          text,
          ...rest,
        );
        if (typeof text === "string" && /FROM ingest\.alerts\s+WHERE alert_id = ANY/.test(text))
          alertReads += 1;
        if (typeof text === "string" && /FROM ingest\.incidents WHERE incident_id/.test(text)) {
          incidentReads += 1;
          if (incidentReads === 1) {
            await writer.query(
              "UPDATE ingest.incidents SET alert_ids = alert_ids || $1::uuid WHERE incident_id = $2",
              [late, incidentId],
            );
          }
        }
        return result;
      };
    },
  });

  try {
    const report = await composeReport({ ...services, pg: racing }, org, incidentId);
    expect(report).not.toBeNull();
    if (!report) return;

    expect(report.seal.events.map((e) => e.event_id)).toEqual([e1]);
    expect(report.seal.chain_root).toBe(toHex(await computeChain(report.seal.events)));
    expect(await verifies(report.seal.chain_root, report.seal.root_signature)).toBe(true);
    const text = await pdfText(report.pdf);
    expect(text).toContain("The first alert");
    expect(text).not.toContain("The late alert");
    // The PDF itself: one timeline row, the first alert's event with its agent, not the
    // late one's; and the integrity block it prints is the seal of that one event.
    expect(text).toContain("Timeline (1 event)");
    expect(stripWs(text)).toContain(stripWs(`agent=${agentId}`));
    expect(text).not.toContain("09:00:01");
    expect(stripWs(text)).toContain(report.seal.chain_root);
    expect(incidentReads).toBe(1);
    expect(alertReads).toBe(1);
  } finally {
    await writer.end();
  }
});
