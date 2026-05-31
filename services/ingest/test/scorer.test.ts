import { expect, test } from "vitest";
import { scoreAlert } from "../src/detect/scorer.js";

// SPEC-006 5d — scorer unit. Covers the forward-compatibility detect_ac_006 does
// NOT exercise (006 is rule-only). Renormalized weighted-sum (ADR-0012 §4),
// rounded to numeric(4,3) storage precision.

test("rule-only: final = heuristic (the lone weight cancels)", () => {
  expect(scoreAlert({ heuristicScore: 0.9 })).toBe(0.9);
  expect(scoreAlert({ heuristicScore: 0.5 })).toBe(0.5);
});

test("rule+ml: renormalizes over the active weights 0.6 + 0.15 = 0.75", () => {
  // (0.6·0.9 + 0.15·0.5) / (0.6 + 0.15) = 0.615 / 0.75 = 0.82
  expect(scoreAlert({ heuristicScore: 0.9, mlScore: 0.5 })).toBeCloseTo(0.82, 10);
});

test("all three present: denominator 1.0, reduces to ADR-0005's literal sum", () => {
  expect(scoreAlert({ heuristicScore: 1, uebaScore: 1, mlScore: 1 })).toBe(1);
});

test("ml-only: final = ml_score (single active source; the ml weight cancels)", () => {
  // (0.15·0.88) / 0.15 = 0.88 — the ml-ONLY renormalization (mlScore 0.88 -> 0.88).
  // Distinct from the rule+ml case above (0.82); this is the value SPEC-006 §4
  // says fixture 05 should carry (its 0.71 is stale).
  expect(scoreAlert({ mlScore: 0.88 })).toBe(0.88);
});

test("no source present: throws (cg_detection_source >=1-score invariant)", () => {
  expect(() => scoreAlert({})).toThrow();
});
