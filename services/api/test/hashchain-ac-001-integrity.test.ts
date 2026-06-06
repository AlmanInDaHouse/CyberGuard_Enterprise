import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildForensicExport } from "../src/forensic/export.js";
import { computeChain, toHex } from "../src/forensic/hashchain.js";
import { buildServices } from "../src/services.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-012 hashchain_ac_001 (SC-HC-001) — verifiable integrity. For a real incident, the
// snapshot export's chain head recomputes byte-for-byte from the canonicalized evidence,
// and the Ed25519 root_signature verifies against the forensic public key. This exercises
// the gate-C crypto path PURELY (no HTTP): buildForensicExport → recompute → verify.
//
// The signature is verified against the key obtained OUT-OF-BAND from the service
// (services.forensicKey.publicKeyRaw), NOT the embedded one — the correct pattern per
// §Compliance line 108 (the embedded pubkey is a convenience, not a trust root). The test
// also asserts the embedded pubkey equals that out-of-band key.

let services: Awaited<ReturnType<typeof buildServices>>;
let config: Config;

/** Ed25519-verify `sig` over `msg` under a raw 32-byte public key. */
async function ed25519Verify(
  pubRaw: Uint8Array,
  sig: Uint8Array,
  msg: Uint8Array,
): Promise<boolean> {
  const key = await globalThis.crypto.subtle.importKey("raw", pubRaw, { name: "Ed25519" }, false, [
    "verify",
  ]);
  return globalThis.crypto.subtle.verify({ name: "Ed25519" }, key, sig, msg);
}

function fromB64url(s: string): Uint8Array {
  return new Uint8Array(Buffer.from(s, "base64url"));
}

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
});

test("hashchain_ac_001: chain recomputes and root_signature verifies for a real incident", async () => {
  const org = "hashchain-ac-001";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const eventEarly = globalThis.crypto.randomUUID();
  const eventLate = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  // Insert LATE first — the export's total order (time ASC, event_id ASC) must not depend
  // on insert order (the determinism the chain rests on).
  await insertCgesEvent(config, {
    agentId,
    eventId: eventLate,
    orgId: org,
    time: "2026-06-05 10:00:05.000000000",
    processName: "powershell.exe",
    processPid: 4322,
    processParentPid: 1000,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: eventEarly,
    orgId: org,
    time: "2026-06-05 10:00:00.000000000",
    processName: "powershell.exe",
    processPid: 4321,
    processParentPid: 1000,
  });
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: org,
    sourceEvents: [eventEarly, eventLate],
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  const exp = await buildForensicExport(services, org, incidentId);
  expect(exp).not.toBeNull();
  if (!exp) return;

  // Two events, in total order (time ASC, event_id ASC) — independent of insert order.
  expect(exp.events.map((e) => e.event_id)).toEqual([eventEarly, eventLate]);

  // Recompute the chain head from the exported (canonicalized) evidence → matches chain_root.
  const recomputed = await computeChain(exp.events);
  expect(toHex(recomputed)).toBe(exp.chain_root);

  // Ed25519-verify the root_signature over the 32 bytes of chain_N, under the trusted key
  // obtained out-of-band from the service (NOT the embedded one).
  const trustedPub = services.forensicKey.publicKeyRaw;
  const sig = fromB64url(exp.root_signature);
  expect(await ed25519Verify(trustedPub, sig, recomputed)).toBe(true);

  // The embedded pubkey equals the out-of-band trusted key.
  expect(exp.forensic_pubkey).toBe(Buffer.from(trustedPub).toString("base64url"));

  // The signature binds the EXACT evidence: a one-byte mutation of an event breaks the
  // recompute-and-verify (the chain head changes, so the original signature no longer
  // verifies over it).
  const tampered = exp.events.map((e, i) =>
    i === 0 ? { ...e, process_pid: e.process_pid + 1 } : e,
  );
  const tamperedChain = await computeChain(tampered);
  expect(toHex(tamperedChain)).not.toBe(exp.chain_root);
  expect(await ed25519Verify(trustedPub, sig, tamperedChain)).toBe(false);
});

test('hashchain_ac_001: empty-evidence incident signs chain_0 = SHA-256("")', async () => {
  // An incident whose drill resolves to ZERO events: N=0, chain_N = chain_0 = SHA-256("").
  const org = "hashchain-ac-001-empty";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  // Alert with source_events that point at NO existing cges_events → empty timeline.
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: org,
    sourceEvents: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  const exp = await buildForensicExport(services, org, incidentId);
  expect(exp).not.toBeNull();
  if (!exp) return;

  expect(exp.events.length).toBe(0);
  const chain0 = await computeChain([]);
  expect(exp.chain_root).toBe(toHex(chain0));
  const ok = await ed25519Verify(
    services.forensicKey.publicKeyRaw,
    fromB64url(exp.root_signature),
    chain0,
  );
  expect(ok).toBe(true);
});
