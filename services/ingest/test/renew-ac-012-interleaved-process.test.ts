import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { issueToken } from "./helpers/db.js";
import { type RenewHarness, agentRow, startRenewHarness } from "./helpers/renew.js";
import {
  type AgentIdentity,
  buildSignedEnvelope,
  enroll,
  postHeartbeat,
} from "./helpers/test-client.js";

// SPEC-025 renew_ac_012 (an interleaved process) [server]; SC-F4-005. Each accepted
// heartbeat computes its process's start as `body.sent_at − body.uptime_seconds`; starts A, B,
// A raise `interleaved_process` once; A, B, B and A, A + 60 s do not (§Operational §6).

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

/** The start of a process A, and of another B, an hour after it, both in the past. */
const A_MS = Date.now() - 3 * 60 * 60 * 1000;
const B_MS = A_MS + 60 * 60 * 1000;

let sequence = 0;

/** A heartbeat whose process started at `startMs`: uptime is the time since then. */
async function heartbeatStartedAt(identity: AgentIdentity, startMs: number): Promise<void> {
  const sentAt = new Date();
  const uptimeSeconds = Math.round((sentAt.getTime() - startMs) / 1000);
  sequence += 1;
  const res = await postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, {
      sequenceNumber: sequence,
      sentAt: sentAt.toISOString(),
      uptimeSeconds,
    }),
  });
  // No request is refused for the signal.
  expect(res.status, res.bodyText).toBe(200);
}

async function freshAgent(): Promise<AgentIdentity> {
  return enroll(harness.enrollUrl, await issueToken(config));
}

function suspicions(agentId: string) {
  return harness.logs.filter(
    (l) => l.msg === "agent duplicate suspected" && l.agent_id === agentId,
  );
}

test("renew_ac_012: starts A, B, A raise interleaved_process, once", async () => {
  const identity = await freshAgent();
  await heartbeatStartedAt(identity, A_MS);
  await heartbeatStartedAt(identity, B_MS);
  expect((await agentRow(config, identity.agentId))?.duplicate_signal).toBeNull();
  await heartbeatStartedAt(identity, A_MS);
  const raised = await agentRow(config, identity.agentId);
  expect(raised?.duplicate_signal).toBe("interleaved_process");
  expect(raised?.duplicate_signal_at).toBeInstanceOf(Date);
  expect(suspicions(identity.agentId)).toHaveLength(1);
  expect(suspicions(identity.agentId)[0]).toMatchObject({ signal: "interleaved_process" });

  // Further alternation changes neither the signal nor its time, and logs nothing more.
  await heartbeatStartedAt(identity, B_MS);
  await heartbeatStartedAt(identity, A_MS);
  const later = await agentRow(config, identity.agentId);
  expect(later?.duplicate_signal).toBe("interleaved_process");
  expect((later?.duplicate_signal_at as Date).getTime()).toBe(
    (raised?.duplicate_signal_at as Date).getTime(),
  );
  expect(suspicions(identity.agentId)).toHaveLength(1);
  // The agent is neither refused nor revoked.
  expect(later?.revoked_at).toBeNull();
});

test("renew_ac_012: starts A, B, B raise nothing: a restart", async () => {
  const identity = await freshAgent();
  await heartbeatStartedAt(identity, A_MS);
  await heartbeatStartedAt(identity, B_MS);
  await heartbeatStartedAt(identity, B_MS);
  const row = await agentRow(config, identity.agentId);
  expect(row?.duplicate_signal).toBeNull();
  // The columns hold the last two distinct starts, B then A.
  expect(Math.abs((row?.process_started_at as Date).getTime() - B_MS)).toBeLessThanOrEqual(2000);
  expect(Math.abs((row?.previous_process_started_at as Date).getTime() - A_MS)).toBeLessThanOrEqual(
    2000,
  );
  expect(suspicions(identity.agentId)).toHaveLength(0);
});

test("renew_ac_012: starts A, A + 60 s raise nothing and move nothing", async () => {
  const identity = await freshAgent();
  await heartbeatStartedAt(identity, A_MS);
  await heartbeatStartedAt(identity, A_MS + 60_000);
  const row = await agentRow(config, identity.agentId);
  expect(row?.duplicate_signal).toBeNull();
  expect(row?.previous_process_started_at).toBeNull();
  expect(Math.abs((row?.process_started_at as Date).getTime() - A_MS)).toBeLessThanOrEqual(2000);
});
