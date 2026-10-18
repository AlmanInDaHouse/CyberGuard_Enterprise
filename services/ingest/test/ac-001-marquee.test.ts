import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getAgent, getHeartbeats, issueToken } from "./helpers/db.js";
import { isElevatedWindows, prepareAgent } from "./helpers/marquee-agent.js";

// AC-001 — MARQUEE (end-to-end loop). The real compiled cg-agent enrolls
// and heartbeats against the real ingest server backed by real
// Postgres/ClickHouse/Redis, and we assert the actual rows. Nothing here
// is mocked — agent, TLS, signature, and databases are all real. RED in
// B4 (startIngest throws); B5 makes it pass without redesigning the test.
//
// On Windows the agent's secure path opens the ETW session first, and an
// unelevated agent exits with code 9 before any heartbeat (SPEC-017
// §Operational §1): there this marquee runs only elevated, with the
// developer-local gate. It runs everywhere else, CI included.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test.skipIf(process.platform === "win32" && !isElevatedWindows())(
  "real cg-agent enrolls + heartbeats -> rows in Postgres agents + ClickHouse heartbeats",
  async () => {
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
      agent_pubkey_fingerprint: string;
    };

    // SPEC-004 AC-001 (a): the row's pubkey is the agent's. identity.json carries the
    // SHA-256 of the agent's 32-byte public key, in lowercase hex (SPEC-002).
    const agentRow = await getAgent(config, identity.agent_id);
    expect(agentRow, "Postgres agents row exists").not.toBeNull();
    expect(agentRow?.pubkey.length, "stored pubkey is 32 bytes").toBe(32);
    expect(
      createHash("sha256")
        .update(agentRow?.pubkey ?? Buffer.alloc(0))
        .digest("hex"),
      "stored pubkey is the agent's",
    ).toBe(identity.agent_pubkey_fingerprint);
    expect(agentRow?.last_seen, "last_seen set by the heartbeat").not.toBeNull();

    // SPEC-004 AC-001 (b): a row with sequence_number 1 and status online, looked up by
    // both, not taken as the first row by arrived_at. ClickHouse stamps arrived_at
    // (DEFAULT now64(3)) and the agent heartbeats every second, and on Docker Desktop
    // that clock at times steps back by more than a second (debt #41), which puts a
    // later heartbeat first (debt #27).
    const heartbeats = await getHeartbeats(config, identity.agent_id);
    expect(
      heartbeats.some((h) => h.sequence_number === "1" && h.status === "online"),
      "a heartbeat with sequence_number 1 and status online",
    ).toBe(true);
  },
);
