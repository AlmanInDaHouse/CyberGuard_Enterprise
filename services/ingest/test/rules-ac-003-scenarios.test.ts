import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, expect, inject, test } from "vitest";
import { z } from "zod";
import type { Config } from "../src/config.js";
import { loadRules } from "../src/detect/engine.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, getWatermark, insertCgesEvent } from "./helpers/db.js";
import { RULES_WINDOWS_DIR, detectConfig } from "./helpers/detect.js";

// SPEC-016 rules_ac_003 — scenarios wired (§Data contracts §4): every
// harness/scenarios/*/scenario.json runs through one runDetectionCycle against
// the whole of rules/windows/, and coverage holds. The runner derives what a
// scenario leaves out: org_id = the scenario id; one agent per scenario and each
// event_id derived deterministically from the id and the event's position; time =
// a fixed base plus the position in seconds. Alerts are counted per agent, not
// from the cycle's result, so a driver tick that runs the cycle first changes
// nothing.

const SCENARIOS_DIR = join(RULES_WINDOWS_DIR, "..", "..", "harness", "scenarios");

/** The fixed event-time base of every scenario (the position adds seconds). */
const TIME_BASE_MS = Date.parse("2026-09-27T10:00:00Z");

const scenarioSchema = z
  .object({
    id: z.string().regex(/^SC\d{3}$/),
    title: z.string().min(1),
    track: z.enum(["rule", "ml", "hybrid"]),
    expected_detection_source: z.enum(["rule", "ml", "hybrid"]).nullable(),
    rule_id: z.string().min(1),
    description: z.string().min(1),
    input: z
      .object({
        cges_events: z
          .array(
            z
              .object({
                activity_id: z.number().int(),
                process_name: z.string().min(1),
                image_file_name: z.string().min(1),
                process_pid: z.number().int().nonnegative(),
                process_parent_pid: z.number().int().nonnegative().nullable(),
              })
              .strict(),
          )
          .nonempty(),
      })
      .strict(),
    expected: z
      .object({
        alert: z.boolean(),
        alert_count: z.number().int().nonnegative(),
        rule_id: z.string().min(1).optional(),
        detection_source: z.literal("rule").optional(),
        final_score: z.number().optional(),
      })
      .strict(),
  })
  .strict();

type Scenario = z.infer<typeof scenarioSchema>;

const scenarioDirs = readdirSync(SCENARIOS_DIR, { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .map((e) => e.name)
  .sort();

const scenarios: Array<[string, Scenario]> = scenarioDirs.map((dir) => {
  const raw: unknown = JSON.parse(readFileSync(join(SCENARIOS_DIR, dir, "scenario.json"), "utf-8"));
  return [dir, scenarioSchema.parse(raw)];
});

const rules = loadRules(RULES_WINDOWS_DIR);

/** A deterministic RFC 9562 version-8 UUID derived from `name`. */
function derivedUuid(name: string): string {
  const h = createHash("sha256").update(name).digest("hex");
  const variant = ((Number.parseInt(h.charAt(16), 16) & 0x3) | 0x8).toString(16);
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-8${h.slice(13, 16)}-${variant}${h.slice(17, 20)}-${h.slice(20, 32)}`;
}

/** ClickHouse DateTime64(9) literal for the scenario's `position`-th event. */
function eventTime(position: number): string {
  const iso = new Date(TIME_BASE_MS + position * 1000).toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 19)}.000000000`;
}

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

test("every scenario is well-formed, named after its id, and consistent with its expectation", () => {
  expect(scenarios.length).toBeGreaterThan(0);
  for (const [dir, s] of scenarios) {
    expect(dir.startsWith(`${s.id}-`), dir).toBe(true);
    expect(s.track, dir).toBe("rule");
    if (s.expected.alert) {
      expect(s.expected.alert_count, dir).toBeGreaterThan(0);
      expect(s.expected_detection_source, dir).toBe("rule");
      expect(s.expected.rule_id, dir).toBe(s.rule_id);
      expect(s.expected.detection_source, dir).toBe("rule");
      expect(s.expected.final_score, dir).toBeDefined();
    } else {
      expect(s.expected.alert_count, dir).toBe(0);
      expect(s.expected_detection_source, dir).toBeNull();
    }
  }
});

test("coverage: every rule has a positive scenario, and every scenario names an existing rule", () => {
  const ruleIds = new Set(rules.map((r) => r.id));
  for (const [dir, s] of scenarios) {
    expect(ruleIds.has(s.rule_id), `${dir} rule_id ${s.rule_id}`).toBe(true);
  }
  for (const id of ruleIds) {
    const positive = scenarios.some(([, s]) => s.expected.alert && s.expected.rule_id === id);
    expect(positive, `positive scenario for ${id}`).toBe(true);
  }
});

test.each(scenarios)("%s passes against the whole rule set", async (_dir, s) => {
  const orgId = s.id;
  const agentId = derivedUuid(`${s.id}/agent`);
  await enrollTestAgent(config, agentId, orgId);
  for (const [position, e] of s.input.cges_events.entries()) {
    await insertCgesEvent(config, {
      agentId,
      orgId,
      eventId: derivedUuid(`${s.id}/event/${position}`),
      activityId: e.activity_id,
      processPid: e.process_pid,
      processName: e.process_name,
      imageFileName: e.image_file_name,
      processParentPid: e.process_parent_pid,
      time: eventTime(position),
    });
  }

  await runDetectionCycle(detectConfig(config, orgId));

  // The cycle ran over the scenario's org (a zero-alert pass is not vacuous) ...
  expect(await getWatermark(config, orgId)).not.toBeNull();
  // ... and the scenario's agent has exactly the expected alerts.
  const alerts = await getAlerts(config, { agentId });
  expect(alerts).toHaveLength(s.expected.alert_count);
  for (const alert of alerts) {
    expect(alert.rule_id).toBe(s.expected.rule_id);
    expect(alert.cg_detection_source).toBe("rule");
    expect(alert.final_score).toBe(s.expected.final_score);
  }
});
