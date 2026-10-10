import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// SPEC-020 auth_ac_001 — a mixed POST persists. One Process Activity, one Network
// Activity and two Authentication elements (a success with a source, a package and
// an elevated token; a failure with withheld names and both codes) in one signed
// POST: answered 200; each 3002 row carries the columns of §Data contracts; the
// 1007 and 4001 rows carry the eleven columns at their defaults; the four rows
// share one arrived_at.

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

const LOGON_COLUMNS = [
  "user_uid",
  "user_name",
  "user_domain",
  "logon_type_id",
  "status_id",
  "status_code",
  "status_detail",
  "auth_protocol",
  "auth_protocol_id",
  "src_hostname",
  "elevated_token",
] as const;

type StoredRow = Record<string, unknown>;

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
               process_name, src_ip, ${LOGON_COLUMNS.join(", ")},
               toString(toUnixTimestamp64Nano(time)) AS time_nanos,
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

const LOGON_DEFAULTS = {
  user_uid: "",
  user_name: "",
  user_domain: "",
  logon_type_id: 0,
  status_id: 0,
  status_code: "",
  status_detail: "",
  auth_protocol: "",
  auth_protocol_id: 0,
  src_hostname: "",
  elevated_token: null,
};

test("auth_ac_001: a POST with 1007, 4001 and two 3002 elements persists all four", async () => {
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
      name: "net.exe",
      created_time: launchNanos,
      parent_pid: 4,
      command_line: "",
      subject_user_sid: "",
      image_file_name: "C:\\Windows\\System32\\net.exe",
    },
  };
  const connection = {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 4001,
    activity_id: 1,
    time: (BASE_NANOS + 2_000n).toString(),
    src_endpoint: { ip: "127.0.0.1", port: 50000 },
    dst_endpoint: { ip: "127.0.0.1", port: 445 },
    connection_info: { protocol_name: "tcp", direction: "outbound" },
    actor: { process: { pid: 4 } },
  };
  const success = {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 3002,
    category_uid: 3,
    activity_id: 1,
    time: (BASE_NANOS + 3_000_000_001n).toString(),
    status_id: 1,
    user: { uid: "S-1-5-21-1111-2222-3333-1001", name: "auth-ac-001-user", domain: "WS-0042" },
    logon_type_id: 3,
    auth_protocol: "NTLM",
    auth_protocol_id: 1,
    src_endpoint: { ip: "127.0.0.1", hostname: "WS-0042" },
    cg_elevated_token: true,
  };
  const failure = {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 3002,
    category_uid: 3,
    activity_id: 1,
    time: (BASE_NANOS + 4_000_000_002n).toString(),
    status_id: 2,
    user: { uid: "S-1-0-0", name: "<withheld>", domain: "<withheld>" },
    logon_type_id: 3,
    auth_protocol: "NTLM",
    auth_protocol_id: 1,
    status_code: "0xc000006d",
    status_detail: "0xc0000064",
  };

  const result = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, {
      sequenceNumber: 1,
      events: [launch, connection, success, failure],
    }),
  });
  expect(result.status, result.bodyText).toBe(200);

  const rows = await storedRows(agentId);
  expect(rows.size).toBe(4);

  expect(rows.get(success.event_id)).toMatchObject({
    class_uid: 3002,
    activity_id: 1,
    process_pid: 0,
    process_uid: "",
    process_name: "",
    src_ip: "127.0.0.1",
    user_uid: success.user.uid,
    user_name: success.user.name,
    user_domain: success.user.domain,
    logon_type_id: 3,
    status_id: 1,
    status_code: "",
    status_detail: "",
    auth_protocol: "NTLM",
    auth_protocol_id: 1,
    src_hostname: "WS-0042",
    elevated_token: true,
    time_nanos: success.time,
  });
  expect(rows.get(failure.event_id)).toMatchObject({
    class_uid: 3002,
    activity_id: 1,
    src_ip: "",
    user_uid: "S-1-0-0",
    user_name: "<withheld>",
    user_domain: "<withheld>",
    logon_type_id: 3,
    status_id: 2,
    status_code: "0xc000006d",
    status_detail: "0xc0000064",
    auth_protocol: "NTLM",
    auth_protocol_id: 1,
    src_hostname: "",
    elevated_token: null,
    time_nanos: failure.time,
  });
  // The other classes carry the eleven columns at their defaults.
  expect(rows.get(launch.event_id)).toMatchObject({ class_uid: 1007, ...LOGON_DEFAULTS });
  expect(rows.get(connection.event_id)).toMatchObject({ class_uid: 4001, ...LOGON_DEFAULTS });

  // One INSERT per POST: the four rows share one arrived_at (SPEC-018 §Data contracts).
  const arrivals = new Set([...rows.values()].map((r) => r.arrived_at));
  expect(arrivals.size).toBe(1);
});
