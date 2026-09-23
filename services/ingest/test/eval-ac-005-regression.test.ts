import { expect, test } from "vitest";
import { evaluateRule, loadRules } from "../src/detect/engine.js";
import { RULES_WINDOWS_DIR } from "./helpers/detect.js";
import { evt } from "./helpers/eval.js";

// SPEC-015 eval_ac_005 — regression: the on-disk office rule, loaded through the
// generalized evaluator, behaves exactly as under SPEC-006. (The SPEC-006
// detect_ac_001..006 suites assert the same rule end-to-end unchanged;
// detect_ac_001 is the elevated marquee, run by Manuel.)

const rule = loadRules(RULES_WINDOWS_DIR).find((r) => r.id === "rule.office_spawns_script_host");
if (!rule) {
  throw new Error("eval_ac_005 setup: rule.office_spawns_script_host not found in rules/windows");
}

test("winword -> powershell matches", () => {
  const m = evaluateRule(rule, evt({}));
  expect(m).not.toBeNull();
  expect(m?.ruleId).toBe("rule.office_spawns_script_host");
  expect(m?.heuristicScore).toBe(0.9);
  expect(m?.severityId).toBe(4);
});

test("explorer -> powershell does not match (parent not an Office app)", () => {
  expect(evaluateRule(rule, evt({ parentImage: "C:\\Windows\\explorer.exe" }))).toBeNull();
});

test("null parent does not match", () => {
  expect(evaluateRule(rule, evt({ parentImage: null }))).toBeNull();
});

test("case-insensitive: uppercase WINWORD.EXE / POWERSHELL.EXE still match", () => {
  const m = evaluateRule(
    rule,
    evt({
      parentImage: "C:\\PROGRA~1\\MICROS~1\\Office16\\WINWORD.EXE",
      imageFileName: "C:\\WINDOWS\\SYSTEM32\\WINDOWSPOWERSHELL\\V1.0\\POWERSHELL.EXE",
    }),
  );
  expect(m).not.toBeNull();
});
