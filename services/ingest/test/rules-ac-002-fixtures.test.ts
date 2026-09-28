import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import { z } from "zod";
import { evaluateRule, loadRules } from "../src/detect/engine.js";
import type { NormalizedProcessEvent, SigmaRule } from "../src/detect/types.js";
import { RULES_WINDOWS_DIR } from "./helpers/detect.js";
import { evt } from "./helpers/eval.js";

// SPEC-016 rules_ac_002 — fixtures wired (§Data contracts §3): every rule has a
// rules/tests/<name>.test.json that meets the minimums, every case evaluates as
// expected, and every fixture has its rule. No rule sits in a subdirectory of
// rules/windows/: the loader does not recurse, so it would be ignored in silence.
// (The near-miss requirement of §3 is an authoring rule, pinned by review.)

const RULES_TESTS_DIR = join(RULES_WINDOWS_DIR, "..", "tests");

/** A path in the device form the agent may emit (SPEC-016 §Context, fact 3). */
const DEVICE_FORM = /^\\Device\\HarddiskVolume\d+\\/i;

const fixtureSchema = z
  .object({
    rule: z.string().min(1),
    description: z.string().min(1),
    cases: z
      .array(
        z
          .object({
            name: z.string().min(1),
            event: z
              .object({ Image: z.string().min(1), ParentImage: z.string().min(1).nullable() })
              .strict(),
            expected_match: z.boolean(),
          })
          .strict(),
      )
      .nonempty(),
  })
  .strict();

type Fixture = z.infer<typeof fixtureSchema>;

const rules = loadRules(RULES_WINDOWS_DIR);
const fixtureFiles = readdirSync(RULES_TESTS_DIR).filter((f) => f.endsWith(".test.json"));

const ruleName = (rule: SigmaRule): string => rule.id.replace(/^rule\./, "");

function readFixture(name: string): Fixture {
  const raw: unknown = JSON.parse(
    readFileSync(join(RULES_TESTS_DIR, `${name}.test.json`), "utf-8"),
  );
  return fixtureSchema.parse(raw);
}

/** A fixture case as the Launch of a process with those two fields. */
function caseEvent(event: Fixture["cases"][number]["event"]): NormalizedProcessEvent {
  return evt({ activityId: 1, imageFileName: event.Image, parentImage: event.ParentImage });
}

const readsParent = (rule: SigmaRule): boolean =>
  rule.blocks.some((b) => b.fields.some((f) => f.field === "ParentImage"));

test("every rule has a fixture, and every fixture has its rule", () => {
  expect(rules.length).toBeGreaterThan(0);
  const fixtureNames = fixtureFiles.map((f) => f.replace(/\.test\.json$/, "")).sort();
  expect(fixtureNames).toEqual(rules.map(ruleName).sort());
  for (const name of fixtureNames) {
    expect(readFixture(name).rule, `${name}.test.json`).toBe(`rule.${name}`);
  }
});

test.each(rules.map((rule) => [rule.id, rule] as const))(
  "%s: the fixture meets the minimums and every case evaluates as expected",
  (_id, rule) => {
    const fixture = readFixture(ruleName(rule));
    const positives = fixture.cases.filter((c) => c.expected_match);
    const negatives = fixture.cases.filter((c) => !c.expected_match);

    expect(positives.length, "positive cases").toBeGreaterThanOrEqual(2);
    expect(negatives.length, "negative cases").toBeGreaterThanOrEqual(2);
    const deviceForm = positives.filter(
      (c) =>
        DEVICE_FORM.test(c.event.Image) &&
        (c.event.ParentImage === null || DEVICE_FORM.test(c.event.ParentImage)),
    );
    expect(deviceForm.length, "positive cases with every path in the device form").toBeGreaterThan(
      0,
    );
    if (readsParent(rule)) {
      const unknownParent = negatives.filter((c) => c.event.ParentImage === null);
      expect(unknownParent.length, "negative case with ParentImage null").toBeGreaterThan(0);
    }

    for (const c of fixture.cases) {
      const matched = evaluateRule(rule, caseEvent(c.event)) !== null;
      expect(matched, c.name).toBe(c.expected_match);
    }
  },
);

test("no rule file sits in a subdirectory of rules/windows/", () => {
  const nested: string[] = [];
  const walk = (dir: string, depth: number): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(path, depth + 1);
      } else if (depth > 0 && /\.ya?ml$/.test(entry.name)) {
        nested.push(path);
      }
    }
  };
  walk(RULES_WINDOWS_DIR, 0);
  expect(nested).toEqual([]);
});
