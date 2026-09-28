import { mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "vitest";
import { stringify } from "yaml";
import { loadRules, parseRule } from "../src/detect/engine.js";
import { UnsupportedRuleError } from "../src/detect/errors.js";
import { RULES_WINDOWS_DIR } from "./helpers/detect.js";
import { rawRule } from "./helpers/eval.js";

// SPEC-016 rules_ac_001 — the loader contract (§Data contracts §1): each
// rejection happens at load and names the construct in the error's detail; the
// repo's rules load.

const SELECTION = { selection: { "Image|endswith": ["\\a.exe"] }, condition: "selection" };
const CG = {
  heuristic_score: 0.9,
  severity_id: 4,
  cg_detection_source: "rule",
  cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
};

/** A raw rule whose `cg` block is CG with `over` applied (an `undefined` value drops the key). */
function withCg(over: Record<string, unknown>): unknown {
  const cg: Record<string, unknown> = { ...CG, ...over };
  for (const [key, value] of Object.entries(cg)) {
    if (value === undefined) delete cg[key];
  }
  return rawRule(SELECTION, { cg });
}

function withMitre(tactics: string[], techniques: string[]): unknown {
  return withCg({ cg_mitre: { tactics, techniques } });
}

/** The UnsupportedRuleError `fn` throws; fails the test if it throws nothing or something else. */
function rejection(fn: () => unknown): UnsupportedRuleError {
  try {
    fn();
  } catch (err) {
    if (err instanceof UnsupportedRuleError) return err;
    throw err;
  }
  throw new Error("expected an UnsupportedRuleError, but the rule was accepted");
}

const parseDetail = (raw: unknown): string => rejection(() => parseRule(raw)).detail;

/** A fresh rules directory holding `files` (file name → rule document). */
function rulesDir(files: Record<string, unknown>): string {
  const dir = mkdtempSync(join(tmpdir(), "cg-rules-ac-001-"));
  for (const [name, doc] of Object.entries(files)) {
    writeFileSync(join(dir, name), stringify(doc));
  }
  return dir;
}

test("an id that does not match ^rule\\.[a-z0-9_]+$ is rejected", () => {
  for (const id of ["rule.Office", "office_spawns", "rule.a-b", "rule."]) {
    expect(parseDetail(rawRule(SELECTION, { id }))).toMatch(`id "${id}" does not match`);
  }
});

test("in loadRules, an id other than rule.<file name> is rejected", () => {
  const dir = rulesDir({ "foo.yml": rawRule(SELECTION, { id: "rule.bar" }) });
  expect(rejection(() => loadRules(dir)).detail).toMatch(
    'rule file "foo.yml": id "rule.bar" does not match its file name (expected "rule.foo")',
  );
});

test("in loadRules, an id already loaded is rejected", () => {
  const rule = rawRule(SELECTION, { id: "rule.foo" });
  const dir = rulesDir({ "foo.yml": rule, "foo.yaml": rule });
  expect(rejection(() => loadRules(dir)).detail).toMatch('id "rule.foo" is already loaded');
});

test("loadRules names the file of a rejected rule", () => {
  const bad = rawRule(SELECTION, {
    id: "rule.bad",
    logsource: { product: "linux", category: "process_creation" },
  });
  const dir = rulesDir({ "bad.yml": bad });
  expect(rejection(() => loadRules(dir)).detail).toMatch(
    /^rule file "bad\.yml": logsource\.product/,
  );
});

test("a logsource.product other than windows is rejected", () => {
  const linux = rawRule(SELECTION, {
    logsource: { product: "linux", category: "process_creation" },
  });
  expect(parseDetail(linux)).toMatch('logsource.product "linux" is not supported');
  const missing = rawRule(SELECTION, { logsource: { category: "process_creation" } });
  expect(parseDetail(missing)).toMatch("logsource.product missing is not supported");
});

test("a level outside the five Sigma levels is rejected", () => {
  expect(parseDetail(rawRule(SELECTION, { level: "severe" }))).toMatch(
    'level "severe" is not one of',
  );
});

test("a cg.severity_id other than the OCSF value of the level is rejected", () => {
  expect(parseDetail(withCg({ severity_id: 3 }))).toMatch(
    'cg.severity_id 3 does not match level "high" (OCSF severity_id 4)',
  );
  expect(parseDetail(rawRule(SELECTION, { level: "critical" }))).toMatch(
    'cg.severity_id 4 does not match level "critical"',
  );
  // Control: every level with its OCSF value loads.
  const pairs: Array<[string, number]> = [
    ["informational", 1],
    ["low", 2],
    ["medium", 3],
    ["high", 4],
    ["critical", 5],
  ];
  for (const [level, severity_id] of pairs) {
    expect(() =>
      parseRule(rawRule(SELECTION, { level, cg: { ...CG, severity_id } })),
    ).not.toThrow();
  }
});

test("a missing cg.cg_detection_source, or any value but rule, is rejected", () => {
  expect(parseDetail(withCg({ cg_detection_source: undefined }))).toMatch(
    "cg.cg_detection_source missing is not supported",
  );
  for (const source of ["ml", "hybrid", "Rule"]) {
    expect(parseDetail(withCg({ cg_detection_source: source }))).toMatch(
      `cg.cg_detection_source "${source}" is not supported`,
    );
  }
});

test("an unknown key in cg or in cg_mitre is rejected", () => {
  expect(parseDetail(withCg({ detection_source: "rule" }))).toMatch(
    /^cg: Unrecognized key\(s\) in object: 'detection_source'/,
  );
  expect(
    parseDetail(
      withCg({
        cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"], groups: ["G0001"] },
      }),
    ),
  ).toMatch(/^cg\.cg_mitre: Unrecognized key\(s\) in object: 'groups'/);
});

test("a tactic outside the fourteen ATT&CK Enterprise tactics is rejected", () => {
  for (const tactic of ["TA0002", "initial_access", "Execution", "evasion"]) {
    expect(parseDetail(withMitre([tactic], ["T1059.001"]))).toMatch(
      `cg.cg_mitre.tactics "${tactic}" is not a MITRE ATT&CK Enterprise tactic name`,
    );
  }
});

test("a malformed technique id is rejected", () => {
  for (const technique of ["T59", "t1059.001", "T1059.1", "T1059.0011", "TA0002"]) {
    expect(parseDetail(withMitre(["execution"], [technique]))).toMatch(
      `cg.cg_mitre.techniques "${technique}" does not match`,
    );
  }
});

test("a repeated tactic or technique is rejected", () => {
  expect(parseDetail(withMitre(["execution", "execution"], ["T1059.001"]))).toMatch(
    'cg.cg_mitre.tactics "execution" is repeated',
  );
  expect(parseDetail(withMitre(["execution"], ["T1059.001", "T1059.001"]))).toMatch(
    'cg.cg_mitre.techniques "T1059.001" is repeated',
  );
});

test("a value containing an escaped backslash (\\\\) is rejected", () => {
  const detection = { selection: { "Image|contains": ["\\\\temp\\\\"] }, condition: "selection" };
  expect(parseDetail(rawRule(detection))).toMatch(
    /field "Image" in block "selection" value .* contains "\\\\" \(Sigma reads it as one escaped backslash/,
  );
});

test("a condition that is true when every block is false is rejected", () => {
  const filter = { "Image|endswith": ["\\b.exe"] };
  const notFilter = { filter, condition: "not filter" };
  expect(parseDetail(rawRule(notFilter))).toMatch(
    'condition "not filter" is true when no block matches',
  );
  const orNot = { ...SELECTION, filter, condition: "selection or not filter" };
  expect(parseDetail(rawRule(orNot))).toMatch(
    'condition "selection or not filter" is true when no block matches',
  );
  // Control: a condition that needs a block to match loads.
  const andNot = { ...SELECTION, filter, condition: "selection and not filter" };
  expect(() => parseRule(rawRule(andNot))).not.toThrow();
});

test("the repo's rules load, each with the id of its file name", () => {
  const rules = loadRules(RULES_WINDOWS_DIR);
  const files = readdirSync(RULES_WINDOWS_DIR).filter(
    (f) => f.endsWith(".yml") || f.endsWith(".yaml"),
  );
  expect(rules.length).toBeGreaterThan(0);
  expect(rules.map((r) => r.id).sort()).toEqual(
    files.map((f) => `rule.${f.replace(/\.ya?ml$/, "")}`).sort(),
  );
});
