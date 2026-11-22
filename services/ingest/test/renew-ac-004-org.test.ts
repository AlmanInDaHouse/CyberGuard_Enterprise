import { createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { issueToken } from "./helpers/db.js";
import {
  type RenewHarness,
  agentRow,
  processEvent,
  startRenewHarness,
  withPool,
} from "./helpers/renew.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// SPEC-025 renew_ac_004 (the org) [server]. An agent enrolled with a token of org `default`
// has that org; its heartbeat's `cges_events` rows carry its row's org, read from the row:
// a test that sets another org on the row sees that org on the rows (§Operational §2).

const OTHER_ORG = "renew-ac-004-org";

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

async function eventOrg(eventId: string): Promise<string | undefined> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: "SELECT org_id FROM cges_events WHERE event_id = toUUID({id:String})",
      query_params: { id: eventId },
      format: "JSONEachRow",
    });
    return (await rs.json<{ org_id: string }>())[0]?.org_id;
  } finally {
    await ch.close();
  }
}

async function heartbeatWithEvent(
  identity: Awaited<ReturnType<typeof enroll>>,
  sequence: number,
): Promise<string> {
  const eventId = globalThis.crypto.randomUUID();
  const res = await postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, {
      sequenceNumber: sequence,
      events: [processEvent(eventId)],
    }),
  });
  expect(res.status, res.bodyText).toBe(200);
  return eventId;
}

test("renew_ac_004: the agent takes its token's org, and its events carry its row's org", async () => {
  const identity = await enroll(harness.enrollUrl, await issueToken(config));
  expect((await agentRow(config, identity.agentId))?.org_id).toBe("default");

  expect(await eventOrg(await heartbeatWithEvent(identity, 1))).toBe("default");

  await withPool(config, (pool) =>
    pool.query("UPDATE agents SET org_id = $2 WHERE agent_id = $1", [identity.agentId, OTHER_ORG]),
  );
  expect(await eventOrg(await heartbeatWithEvent(identity, 2))).toBe(OTHER_ORG);
});
