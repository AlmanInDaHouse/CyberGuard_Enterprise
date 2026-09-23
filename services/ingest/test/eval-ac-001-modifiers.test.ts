import { expect, test } from "vitest";
import { evaluateRule, parseRule } from "../src/detect/engine.js";
import { evt, rawRule } from "./helpers/eval.js";

// SPEC-015 eval_ac_001 — every modifier matches and misses, case-insensitively,
// over Image and ParentImage; a selection with a single field is valid.

function ruleWith(field: string, values: string[]) {
  return parseRule(rawRule({ selection: { [field]: values }, condition: "selection" }));
}

test("exact (no modifier): full-string equality, case-insensitive", () => {
  const rule = ruleWith("Image", ["c:\\tools\\rogue.exe"]);
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\Tools\\Rogue.EXE" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\Tools\\other.exe" }))).toBeNull();
});

test("endswith over Image", () => {
  const rule = ruleWith("Image|endswith", ["\\powershell.exe"]);
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\POWERSHELL.EXE" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\notepad.exe" }))).toBeNull();
});

test("startswith over Image", () => {
  const rule = ruleWith("Image|startswith", ["c:\\windows\\temp\\"]);
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\Windows\\Temp\\a.exe" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\Program Files\\a.exe" }))).toBeNull();
});

test("contains over Image", () => {
  const rule = ruleWith("Image|contains", ["\\appdata\\"]);
  const hit = "C:\\Users\\x\\AppData\\Local\\a.exe";
  expect(evaluateRule(rule, evt({ imageFileName: hit }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\Windows\\a.exe" }))).toBeNull();
});

test("modifiers apply to ParentImage too (endswith)", () => {
  const rule = ruleWith("ParentImage|endswith", ["\\winword.exe"]);
  expect(evaluateRule(rule, evt({ parentImage: "C:\\O\\WINWORD.EXE" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ parentImage: "C:\\O\\excel.exe" }))).toBeNull();
});

test("a value list is OR", () => {
  const rule = ruleWith("Image|endswith", ["\\cmd.exe", "\\powershell.exe"]);
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\cmd.exe" }))).not.toBeNull();
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\pwsh.exe" }))).toBeNull();
});

test("a single-field selection is valid", () => {
  const rule = ruleWith("Image|endswith", ["\\powershell.exe"]);
  expect(rule.blocks[0]?.fields).toHaveLength(1);
  expect(evaluateRule(rule, evt({ imageFileName: "C:\\X\\powershell.exe" }))).not.toBeNull();
});
