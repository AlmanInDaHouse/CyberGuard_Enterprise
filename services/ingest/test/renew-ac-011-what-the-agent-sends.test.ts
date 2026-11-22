import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { issueToken } from "./helpers/db.js";
import { type RenewHarness, agentRow, startRenewHarness } from "./helpers/renew.js";
import { buildSignedEnvelope, enroll, postHeartbeat } from "./helpers/test-client.js";

// SPEC-025 renew_ac_011 (what the agent sends) [server]. An enrollment stores the host
// name, platform and version it carries; a heartbeat with another host name updates it; a
// host name of 300 characters is stored cut to 255 code points (§Data contracts §1,
// §Operational §2; Decision record 9).

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

test("renew_ac_011: the enrollment's host name, platform and version are kept, and a heartbeat's replace them", async () => {
  const identity = await enroll(harness.enrollUrl, await issueToken(config), {
    hostname: "RENEW-011-A",
    platform: "windows",
    version: "1.2.3",
  });
  expect(await agentRow(config, identity.agentId)).toMatchObject({
    hostname: "RENEW-011-A",
    platform: "windows",
    agent_version: "1.2.3",
  });

  const res = await postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { hostname: "RENEW-011-B" }),
  });
  expect(res.status, res.bodyText).toBe(200);
  // The test client's agent block carries platform "linux" and version "0.1.0".
  expect(await agentRow(config, identity.agentId)).toMatchObject({
    hostname: "RENEW-011-B",
    platform: "linux",
    agent_version: "0.1.0",
  });
});

test("renew_ac_011: a host name of 300 characters is stored cut to 255 code points", async () => {
  const identity = await enroll(harness.enrollUrl, await issueToken(config), {
    hostname: "h".repeat(300),
  });
  expect((await agentRow(config, identity.agentId))?.hostname).toBe("h".repeat(255));

  // Characters outside the BMP: cut by code points, not by UTF-16 units.
  const wide = "\u{1F5A5}".repeat(300);
  const res = await postHeartbeat(harness.heartbeatUrl, {
    caCertPem: harness.caCertPem,
    identity,
    envelope: await buildSignedEnvelope(identity, { hostname: wide }),
  });
  expect(res.status, res.bodyText).toBe(200);
  const stored = (await agentRow(config, identity.agentId))?.hostname as string;
  expect([...stored]).toHaveLength(255);
  expect(stored).toBe("\u{1F5A5}".repeat(255));
});
