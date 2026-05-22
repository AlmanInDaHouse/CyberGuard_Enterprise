import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getHeartbeats, issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// AC-004 — a heartbeat whose signature does not verify is rejected 401; no row.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("tampered signature is rejected 401; no heartbeat row", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  const envelope = await buildSignedEnvelope(identity);

  const res = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope,
    tamperSignature: true,
  });
  expect(res.status).toBe(401);

  const heartbeats = await getHeartbeats(config, identity.agentId);
  expect(heartbeats.length).toBe(0);
});
