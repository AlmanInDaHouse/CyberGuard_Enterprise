import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getHeartbeats, issueToken } from "./helpers/db.js";
import {
  type AgentIdentity,
  buildSignedEnvelope,
  enroll,
  postHeartbeat,
} from "./helpers/test-client.js";

// SPEC-019 net_ac_002 — validation. A POST with an invalid element is answered
// 400 invalid_request as a whole and stores nothing, its valid elements included
// (§Operational §6): an element whose class_uid is neither 1007 nor 4001; a 4001
// element without dst_endpoint; with a port of 65536; with an ip that is not an
// address; with activity_id 2; with a direction of lateral.

let config: Config;
let server: IngestServer;
let identity: AgentIdentity;
let sequence = 0;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
  identity = await enroll(server.enrollUrl, await issueToken(config));
});

afterAll(async () => {
  await server?.close();
});

/** 2026-10-10T00:00:00Z in Unix nanoseconds. */
const BASE_NANOS = 1_791_590_400_000_000_000n;

function launch(agentId: string): Record<string, unknown> {
  const nanos = (BASE_NANOS + 1_000n).toString();
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 1007,
    activity_id: 1,
    time: nanos,
    process: {
      pid: 4321,
      uid: `${agentId}:4321:${nanos}`,
      name: "curl.exe",
      created_time: nanos,
      parent_pid: 4,
      command_line: "",
      subject_user_sid: "",
      image_file_name: "C:\\Windows\\System32\\curl.exe",
    },
  };
}

function connection(): Record<string, unknown> {
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 4001,
    activity_id: 1,
    time: (BASE_NANOS + 2_000n).toString(),
    src_endpoint: { ip: "192.0.2.10", port: 49213 },
    dst_endpoint: { ip: "198.51.100.7", port: 443 },
    connection_info: { protocol_name: "tcp", direction: "outbound" },
    actor: { process: { pid: 4321 } },
  };
}

async function countEvents(agentId: string): Promise<number> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: "SELECT count() AS n FROM cges_events WHERE agent_id = {id:String}",
      query_params: { id: agentId },
      format: "JSONEachRow",
    });
    return Number((await rs.json<{ n: string }>())[0]?.n ?? 0);
  } finally {
    await ch.close();
  }
}

const INVALID: [string, (e: Record<string, unknown>) => Record<string, unknown>][] = [
  ["a class_uid other than 1007 and 4001", (e) => ({ ...e, class_uid: 3002 })],
  [
    "a 4001 element without dst_endpoint",
    (e) => {
      const { dst_endpoint: _omitted, ...rest } = e;
      return rest;
    },
  ],
  ["a port of 65536", (e) => ({ ...e, src_endpoint: { ip: "192.0.2.10", port: 65536 } })],
  ["an ip that is not an address", (e) => ({ ...e, dst_endpoint: { ip: "not-an-ip", port: 443 } })],
  ["activity_id 2", (e) => ({ ...e, activity_id: 2 })],
  [
    "a direction of lateral",
    (e) => ({ ...e, connection_info: { protocol_name: "tcp", direction: "lateral" } }),
  ],
];

test.each(INVALID)("net_ac_002: a POST with %s is refused whole", async (_name, invalidate) => {
  sequence += 1;
  const events = [launch(identity.agentId), connection(), invalidate(connection())];
  const result = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { sequenceNumber: sequence, events }),
  });
  expect(result.status, result.bodyText).toBe(400);
  expect(JSON.parse(result.bodyText)).toEqual({ error: "invalid_request" });
  // Nothing is stored: not the valid elements, not the heartbeat.
  expect(await countEvents(identity.agentId)).toBe(0);
  expect(await getHeartbeats(config, identity.agentId)).toEqual([]);
});
