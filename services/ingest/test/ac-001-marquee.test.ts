import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getAgent, getHeartbeats, issueToken } from "./helpers/db.js";
import { prepareAgent } from "./helpers/marquee-agent.js";

// AC-001 — MARQUEE (end-to-end loop). The real compiled cg-agent enrolls
// and heartbeats against the real ingest server backed by real
// Postgres/ClickHouse/Redis, and we assert the actual rows. Nothing here
// is mocked — agent, TLS, signature, and databases are all real. RED in
// B4 (startIngest throws); B5 makes it pass without redesigning the test.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("real cg-agent enrolls + heartbeats -> rows in Postgres agents + ClickHouse heartbeats", async () => {
  const token = await issueToken(config);

  const agent = prepareAgent({
    enrollUrl: server.enrollUrl,
    heartbeatUrl: server.heartbeatUrl,
    caCertPem: server.caCertPem,
    token,
  });
  const result = await agent.run();
  expect(result.stderr, "agent must not panic/crash").not.toContain("panic");

  // The agent persisted its server-assigned identity; read its agent_id back.
  const identity = JSON.parse(readFileSync(join(agent.identityDir, "identity.json"), "utf8")) as {
    agent_id: string;
  };

  const agentRow = await getAgent(config, identity.agent_id);
  expect(agentRow, "Postgres agents row exists").not.toBeNull();
  expect(agentRow?.pubkey.length, "stored pubkey is 32 bytes").toBe(32);
  expect(agentRow?.last_seen, "last_seen set by the heartbeat").not.toBeNull();

  const heartbeats = await getHeartbeats(config, identity.agent_id);
  expect(heartbeats.length, "at least one ClickHouse heartbeat row").toBeGreaterThanOrEqual(1);
  expect(heartbeats[0]?.sequence_number, "first heartbeat sequence_number = 1").toBe("1");
  expect(heartbeats[0]?.status, "status online").toBe("online");
});
