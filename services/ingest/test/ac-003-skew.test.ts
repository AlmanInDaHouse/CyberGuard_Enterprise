import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getHeartbeats, issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// AC-003 — a heartbeat whose sent_at is outside ±5 min is rejected 422; no row.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("timestamp skew (6 min in the past) is rejected 422; no heartbeat row", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  const sixMinAgo = new Date(Date.now() - 6 * 60 * 1000).toISOString();
  const envelope = await buildSignedEnvelope(identity, { sentAt: sixMinAgo });

  const res = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope,
  });
  expect(res.status).toBe(422);

  const heartbeats = await getHeartbeats(config, identity.agentId);
  expect(heartbeats.length).toBe(0);
});
