import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// SPEC-019 net_ac_001 — a mixed POST persists. One Process Activity element and
// two Network Activity elements (one outbound over IPv4 with a uid, one inbound
// over IPv6 without) in one signed POST: answered 200; each 4001 row carries its
// element's endpoints, protocol, direction, pid, uid ('' when absent) and time;
// the 1007 row is stored as before with the six network columns at their
// defaults; the three rows share one arrived_at (§Data contracts, §Operational §6).

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

/** 2026-10-10T00:00:00Z in Unix nanoseconds. */
const BASE_NANOS = 1_791_590_400_000_000_000n;

interface StoredRow {
  class_uid: number;
  activity_id: number;
  process_pid: number;
  process_uid: string;
  process_name: string;
  image_file_name: string;
  src_ip: string;
  src_port: number;
  dst_ip: string;
  dst_port: number;
  net_protocol: string;
  net_direction: string;
  time_nanos: string;
  arrived_at: string;
}

async function storedRows(agentId: string): Promise<Map<string, StoredRow>> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: `
        SELECT toString(event_id) AS event_id, class_uid, activity_id, process_pid, process_uid,
               process_name, image_file_name, src_ip, src_port, dst_ip, dst_port, net_protocol,
               net_direction, toString(toUnixTimestamp64Nano(time)) AS time_nanos,
               toString(arrived_at) AS arrived_at
        FROM cges_events FINAL
        WHERE agent_id = {id:String}
      `,
      query_params: { id: agentId },
      format: "JSONEachRow",
    });
    const rows = await rs.json<StoredRow & { event_id: string }>();
    return new Map(rows.map(({ event_id, ...row }) => [event_id, row]));
  } finally {
    await ch.close();
  }
}

test("net_ac_001: a POST with one 1007 and two 4001 elements persists all three", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  const agentId = identity.agentId;

  const launchNanos = (BASE_NANOS + 1_000n).toString();
  const launch = {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 1007,
    activity_id: 1,
    time: launchNanos,
    process: {
      pid: 4321,
      uid: `${agentId}:4321:${launchNanos}`,
      name: "curl.exe",
      created_time: launchNanos,
      parent_pid: 4,
      command_line: "",
      subject_user_sid: "",
      image_file_name: "C:\\Windows\\System32\\curl.exe",
    },
  };
  const outbound = {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 4001,
    activity_id: 1,
    time: (BASE_NANOS + 2_345_678_901n).toString(),
    src_endpoint: { ip: "192.0.2.10", port: 49213 },
    dst_endpoint: { ip: "198.51.100.7", port: 443 },
    connection_info: { protocol_name: "tcp", direction: "outbound" },
    actor: { process: { pid: 4321, uid: `${agentId}:4321:${launchNanos}` } },
  };
  const inbound = {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 4001,
    activity_id: 1,
    time: (BASE_NANOS + 3_000_000_007n).toString(),
    src_endpoint: { ip: "2001:db8::7", port: 50123 },
    dst_endpoint: { ip: "2001:db8::10", port: 8443 },
    connection_info: { protocol_name: "tcp", direction: "inbound" },
    actor: { process: { pid: 1234 } },
  };

  const result = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, {
      sequenceNumber: 1,
      events: [launch, outbound, inbound],
    }),
  });
  expect(result.status, result.bodyText).toBe(200);

  const rows = await storedRows(agentId);
  expect(rows.size).toBe(3);

  expect(rows.get(outbound.event_id)).toMatchObject({
    class_uid: 4001,
    activity_id: 1,
    process_pid: 4321,
    process_uid: outbound.actor.process.uid,
    process_name: "",
    image_file_name: "",
    src_ip: "192.0.2.10",
    src_port: 49213,
    dst_ip: "198.51.100.7",
    dst_port: 443,
    net_protocol: "tcp",
    net_direction: "outbound",
    time_nanos: outbound.time,
  });
  expect(rows.get(inbound.event_id)).toMatchObject({
    class_uid: 4001,
    activity_id: 1,
    process_pid: 1234,
    process_uid: "",
    process_name: "",
    src_ip: "2001:db8::7",
    src_port: 50123,
    dst_ip: "2001:db8::10",
    dst_port: 8443,
    net_protocol: "tcp",
    net_direction: "inbound",
    time_nanos: inbound.time,
  });
  expect(rows.get(launch.event_id)).toMatchObject({
    class_uid: 1007,
    activity_id: 1,
    process_pid: 4321,
    process_uid: launch.process.uid,
    process_name: "curl.exe",
    image_file_name: "C:\\Windows\\System32\\curl.exe",
    src_ip: "",
    src_port: 0,
    dst_ip: "",
    dst_port: 0,
    net_protocol: "",
    net_direction: "",
    time_nanos: launchNanos,
  });

  // One INSERT per POST: the three rows share one arrived_at (SPEC-018 §Data contracts).
  const arrivals = new Set([...rows.values()].map((r) => r.arrived_at));
  expect(arrivals.size).toBe(1);
});
