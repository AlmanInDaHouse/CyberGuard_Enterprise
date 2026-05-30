import { expect, test } from "vitest";
import { scoreAlert } from "../src/detect/scorer.js";

// SPEC-006 detect_ac_006 — renormalized score (ADR-0012 §4 / SPEC-006 §Operational §3).
// Rule-only ⇒ final_score = heuristic_score (denominator = w_rule = 0.6), NOT
// 0.6·heuristic = 0.54. Pure unit: no DB, no testcontainers. CI-able.
//
// Harness-first RED: scoreAlert is a NotImplemented stub, so these GREEN-shaped
// assertions fail by NotImplementedError until the Phase-5 scorer (5d) lands.

test("detect_ac_006: rule-only score renormalizes to heuristic_score (0.9 -> 0.9, not 0.54)", () => {
  expect(scoreAlert({ heuristicScore: 0.9 })).toBe(0.9);
});

test("detect_ac_006: rule-only score identity holds at 0.5 (-> 0.5)", () => {
  expect(scoreAlert({ heuristicScore: 0.5 })).toBe(0.5);
});
