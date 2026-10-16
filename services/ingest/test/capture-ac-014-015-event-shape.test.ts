import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { getHeartbeats, issueToken } from "./helpers/db.js";
import {
  type AgentIdentity,
  buildSignedEnvelope,
  enroll,
  postHeartbeat,
} from "./helpers/test-client.js";

// SPEC-017 capture_ac_014 and 015 — the Amendment 2026-10-16 (debt #31's shape half):
// an element's `time`, and a non-null `created_time`, are 1 to 19 decimal digits without
// a leading 0, below 2^63; a 1007 `event_id` is a UUID, its pids fit UInt32 and its exit_code Int32. Any
// other value refuses the POST whole (SPEC-019 §Operational §6); the edges are stored.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

/** A minute before now, in Unix nanoseconds. */
function recent(): string {
  return (BigInt(Date.now() - 60_000) * 1_000_000n).toString();
}

type Element = Record<string, unknown>;

function launch(agentId: string, over: { time?: string; process?: Element } = {}): Element {
  const time = over.time ?? recent();
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 1007,
    activity_id: 1,
    time,
    process: {
      pid: 4321,
      uid: `${agentId}:4321:${time}`,
      name: "probe.exe",
      created_time: time,
      parent_pid: 4,
      command_line: "",
      subject_user_sid: "",
      image_file_name: "C:\\Probe\\probe.exe",
      ...over.process,
    },
  };
}

function terminate(agentId: string, exitCode: number): Element {
  const e = launch(agentId, { process: { exit_code: exitCode } });
  return { ...e, activity_id: 2 };
}

function connection(time: string): Element {
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 4001,
    activity_id: 1,
    time,
    src_endpoint: { ip: "192.0.2.10", port: 49213 },
    dst_endpoint: { ip: "198.51.100.7", port: 443 },
    connection_info: { protocol_name: "tcp", direction: "outbound" },
    actor: { process: { pid: 4321 } },
  };
}

function logon(time: string): Element {
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 3002,
    category_uid: 3,
    activity_id: 1,
    time,
    status_id: 1,
    user: { uid: "S-1-5-21-1111-2222-3333-1001", name: "capture-ac-014", domain: "WS-0042" },
    logon_type_id: 2,
    auth_protocol: "Negotiate",
    auth_protocol_id: 99,
  };
}

interface Row {
  event_id: string;
  time_nanos: string;
  created_nanos: string | null;
  pid: number;
  parent_pid: number | null;
  exit_code: number | null;
}

async function rowsOf(agentId: string): Promise<Row[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: `SELECT toString(event_id) AS event_id,
                     toString(toUnixTimestamp64Nano(time)) AS time_nanos,
                     if(isNull(process_created_time), NULL,
                        toString(toUnixTimestamp64Nano(assumeNotNull(process_created_time)))) AS created_nanos,
                     process_pid AS pid, process_parent_pid AS parent_pid,
                     process_exit_code AS exit_code
                FROM cges_events FINAL WHERE agent_id = {id:String}`,
      query_params: { id: agentId },
      format: "JSONEachRow",
    });
    return await rs.json<Row>();
  } finally {
    await ch.close();
  }
}

async function post(identity: AgentIdentity, events: Element[]) {
  return postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { events }),
  });
}

const TWO_63 = "9223372036854775808";

const MALFORMED: [string, (agentId: string) => Element][] = [
  ["a 1007 time of 0x10", (a) => launch(a, { time: "0x10" })],
  ["a 1007 time of ' 1'", (a) => launch(a, { time: " 1" })],
  ["a 1007 time of -1", (a) => launch(a, { time: "-1" })],
  ["a 1007 time of 0", (a) => launch(a, { time: "0" })],
  ["a 1007 time of 0123", (a) => launch(a, { time: "0123" })],
  ["a 1007 time of 2^63", (a) => launch(a, { time: TWO_63 })],
  ["a 1007 time of 20 digits", (a) => launch(a, { time: "1".repeat(20) })],
  ["a 4001 time of 1.5e18", () => connection("1.5e18")],
  ["a 4001 time of 2^63", () => connection(TWO_63)],
  ["a 3002 time that is empty", () => logon("")],
  ["a 3002 time of 2^63", () => logon(TWO_63)],
  ["a 1007 created_time of x", (a) => launch(a, { process: { created_time: "x" } })],
  ["a 1007 created_time of 2^63", (a) => launch(a, { process: { created_time: TWO_63 } })],
  ["a 1007 event_id that is not a UUID", (a) => ({ ...launch(a), event_id: "not-a-uuid" })],
  ["a 1007 pid of 4294967296", (a) => launch(a, { process: { pid: 4_294_967_296 } })],
  ["a 1007 parent_pid of 4294967296", (a) => launch(a, { process: { parent_pid: 4_294_967_296 } })],
  ["a 1007 exit_code of 2147483648", (a) => terminate(a, 2_147_483_648)],
  ["a 1007 exit_code of -2147483649", (a) => terminate(a, -2_147_483_649)],
];

describe("capture_ac_014 — the shape", () => {
  test.each(MALFORMED)("a POST with %s is refused whole", async (_name, malformed) => {
    const identity = await enroll(server.enrollUrl, await issueToken(config));
    const result = await post(identity, [launch(identity.agentId), malformed(identity.agentId)]);
    expect(result.status, result.bodyText).toBe(400);
    expect(JSON.parse(result.bodyText)).toEqual({ error: "invalid_request" });
    expect(await rowsOf(identity.agentId)).toEqual([]);
    expect(await getHeartbeats(config, identity.agentId)).toEqual([]);
  });
});

describe("capture_ac_015 — the edges pass", () => {
  test("the largest time, created_time and pid, and the smallest exit_code, are stored as sent", async () => {
    const identity = await enroll(server.enrollUrl, await issueToken(config));
    const last = "9223372036854775807";
    const edge = launch(identity.agentId, {
      time: last,
      process: { created_time: last, pid: 4_294_967_295, parent_pid: 4_294_967_295 },
    });
    const exited = terminate(identity.agentId, -2_147_483_648);
    const exitedHigh = terminate(identity.agentId, 2_147_483_647);
    const result = await post(identity, [edge, exited, exitedHigh]);
    expect(result.status, result.bodyText).toBe(200);
    const rows = await rowsOf(identity.agentId);
    expect(rows.find((r) => r.event_id === edge.event_id)).toMatchObject({
      time_nanos: last,
      created_nanos: last,
      pid: 4_294_967_295,
      parent_pid: 4_294_967_295,
    });
    expect(rows.find((r) => r.event_id === exited.event_id)?.exit_code).toBe(-2_147_483_648);
    expect(rows.find((r) => r.event_id === exitedHigh.event_id)?.exit_code).toBe(2_147_483_647);
  });
});
