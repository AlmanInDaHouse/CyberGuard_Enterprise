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

// SPEC-020 auth_ac_002 — validation. A POST with an invalid Authentication element
// is answered 400 invalid_request as a whole and stores nothing, its valid elements
// included (§Operational §7).

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

type Element = Record<string, unknown>;

function success(): Element {
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 3002,
    category_uid: 3,
    activity_id: 1,
    time: (BASE_NANOS + 1_000n).toString(),
    status_id: 1,
    user: { uid: "S-1-5-21-1111-2222-3333-1001", name: "auth-ac-002-user", domain: "WS-0042" },
    logon_type_id: 2,
    auth_protocol: "Negotiate",
    auth_protocol_id: 99,
    cg_elevated_token: false,
  };
}

function failure(): Element {
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 3002,
    category_uid: 3,
    activity_id: 1,
    time: (BASE_NANOS + 2_000n).toString(),
    status_id: 2,
    user: { uid: "S-1-0-0", name: "<withheld>", domain: "<withheld>" },
    logon_type_id: 3,
    auth_protocol: "NTLM",
    auth_protocol_id: 1,
    src_endpoint: { ip: "192.0.2.10", hostname: "WS-0042" },
    status_code: "0xc000006d",
    status_detail: "0xc0000064",
  };
}

function without(e: Element, key: string): Element {
  const { [key]: _omitted, ...rest } = e;
  return rest;
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

const INVALID: [string, () => Element][] = [
  ["category_uid 1", () => ({ ...success(), category_uid: 1 })],
  ["activity_id 2", () => ({ ...success(), activity_id: 2 })],
  ["status_id 3", () => ({ ...success(), status_id: 3 })],
  ["no user.uid", () => ({ ...success(), user: { name: "x", domain: "y" } })],
  ["an empty user.name", () => ({ ...success(), user: { uid: "S-1-5-7", name: "", domain: "y" } })],
  ["no user.domain", () => ({ ...success(), user: { uid: "S-1-5-7", name: "x" } })],
  ["logon_type_id 100", () => ({ ...success(), logon_type_id: 100 })],
  ["auth_protocol_id 3", () => ({ ...success(), auth_protocol_id: 3 })],
  ["no auth_protocol", () => without(success(), "auth_protocol")],
  [
    "a src_endpoint.ip that is not an address",
    () => ({ ...failure(), src_endpoint: { ip: "WS-0042" } }),
  ],
  ["a success with status_code", () => ({ ...success(), status_code: "0xc000006d" })],
  ["a failure without status_detail", () => without(failure(), "status_detail")],
  ["status_code in upper case", () => ({ ...failure(), status_code: "0xC000006D" })],
  ["a failure with cg_elevated_token", () => ({ ...failure(), cg_elevated_token: true })],
];

test.each(INVALID)("auth_ac_002: a POST with %s is refused whole", async (_name, invalid) => {
  sequence += 1;
  const events = [success(), failure(), invalid()];
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

test("auth_ac_002: the two valid shapes alone are accepted", async () => {
  sequence += 1;
  const result = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, {
      sequenceNumber: sequence,
      events: [success(), failure()],
    }),
  });
  expect(result.status, result.bodyText).toBe(200);
});
