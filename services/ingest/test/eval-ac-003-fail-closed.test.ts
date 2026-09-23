import { expect, test } from "vitest";
import { parseRule } from "../src/detect/engine.js";
import { UnsupportedRuleError } from "../src/detect/errors.js";
import { rawRule } from "./helpers/eval.js";

// SPEC-015 eval_ac_003 — fail-closed: every out-of-scope construct on the
// SPEC-015 §Scope list is rejected at load, and the error names it.

const rejects = (detection: unknown, over?: Record<string, unknown>) =>
  expect(() => parseRule(rawRule(detection, over))).toThrow(UnsupportedRuleError);

test("CommandLine and User fields are rejected with a B2 pointer", () => {
  expect(() =>
    parseRule(rawRule({ selection: { "CommandLine|contains": ["-enc"] }, condition: "selection" })),
  ).toThrow(/B2/);
  expect(() =>
    parseRule(rawRule({ selection: { "User|endswith": ["x"] }, condition: "selection" })),
  ).toThrow(/B2/);
});

test("any other field is rejected", () => {
  rejects({ selection: { "Hashes|contains": ["abc"] }, condition: "selection" });
  rejects({ selection: { CommandLineFoo: ["x"] }, condition: "selection" });
});

test("a modifier outside the four is rejected", () => {
  expect(() =>
    parseRule(rawRule({ selection: { "Image|re": ["x"] }, condition: "selection" })),
  ).toThrow(/modifier/);
});

test("more than one modifier on a field is rejected", () => {
  rejects({ selection: { "Image|endswith|all": ["x"] }, condition: "selection" });
});

test("an explicit |exact modifier is rejected (exact match is written without a modifier)", () => {
  expect(() =>
    parseRule(rawRule({ selection: { "Image|exact": ["c:\\a.exe"] }, condition: "selection" })),
  ).toThrow(/exact match is written without a modifier/);
});

test("a wildcard value is rejected", () => {
  expect(() =>
    parseRule(rawRule({ selection: { "Image|endswith": ["*.exe"] }, condition: "selection" })),
  ).toThrow(/wildcard/);
  rejects({ selection: { "Image|endswith": ["a?b"] }, condition: "selection" });
});

test("an empty value or an empty list is rejected", () => {
  expect(() =>
    parseRule(rawRule({ selection: { "Image|endswith": [""] }, condition: "selection" })),
  ).toThrow(/empty value/);
  rejects({ selection: { "Image|endswith": [] }, condition: "selection" });
});

test("a non-string value is rejected", () => {
  rejects({ selection: { "Image|endswith": [123] }, condition: "selection" });
});

test("a list-of-maps block is rejected", () => {
  rejects({ selection: [{ "Image|endswith": ["x"] }], condition: "selection" });
});

test("a keyword-only block (string or number, not a map) is rejected", () => {
  rejects({ selection: "powershell", condition: "selection" });
  rejects({ selection: 5, condition: "selection" });
});

test("an empty block (no fields) is rejected", () => {
  rejects({ selection: {}, condition: "selection" });
});

test("condition set-ops `1 of` / `all of` / `them` are rejected", () => {
  rejects({ selection: { "Image|endswith": ["\\a.exe"] }, condition: "1 of them" });
  rejects({ selection: { "Image|endswith": ["\\a.exe"] }, condition: "all of them" });
});

test("an aggregation pipe is rejected", () => {
  rejects({ selection: { "Image|endswith": ["\\a.exe"] }, condition: "selection | count() > 5" });
});

test("a timeframe key is rejected", () => {
  rejects({
    selection: { "Image|endswith": ["\\a.exe"] },
    condition: "selection",
    timeframe: "5m",
  });
});

test("an undefined block referenced by the condition is rejected", () => {
  rejects({ selection: { "Image|endswith": ["\\a.exe"] }, condition: "selection and filter" });
});

test("an unreferenced block is rejected", () => {
  rejects({
    selection: { "Image|endswith": ["\\a.exe"] },
    extra: { "Image|endswith": ["\\b.exe"] },
    condition: "selection",
  });
});

test("another logsource category is rejected", () => {
  expect(() =>
    parseRule(
      rawRule(
        { selection: { "Image|endswith": ["\\a.exe"] }, condition: "selection" },
        { logsource: { product: "windows", category: "network_connection" } },
      ),
    ),
  ).toThrow(/logsource/);
});

test("a missing condition is rejected", () => {
  rejects({ selection: { "Image|endswith": ["\\a.exe"] } });
});
