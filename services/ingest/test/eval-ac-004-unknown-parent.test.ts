import { expect, test } from "vitest";
import { evaluateRule, parseRule } from "../src/detect/engine.js";
import { evt, rawRule } from "./helpers/eval.js";

// SPEC-015 eval_ac_004 — unknown parent: a ParentImage matcher is false when the
// parent is unknown (null), and `selection and not filter_parent` FIRES when the
// parent is unknown (a security evaluator prefers a false positive to a false
// negative; SPEC-015 §Data contracts).

test("a ParentImage matcher is false when the parent is unknown (null)", () => {
  const rule = parseRule(
    rawRule({ selection: { "ParentImage|endswith": ["\\winword.exe"] }, condition: "selection" }),
  );
  expect(evaluateRule(rule, evt({ parentImage: null }))).toBeNull();
});

test("`selection and not filter_parent` fires when the parent is unknown", () => {
  const rule = parseRule(
    rawRule({
      selection: { "Image|endswith": ["\\powershell.exe"] },
      filter_parent: { "ParentImage|endswith": ["\\trusted.exe"] },
      condition: "selection and not filter_parent",
    }),
  );
  // Parent unknown -> filter_parent false -> `not filter_parent` true -> fires.
  expect(evaluateRule(rule, evt({ parentImage: null }))).not.toBeNull();
  // Parent is the trusted one -> the filter matches -> suppressed.
  expect(evaluateRule(rule, evt({ parentImage: "C:\\T\\trusted.exe" }))).toBeNull();
});
