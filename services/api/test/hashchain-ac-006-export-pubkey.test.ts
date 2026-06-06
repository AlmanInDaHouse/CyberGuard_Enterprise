import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { computeChain, toHex } from "../src/forensic/hashchain.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-012 hashchain_ac_006 (SC-HC-006) — auditor-facing pubkey, server-side private. Over
// HTTP (GET /v1/incidents/:id/forensic-export), the export EMBEDS the forensic public key,
// a verifier using the embedded key validates root_signature, and the export (and the whole
// response) NEVER contains the private key.
//
// NOTE (§Compliance line 108): verifying against the embedded key is the AC's literal check
// of cryptographic consistency. It is NOT an endorsement of the verify-against-embedded
// anti-pattern — a real auditor anchors the key out-of-band (the deferred Open question).

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

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
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("hashchain_ac_006: export embeds the pubkey; embedded key validates root_signature", async () => {
  const org = "hashchain-ac-006";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const event1 = globalThis.crypto.randomUUID();
  const event2 = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  await insertCgesEvent(config, {
    agentId,
    eventId: event1,
    orgId: org,
    time: "2026-06-05 11:00:00.000000000",
    processPid: 5001,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: event2,
    orgId: org,
    time: "2026-06-05 11:00:02.000000000",
    processPid: 5002,
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [event1, event2] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-hc-ac006",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/forensic-export`,
    cookies: { cgsess: "tok-hc-ac006" },
  });

  expect(res.statusCode).toBe(200);
  const body = res.json();

  // Export shape (SPEC-012 §3).
  expect(body.incident_id).toBe(incidentId);
  expect(Array.isArray(body.events)).toBe(true);
  expect(body.events.length).toBe(2);
  expect(typeof body.chain_root).toBe("string");
  expect(typeof body.root_signature).toBe("string");
  expect(typeof body.forensic_pubkey).toBe("string");

  // The embedded pubkey is the raw 32-byte Ed25519 key (base64url).
  const embeddedPub = fromB64url(body.forensic_pubkey);
  expect(embeddedPub.length).toBe(32);

  // Recompute chain_N from the returned evidence and verify root_signature with the
  // EMBEDDED key (the AC's literal cryptographic-consistency check).
  const chainN = await computeChain(body.events);
  expect(toHex(chainN)).toBe(body.chain_root);
  const ok = await ed25519Verify(embeddedPub, fromB64url(body.root_signature), chainN);
  expect(ok).toBe(true);
});

test("hashchain_ac_006: the response NEVER contains the forensic private key", async () => {
  const org = "hashchain-ac-006-priv";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const eventId = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: org,
    time: "2026-06-05 12:00:00.000000000",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [eventId] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-hc-ac006-priv",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/forensic-export`,
    cookies: { cgsess: "tok-hc-ac006-priv" },
  });
  expect(res.statusCode).toBe(200);
  const raw = res.body; // the literal serialized response

  // No private-key field of any kind in the serialized response.
  expect(raw.toLowerCase()).not.toContain("private");

  // And the response does not contain the ACTUAL private-key material (the decrypted
  // PKCS#8 base64 read straight from Postgres).
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const dec = await pool.query<{ pk: string }>(
      "SELECT pgp_sym_decrypt(private_key, $1) AS pk FROM forensic_key WHERE id = 1",
      [config.API_FORENSIC_PASSPHRASE],
    );
    const privB64 = dec.rows[0]?.pk ?? "";
    expect(privB64.length).toBeGreaterThan(0);
    expect(raw).not.toContain(privB64);
  } finally {
    await pool.end();
  }
});
