import { X509Certificate } from "node:crypto";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { issueToken } from "./helpers/db.js";
import {
  DAY_MS,
  type RenewAnswer,
  type RenewHarness,
  ageAgentCert,
  agentRow,
  buildRenewRequest,
  errorOf,
  postRenew,
  rawPubkey,
  signAgentCert,
  startRenewHarness,
  withPair,
  withPool,
} from "./helpers/renew.js";
import {
  type AgentIdentity,
  buildSignedEnvelope,
  enroll,
  generateKeyPair,
  postHeartbeat,
} from "./helpers/test-client.js";

// SPEC-025 renew_ac_008 (renewal) [server]; SC-F4-002. With 45 days left on the presented
// certificate, `200` with a certificate of `CN = agent_id`, the new key, a chain to the CA
// and a `notAfter` 90 days after its `notBefore`; the row's `pending_cert_pem` is that
// certificate, its `pubkey` and `cert_pem` unchanged. With the pending certificate presented
// and due, the row is promoted and its `pending_cert_pem` is the answer's. A heartbeat under
// the pending certificate whose write fails answers `503` and leaves both columns. With 46
// days, `409 renewal_not_due`; a bad `new_key_signature`, `401 bad_new_key_signature`; a
// replayed nonce, `409`; a stale `sent_at`, `422`; a second issuance within 10 minutes, `429`
// (§Operational §3, §4 and §5).

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

async function dueAgent(daysLeft: number): Promise<AgentIdentity> {
  const enrolled = await enroll(harness.enrollUrl, await issueToken(config));
  return ageAgentCert(config, harness.ca, enrolled, daysLeft);
}

async function renew(
  identity: AgentIdentity,
  newKeys: Awaited<ReturnType<typeof generateKeyPair>>,
  opts: Parameters<typeof buildRenewRequest>[2] = {},
) {
  return postRenew(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildRenewRequest(identity, newKeys, opts),
  });
}

/** The raw public key a certificate carries (Ed25519: the SPKI's last 32 bytes). */
function certRawKey(certPem: string): Buffer {
  const spki = new X509Certificate(certPem).publicKey.export({ type: "spki", format: "der" });
  return Buffer.from(spki.subarray(spki.length - 32));
}

test("renew_ac_008: with 45 days left, the answer is a pending certificate for the new key", async () => {
  const identity = await dueAgent(45);
  const before = await agentRow(config, identity.agentId);
  const newKeys = await generateKeyPair();
  const res = await renew(identity, newKeys);
  expect(res.status, res.bodyText).toBe(200);
  const answer = JSON.parse(res.bodyText) as RenewAnswer;
  expect(answer.agent_id).toBe(identity.agentId);
  expect(answer.envelope_version).toBe("0.1.0");

  const issued = new X509Certificate(answer.client_certificate);
  expect(issued.subject).toBe(`CN=${identity.agentId}`);
  expect(certRawKey(answer.client_certificate)).toEqual(await rawPubkey(newKeys));
  const ca = new X509Certificate(harness.caCertPem);
  expect(issued.checkIssued(ca)).toBe(true);
  expect(issued.verify(ca.publicKey)).toBe(true);
  expect(new Date(issued.validTo).getTime() - new Date(issued.validFrom).getTime()).toBe(
    90 * DAY_MS,
  );
  expect(answer.issued_at).toBe(new Date(issued.validFrom).toISOString());
  expect(answer.expires_at).toBe(new Date(issued.validTo).toISOString());

  const row = await agentRow(config, identity.agentId);
  expect(row?.pending_cert_pem).toBe(answer.client_certificate);
  expect(row?.pubkey).toEqual(before?.pubkey);
  expect(row?.cert_pem).toBe(before?.cert_pem);

  // A second certificate within 10 minutes is refused, before anything is signed or stored.
  const again = await renew(identity, await generateKeyPair());
  expect(again.status, again.bodyText).toBe(429);
  expect(errorOf(again)).toBe("rate_limited");
  expect((await agentRow(config, identity.agentId))?.pending_cert_pem).toBe(
    answer.client_certificate,
  );
});

