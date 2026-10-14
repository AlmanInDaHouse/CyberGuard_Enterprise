import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { type UnlinkedStep, chainSteps, recomputeHead } from "../src/soar/chain.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";
import {
  type SoarHarness,
  getExecutions,
  seedOfficeSpawn,
  shippedPlaybooks,
  soarHarness,
} from "./helpers/soar.js";

// soar_ac_005 — SPEC-022 §Data contracts §3: the steps of a record are chained in
// SPEC-012's construction. The known answers below were computed once with the
// api's services/api/src/forensic/hashchain.ts (computeChainHex), which applies
// that construction, over the same objects: ingest's repeated code must agree.

const STEPS: UnlinkedStep[] = [
  {
    step_id: "isolate",
    action: "agent.isolate",
    mode: "dry_run",
    outcome: "would_do",
    detail: "would isolate agent 00000000-0000-4000-8000-000000000001",
    started_at: "2026-10-14T00:00:00.000Z",
    finished_at: "2026-10-14T00:00:00.001Z",
  },
  {
    step_id: "notify",
    action: "notify.email",
    mode: "live",
    outcome: "done",
    detail: "sent",
    started_at: "2026-10-14T00:00:00.001Z",
    finished_at: "2026-10-14T00:00:00.250Z",
  },
];

test("fixed steps give the known heads, link by link", () => {
  expect(chainSteps([]).head).toBe(
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
  const one = chainSteps(STEPS.slice(0, 1));
  expect(one.head).toBe("30ef02bf3aa53b43bc203bb24cf896222cc4bfe9505c5a49341973c040921f50");
  const two = chainSteps(STEPS);
  expect(two.steps.map((s) => s.link)).toEqual([
    "30ef02bf3aa53b43bc203bb24cf896222cc4bfe9505c5a49341973c040921f50",
    "c08b894d85dbde9a181bd206ffe2790b07ffa9c6cf2ab2b15b6b4e2e9f45642f",
  ]);
  expect(two.head).toBe("c08b894d85dbde9a181bd206ffe2790b07ffa9c6cf2ab2b15b6b4e2e9f45642f");
});

test("changing one step's detail changes the head", () => {
  const edited = STEPS.map((s, i) => (i === 0 ? { ...s, detail: `${s.detail} (edited)` } : s));
  expect(chainSteps(edited).head).not.toBe(chainSteps(STEPS).head);
});

let config: Config;
let harness: SoarHarness;
beforeAll(() => {
  config = inject("ingestConfig");
  harness = soarHarness(config, shippedPlaybooks());
});
afterAll(async () => {
  await harness?.close();
});

test("a stored record's chain recomputes to its chain_head", async () => {
  const org = "soar-ac-005-stored";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  await runDetectionCycle(detectConfig(config, org), spyNotify().notify, harness.soar);

  const [incident] = await getIncidents(config, { agentId: agent });
  const [execution] = await getExecutions(config, incident?.incident_id ?? "");
  if (!execution) throw new Error("no execution");
  expect(recomputeHead(execution.steps)).toBe(execution.chain_head);

  const tampered = execution.steps.map((s, i) => (i === 1 ? { ...s, detail: "not sent" } : s));
  expect(recomputeHead(tampered)).not.toBe(execution.chain_head);
});
