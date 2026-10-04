import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import { HEARTBEAT_BODY_LIMIT_BYTES } from "../src/app.js";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// SPEC-017 capture_ac_011 — the server side of capture delivery. A signed
// POST whose body carries events persists them to cges_events; the same
// events posted again (a retry: fresh nonce, same events) leave the FINAL
// count unchanged; a batch of 1024 events with 1 KiB paths is accepted
// (the heartbeat listener's 4 MiB body limit, §Operational §7).

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

/** 2026-10-04T00:00:00Z in Unix nanoseconds, string-encoded. */
const BASE_NANOS = 1_791_072_000_000_000_000n;

/** A Launch as the agent renders it (SPEC-017 §Data contracts). */
function launchEvent(agentId: string, pid: number, imageFileName: string): Record<string, unknown> {
  const nanos = (BASE_NANOS + BigInt(pid) * 1_000n).toString();
  return {
    event_id: globalThis.crypto.randomUUID(),
    class_uid: 1007,
    activity_id: 1,
    time: nanos,
    process: {
      pid,
      uid: `${agentId}:${pid}:${nanos}`,
      name: imageFileName.split("\\").pop() ?? "probe.exe",
      created_time: nanos,
      parent_pid: 4,
      command_line: "",
      subject_user_sid: "",
      image_file_name: imageFileName,
    },
  };
}

async function countEvents(agentId: string, final: boolean): Promise<number> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: `SELECT count() AS n FROM cges_events ${final ? "FINAL" : ""} WHERE agent_id = {id:String}`,
      query_params: { id: agentId },
      format: "JSONEachRow",
    });
    const rows = await rs.json<{ n: string }>();
    return Number(rows[0]?.n ?? 0);
  } finally {
    await ch.close();
  }
}

test("capture_ac_011: events persist, and a resent batch leaves the FINAL count unchanged", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  const events = [1, 2, 3].map((pid) =>
    launchEvent(identity.agentId, pid, `C:\\Windows\\System32\\probe${pid}.exe`),
  );

  const first = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { sequenceNumber: 1, events }),
  });
  expect(first.status).toBe(200);
  expect(await countEvents(identity.agentId, true)).toBe(3);

  // The retry: same sequence_number and events, a fresh nonce and signature.
  const retry = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { sequenceNumber: 1, events }),
  });
  expect(retry.status).toBe(200);
  expect(await countEvents(identity.agentId, true), "FINAL collapses the resent events").toBe(3);
});

test("capture_ac_011: a 1024-event batch with 1 KiB paths is accepted", async () => {
  const identity = await enroll(server.enrollUrl, await issueToken(config));
  const longDir = "a".repeat(1024);
  const events = Array.from({ length: 1024 }, (_, i) =>
    launchEvent(identity.agentId, i + 1, `C:\\${longDir}\\probe${i + 1}.exe`),
  );
  const envelope = await buildSignedEnvelope(identity, { sequenceNumber: 1, events });
  const bodyBytes = Buffer.byteLength(JSON.stringify(envelope));
  // Over Fastify's 1 MiB default, under the listener's limit.
  expect(bodyBytes).toBeGreaterThan(1024 * 1024);
  expect(bodyBytes).toBeLessThan(HEARTBEAT_BODY_LIMIT_BYTES);

  const result = await postHeartbeat(server.heartbeatUrl, {
    caCertPem: server.caCertPem,
    identity,
    envelope,
  });
  expect(result.status, result.bodyText).toBe(200);
  expect(await countEvents(identity.agentId, true)).toBe(1024);
});