test("renew_ac_008: a renewal under the pending certificate, due, promotes it and pends the new one", async () => {
  const identity = await dueAgent(40);
  // A pending certificate already due: the test signs it and records it as the renewal would.
  const pendingKeys = await generateKeyPair();
  const pending = await withPair(
    identity,
    await signAgentCert(harness.ca, identity.agentId, pendingKeys, {
      notBefore: new Date(Date.now() - 60_000),
      notAfter: new Date(Date.now() + 41 * DAY_MS),
    }),
    pendingKeys,
  );
  await withPool(config, (pool) =>
    pool.query("UPDATE agents SET pending_cert_pem = $2 WHERE agent_id = $1", [
      identity.agentId,
      pending.certPem,
    ]),
  );
  const newKeys = await generateKeyPair();
  const res = await renew(pending, newKeys);
  expect(res.status, res.bodyText).toBe(200);
  const answer = JSON.parse(res.bodyText) as RenewAnswer;
  const row = await agentRow(config, identity.agentId);
  expect(row?.cert_pem).toBe(pending.certPem);
  expect(row?.pubkey).toEqual(await rawPubkey(pendingKeys));
  expect(row?.pending_cert_pem).toBe(answer.client_certificate);
  expect(certRawKey(answer.client_certificate)).toEqual(await rawPubkey(newKeys));
});

test("renew_ac_008: a heartbeat under the pending certificate whose write fails answers 503 and changes nothing", async () => {
  const identity = await dueAgent(45);
  const newKeys = await generateKeyPair();
  const res = await renew(identity, newKeys);
  expect(res.status, res.bodyText).toBe(200);
  const fresh = await withPair(
    identity,
    (JSON.parse(res.bodyText) as RenewAnswer).client_certificate,
    newKeys,
  );
  const before = await agentRow(config, identity.agentId);
  // The test makes the agent's row refuse every update.
  const fn = `renew_ac_008_fail_${identity.agentId.replaceAll("-", "")}`;
  await withPool(config, async (pool) => {
    await pool.query(
      `CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$
       BEGIN RAISE EXCEPTION 'renew_ac_008 write refused'; END $$`,
    );
    await pool.query(
      `CREATE TRIGGER ${fn} BEFORE UPDATE ON agents FOR EACH ROW
       WHEN (OLD.agent_id = '${identity.agentId}') EXECUTE FUNCTION ${fn}()`,
    );
  });
  try {
    const hb = await postHeartbeat(harness.heartbeatUrl, {
      caCertPem: harness.caCertPem,
      identity: fresh,
      envelope: await buildSignedEnvelope(fresh),
    });
    expect(hb.status, hb.bodyText).toBe(503);
  } finally {
    await withPool(config, async (pool) => {
      await pool.query(`DROP TRIGGER IF EXISTS ${fn} ON agents`);
      await pool.query(`DROP FUNCTION IF EXISTS ${fn}()`);
    });
  }
  const after = await agentRow(config, identity.agentId);
  expect(after?.cert_pem).toBe(before?.cert_pem);
  expect(after?.pending_cert_pem).toBe(before?.pending_cert_pem);
  expect(after?.pubkey).toEqual(before?.pubkey);
});

test("renew_ac_008: with 46 days left, 409 renewal_not_due", async () => {
  const identity = await dueAgent(46);
  const res = await renew(identity, await generateKeyPair());
  expect(res.status, res.bodyText).toBe(409);
  expect(errorOf(res)).toBe("renewal_not_due");
  expect((await agentRow(config, identity.agentId))?.pending_cert_pem).toBeNull();
});

test("renew_ac_008: a bad new_key_signature, 401 bad_new_key_signature; its replay, 409", async () => {
  const identity = await dueAgent(45);
  const nonce = Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(16))).toString(
    "base64url",
  );
  const envelope = await buildRenewRequest(identity, await generateKeyPair(), {
    nonceB64url: nonce,
    badNewKeySignature: true,
  });
  const res = await postRenew(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope,
  });
  expect(res.status, res.bodyText).toBe(401);
  expect(errorOf(res)).toBe("bad_new_key_signature");

  const replay = await renew(identity, await generateKeyPair(), { nonceB64url: nonce });
  expect(replay.status, replay.bodyText).toBe(409);
  expect(errorOf(replay)).toBe("replay");
  expect((await agentRow(config, identity.agentId))?.pending_cert_pem).toBeNull();
});

test("renew_ac_008: a stale sent_at, 422", async () => {
  const identity = await dueAgent(45);
  const res = await renew(identity, await generateKeyPair(), {
    sentAt: new Date(Date.now() - 6 * 60 * 1000).toISOString(),
  });
  expect(res.status, res.bodyText).toBe(422);
  expect((await agentRow(config, identity.agentId))?.pending_cert_pem).toBeNull();
});

test("renew_ac_008: a signature by another key than the presented certificate's, 401", async () => {
  const identity = await dueAgent(45);
  const res = await renew(identity, await generateKeyPair(), {
    signingKey: await generateKeyPair(),
  });
  expect(res.status, res.bodyText).toBe(401);
  expect(errorOf(res)).toBe("bad_signature");
});
