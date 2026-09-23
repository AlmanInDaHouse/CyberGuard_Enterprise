import { expect, test } from "vitest";
import { evaluateRule, parseRule } from "../src/detect/engine.js";
import { evt, rawRule } from "./helpers/eval.js";

// SPEC-015 eval_ac_002 — condition: and / or / not / parentheses, including
// `selection and not filter`.

test("`selection and not filter`: fires unless the filter matches", () => {
  const rule = parseRule(
    rawRule({
      selection: { "Image|endswith": ["\\powershell.exe"] },
      filter: { "ParentImage|endswith": ["\\explorer.exe"] },
      condition: "selection and not filter",
    }),
  );
  // powershell child, non-explorer parent -> fires.
  expect(evaluateRule(rule, evt({ parentImage: "C:\\O\\winword.exe" }))).not.toBeNull();
  // powershell child, explorer parent -> suppressed by the filter.
  expect(evaluateRule(rule, evt({ parentImage: "C:\\Windows\\explorer.exe" }))).toBeNull();
});

test("`a or b`: either block matching fires", () => {
  const rule = parseRule(
    rawRule({
      a: { "Image|endswith": ["\\cmd.exe"] },
      b: { "Image|endswith": ["\\powershell.exe"] },
      condition: "a or b",
    }),
  );
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\cmd.exe" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\powershell.exe" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\notepad.exe" }))).toBeNull();
});

test("`a and b`: both blocks must match", () => {
  const rule = parseRule(
    rawRule({
      a: { "Image|endswith": ["\\powershell.exe"] },
      b: { "ParentImage|endswith": ["\\winword.exe"] },
      condition: "a and b",
    }),
  );
  expect(evaluateRule(rule, evt({}))).not.toBeNull(); // ps child, winword parent
  expect(evaluateRule(rule, evt({ parentImage: "C:\\Windows\\explorer.exe" }))).toBeNull();
});

test("parentheses regroup: `(a or b) and c`", () => {
  const rule = parseRule(
    rawRule({
      a: { "Image|endswith": ["\\cmd.exe"] },
      b: { "Image|endswith": ["\\powershell.exe"] },
      c: { "ParentImage|endswith": ["\\winword.exe"] },
      condition: "(a or b) and c",
    }),
  );
  // cmd child + winword parent -> (a) and c -> fires.
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\cmd.exe" }))).not.toBeNull();
  // cmd child but explorer parent -> c false -> no fire.
  const noParent = evt({ imageFileName: "C:\\X\\cmd.exe", parentImage: "C:\\W\\explorer.exe" });
  expect(evaluateRule(rule, noParent)).toBeNull();
});
