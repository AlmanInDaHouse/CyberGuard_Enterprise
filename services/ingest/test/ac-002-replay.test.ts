import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getHeartbeats, issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// AC-002 — a heartbeat accepted once, replayed verbatim (same nonce), is
// rejected 409 and no second ClickHouse row appears.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("replayed nonce is rejected 409; no duplicate heartbeat row", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  const envelope = await buildSignedEnvelope(identity);

  const first = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope,
  });
  expect(first.status).toBe(200);

  const replay = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope,
  });
  expect(replay.status).toBe(409);

  const heartbeats = await getHeartbeats(config, identity.agentId);
  expect(heartbeats.length).toBe(1);
});
