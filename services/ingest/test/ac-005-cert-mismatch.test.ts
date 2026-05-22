import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// AC-005 — a heartbeat whose envelope agent_id does not equal the
// presented client certificate's CN is rejected 401 (ADR-0004 step 2).

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("envelope agent_id != client-cert CN is rejected 401", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  // Sign a well-formed envelope but claim a different agent_id than the cert CN.
  const envelope = await buildSignedEnvelope(identity, {
    agentIdOverride: "01934abc-def0-7000-89ab-0000000000ff",
  });

  const res = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope,
  });
  expect(res.status).toBe(401);
});
