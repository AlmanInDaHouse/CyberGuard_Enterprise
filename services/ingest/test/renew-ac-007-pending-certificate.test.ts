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
  derOf,
  errorOf,
  postRenew,
  rawPubkey,
  serialOf,
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

// SPEC-025 renew_ac_007 (the pending certificate, another one) [server]; SC-F4-002 and
// SC-F4-005. An agent enrolled, renewed, and then heartbeating under the new certificate: the
// row holds the new key and certificate and no pending one, and the log has `agent
// certificate promoted`; a heartbeat under the old one answers `403 agent_superseded` and
// sets `duplicate_signal = 'superseded_certificate'`. A certificate the CA's key signed by no
// renewal answers `403 agent_superseded` and changes neither `pubkey` nor `cert_pem`. A
// pending certificate whose key does not verify the envelope answers `401` and changes
// nothing. A database restored to the row before the renewal refuses the new certificate
// (§Operational §3 and §7).

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

/** An enrolled agent whose certificate is due, renewed once: its old and its new pairs. */
async function renewed(): Promise<{ old: AgentIdentity; fresh: AgentIdentity }> {
  const enrolled = await enroll(harness.enrollUrl, await issueToken(config));
  const old = await ageAgentCert(config, harness.ca, enrolled, 40);
  const newKeys = await generateKeyPair();
  const res = await postRenew(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity: old,
    envelope: await buildRenewRequest(old, newKeys),
  });
  expect(res.status, res.bodyText).toBe(200);
  const answer = JSON.parse(res.bodyText) as RenewAnswer;
  return { old, fresh: await withPair(old, answer.client_certificate, newKeys) };
}

async function heartbeat(identity: AgentIdentity, sequence = 1) {
  return postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { sequenceNumber: sequence }),
  });
}

test("renew_ac_007: the first heartbeat under the pending certificate makes it the agent's; the old one is then superseded", async () => {
  const { old, fresh } = await renewed();
  const pending = await agentRow(config, old.agentId);
  expect(pending?.pending_cert_pem).toBe(fresh.certPem);

  const accepted = await heartbeat(fresh);
  expect(accepted.status, accepted.bodyText).toBe(200);
  const row = await agentRow(config, old.agentId);
  expect(row?.pubkey).toEqual(await rawPubkey(fresh.keyPair));
  expect(row?.cert_pem).toBe(fresh.certPem);
  expect(row?.pending_cert_pem).toBeNull();
  expect((row?.expires_at as Date).getTime()).toBe(
    new Date(new X509Certificate(fresh.certPem).validTo).getTime(),
  );
  const promoted = harness.logs.filter(
    (l) => l.msg === "agent certificate promoted" && l.agent_id === old.agentId,
  );
  expect(promoted).toHaveLength(1);
  expect(promoted[0]).toMatchObject({
    old_serial: serialOf(old.certPem),
    new_serial: serialOf(fresh.certPem),
  });

  const superseded = await heartbeat(old, 2);
  expect(superseded.status, superseded.bodyText).toBe(403);
  expect(errorOf(superseded)).toBe("agent_superseded");
  const flagged = await agentRow(config, old.agentId);
  expect(flagged?.duplicate_signal).toBe("superseded_certificate");
  expect(flagged?.duplicate_signal_at).toBeInstanceOf(Date);
  expect(
    harness.logs.some(
      (l) =>
        l.msg === "agent duplicate suspected" &&
        l.agent_id === old.agentId &&
        l.signal === "superseded_certificate",
    ),
  ).toBe(true);
  // The agent's certificate stayed the new one.
  expect(flagged?.cert_pem).toBe(fresh.certPem);
});

test("renew_ac_007: a certificate the CA's key signed by no renewal is superseded and changes nothing", async () => {
  const identity = await enroll(harness.enrollUrl, await issueToken(config));
  const before = await agentRow(config, identity.agentId);
  const forgedKeys = await generateKeyPair();
  const forged = await withPair(
    identity,
    await signAgentCert(harness.ca, identity.agentId, forgedKeys, {
      notBefore: new Date(Date.now() + 1000 - 30_000),
      notAfter: new Date(Date.now() + 200 * DAY_MS),
    }),
    forgedKeys,
  );
  const res = await heartbeat(forged);
  expect(res.status, res.bodyText).toBe(403);
  expect(errorOf(res)).toBe("agent_superseded");
  const after = await agentRow(config, identity.agentId);
  expect(after?.pubkey).toEqual(before?.pubkey);
  expect(after?.cert_pem).toBe(before?.cert_pem);
  expect(after?.duplicate_signal).toBe("superseded_certificate");
});

test("renew_ac_007: a pending certificate whose key does not verify the envelope answers 401 and changes nothing", async () => {
  const { old, fresh } = await renewed();
  const before = await agentRow(config, old.agentId);
  // The handshake is under the pending pair; the envelope is signed by another key.
  const stranger = await generateKeyPair();
  const res = await postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity: fresh,
    envelope: await buildSignedEnvelope({ ...fresh, keyPair: stranger }),
  });
  expect(res.status, res.bodyText).toBe(401);
  expect(errorOf(res)).toBe("bad_signature");
  const after = await agentRow(config, old.agentId);
  expect(after?.pubkey).toEqual(before?.pubkey);
  expect(after?.cert_pem).toBe(before?.cert_pem);
  expect(after?.pending_cert_pem).toBe(before?.pending_cert_pem);
  expect(after?.duplicate_signal).toBeNull();
});

test("renew_ac_007: a database restored to before the renewal refuses the new certificate as superseded", async () => {
  const enrolled = await enroll(harness.enrollUrl, await issueToken(config));
  const old = await ageAgentCert(config, harness.ca, enrolled, 40);
  const backup = await agentRow(config, old.agentId);
  const newKeys = await generateKeyPair();
  const res = await postRenew(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity: old,
    envelope: await buildRenewRequest(old, newKeys),
  });
  expect(res.status, res.bodyText).toBe(200);
  const fresh = await withPair(
    old,
    (JSON.parse(res.bodyText) as RenewAnswer).client_certificate,
    newKeys,
  );
  // The restore: the row as the backup held it, with no pending certificate.
  await withPool(config, (pool) =>
    pool.query(
      `UPDATE agents SET pubkey = $2, cert_pem = $3, expires_at = $4, pending_cert_pem = NULL
        WHERE agent_id = $1`,
      [old.agentId, backup?.pubkey, backup?.cert_pem, backup?.expires_at],
    ),
  );
  const refused = await heartbeat(fresh);
  expect(refused.status, refused.bodyText).toBe(403);
  expect(errorOf(refused)).toBe("agent_superseded");
  expect(derOf((await agentRow(config, old.agentId))?.cert_pem as string)).toEqual(
    derOf(old.certPem),
  );
});
