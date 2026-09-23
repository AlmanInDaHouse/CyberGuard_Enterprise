import { expect, test } from "vitest";
import { loadRules, parseRule } from "../src/detect/engine.js";
import { RULES_WINDOWS_DIR } from "./helpers/detect.js";
import { rawRule } from "./helpers/eval.js";

// SPEC-015 — engine shape tests: loadRules parses the on-disk office rule into
// the generalized SigmaRule shape, and parseRule normalizes a rule's blocks +
// condition. Behavioural coverage (modifiers, condition, fail-closed rejection,
// regression) lives in the eval_ac_* suites.

const rule = loadRules(RULES_WINDOWS_DIR).find((r) => r.id === "rule.office_spawns_script_host");
if (!rule) {
  throw new Error("engine.test setup: rule.office_spawns_script_host not found in rules/windows");
}

test("loadRules parses office_spawns_script_host.yml into a SigmaRule", () => {
  expect(rule.id).toBe("rule.office_spawns_script_host");
  expect(rule.heuristicScore).toBe(0.9);
  expect(rule.severityId).toBe(4);
  expect(rule.logsourceCategory).toBe("process_creation");
  expect(rule.cgMitre.tactics).toContain("execution");
  expect(rule.cgMitre.techniques).toContain("T1059.001");

  // One "selection" block over Image + ParentImage with the endswith modifier.
  const selection = rule.blocks.find((b) => b.name === "selection");
  expect(selection).toBeDefined();
  const fields = selection?.fields ?? [];
  const image = fields.find((f) => f.field === "Image");
  const parent = fields.find((f) => f.field === "ParentImage");
  expect(image?.modifier).toBe("endswith");
  expect(parent?.modifier).toBe("endswith");
  // Values are lowercased at parse time.
  expect(image?.values).toContain("\\powershell.exe");
  expect(parent?.values).toContain("\\winword.exe");
});

test("parseRule normalizes a valid rule's blocks and condition", () => {
  const parsed = parseRule(
    rawRule({
      selection: { "Image|endswith": ["\\POWERSHELL.EXE"] },
      condition: "selection",
    }),
  );
  expect(parsed.logsourceCategory).toBe("process_creation");
  expect(parsed.blocks).toHaveLength(1);
  expect(parsed.blocks[0]?.name).toBe("selection");
  expect(parsed.blocks[0]?.fields[0]?.field).toBe("Image");
  expect(parsed.blocks[0]?.fields[0]?.modifier).toBe("endswith");
  // Values are lowercased at parse time.
  expect(parsed.blocks[0]?.fields[0]?.values).toEqual(["\\powershell.exe"]);
});
