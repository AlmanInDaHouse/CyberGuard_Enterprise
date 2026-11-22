import { Redis } from "ioredis";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { getCgesEvents, getHeartbeats, issueToken } from "./helpers/db.js";
import {
  type RenewHarness,
  ageAgentCert,
  buildRenewRequest,
  errorOf,
  postRenew,
  processEvent,
  startRenewHarness,
  withPool,
} from "./helpers/renew.js";
import {
  buildSignedEnvelope,
  enroll,
  generateKeyPair,
  postHeartbeat,
} from "./helpers/test-client.js";

// SPEC-025 renew_ac_006 (a revoked agent) [server]; SC-F4-004. A heartbeat and a renewal of
// a revoked agent answer `403 agent_revoked`, write no heartbeat row and no event, and spend
// no nonce: Redis holds no `nonce:` key for the request (§Operational §2, step 2).

let config: Config;
let harness: RenewHarness;
let redis: Redis;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
  redis = new Redis(config.INGEST_REDIS_URL, { maxRetriesPerRequest: 1 });
});

afterAll(async () => {
  await redis?.quit();
  await harness?.close();
});

function nonce(): string {
  return Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(16))).toString("base64url");
}

test("renew_ac_006: a revoked agent's heartbeat and renewal answer 403 agent_revoked and spend no nonce", async () => {
  const enrolled = await enroll(harness.enrollUrl, await issueToken(config));
  // Due for renewal, so that only the revocation refuses it.
  const identity = await ageAgentCert(config, harness.ca, enrolled, 30);
  await withPool(config, (pool) =>
    pool.query(
      "UPDATE agents SET revoked_at = now(), revocation_reason = 'renew_ac_006' WHERE agent_id = $1",
      [identity.agentId],
    ),
  );

  const heartbeatNonce = nonce();
  const eventId = globalThis.crypto.randomUUID();
  const heartbeat = await postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, {
      nonceB64url: heartbeatNonce,
      events: [processEvent(eventId)],
    }),
  });
  expect(heartbeat.status, heartbeat.bodyText).toBe(403);
  expect(JSON.parse(heartbeat.bodyText)).toEqual({ error: "agent_revoked" });

  const renewNonce = nonce();
  const renewal = await postRenew(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildRenewRequest(identity, await generateKeyPair(), {
      nonceB64url: renewNonce,
    }),
  });
  expect(renewal.status, renewal.bodyText).toBe(403);
  expect(errorOf(renewal)).toBe("agent_revoked");

  expect(await redis.exists(`nonce:${heartbeatNonce}`)).toBe(0);
  expect(await redis.exists(`nonce:${renewNonce}`)).toBe(0);
  expect(await getHeartbeats(config, identity.agentId)).toEqual([]);
  expect(await getCgesEvents(config, identity.agentId)).toEqual([]);
  // Nothing was renewed.
  const row = await withPool(config, (pool) =>
    pool.query<{ pending_cert_pem: string | null }>(
      "SELECT pending_cert_pem FROM agents WHERE agent_id = $1",
      [identity.agentId],
    ),
  );
  expect(row.rows[0]?.pending_cert_pem).toBeNull();
});
