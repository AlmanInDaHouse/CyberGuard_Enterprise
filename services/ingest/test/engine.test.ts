import { expect, test } from "vitest";
import { UnsupportedRuleError, evaluateRule, loadRules, parseRule } from "../src/detect/engine.js";
import type { NormalizedProcessEvent } from "../src/detect/types.js";
import { RULES_WINDOWS_DIR } from "./helpers/detect.js";

// SPEC-006 5c — rule-evaluator gate. PURE (no DB): 5c's own verifiable GREEN
// while detect_ac_002..006 stay RED (they go through runDetectionCycle /
// scoreAlert, wired in 5d/5e). The loud-rejection block is EXHAUSTIVE, not a
// sample: zod .strict() is the only defense against a rule the evaluator cannot
// understand, and a security evaluator that silently accepts such a rule is
// worse than one that does not detect.

const rule = loadRules(RULES_WINDOWS_DIR).find((r) => r.id === "rule.office_spawns_script_host");
if (!rule) {
  throw new Error("engine.test setup: rule.office_spawns_script_host not found in rules/windows");
}

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const PS = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

function event(over: Partial<NormalizedProcessEvent>): NormalizedProcessEvent {
  return {
    eventId: "01934abc-def0-4000-89ab-000000000001",
    agentId: "01934abc-def0-7000-89ab-000000000001",
    activityId: 1,
    pid: 4099,
    uid: "uid",
    processName: "powershell.exe",
    imageFileName: PS,
    parentPid: 4012,
    parentImage: WINWORD,
    time: "2026-05-31 16:00:01.000000000",
    ...over,
  };
}

test("loadRules parses office_spawns_script_host.yml into a SigmaRule", () => {
  expect(rule.id).toBe("rule.office_spawns_script_host");
  expect(rule.heuristicScore).toBe(0.9);
  expect(rule.severityId).toBe(4);
  expect(rule.imageEndsWith).toContain("\\powershell.exe");
  expect(rule.parentImageEndsWith).toContain("\\winword.exe");
  expect(rule.cgMitre.tactics).toContain("execution");
  expect(rule.cgMitre.techniques).toContain("T1059.001");
});

test("matches Office parent -> script-host child", () => {
  const m = evaluateRule(rule, event({}));
  expect(m).not.toBeNull();
  expect(m?.ruleId).toBe("rule.office_spawns_script_host");
  expect(m?.heuristicScore).toBe(0.9);
  expect(m?.severityId).toBe(4);
  expect(m?.sourceEvent.eventId).toBe("01934abc-def0-4000-89ab-000000000001");
});

test("no match: parent is explorer.exe (not an Office app)", () => {
  expect(evaluateRule(rule, event({ parentImage: "C:\\Windows\\explorer.exe" }))).toBeNull();
});

test("no match: parentImage is null (parent not captured)", () => {
  expect(evaluateRule(rule, event({ parentImage: null }))).toBeNull();
});

test("case-insensitive: uppercase WINWORD.EXE / POWERSHELL.EXE still match", () => {
  const m = evaluateRule(
    rule,
    event({
      parentImage: "C:\\PROGRA~1\\MICROS~1\\Office16\\WINWORD.EXE",
      imageFileName: "C:\\WINDOWS\\SYSTEM32\\WINDOWSPOWERSHELL\\V1.0\\POWERSHELL.EXE",
    }),
  );
  expect(m).not.toBeNull();
});

test("no match: Terminate event (activityId=2) — explicit process-creation guard", () => {
  expect(evaluateRule(rule, event({ activityId: 2 }))).toBeNull();
});

// --- Exhaustive loud-rejection: parseRule -> UnsupportedRuleError ---

function ruleWith(opts: {
  selection?: Record<string, unknown>;
  condition?: unknown;
  detectionExtra?: Record<string, unknown>;
  category?: unknown;
  cg?: unknown;
}): unknown {
  const detection: Record<string, unknown> = {
    selection: opts.selection ?? {
      "ParentImage|endswith": ["\\winword.exe"],
      "Image|endswith": ["\\powershell.exe"],
    },
    condition: opts.condition ?? "selection",
    ...opts.detectionExtra,
  };
  return {
    id: "rule.test",
    title: "Test rule",
    level: "high",
    logsource: { product: "windows", category: opts.category ?? "process_creation" },
    detection,
    cg: opts.cg ?? {
      heuristic_score: 0.9,
      severity_id: 4,
      cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
    },
  };
}

test("parseRule accepts the valid base rule", () => {
  expect(() => parseRule(ruleWith({}))).not.toThrow();
});

test("parseRule rejects every unsupported construct (exhaustive)", () => {
  const rejects = (raw: unknown) => expect(() => parseRule(raw)).toThrow(UnsupportedRuleError);

  // 1. Unsupported modifier on a supported field.
  rejects(
    ruleWith({
      selection: {
        "ParentImage|endswith": ["\\winword.exe"],
        "Image|endswith": ["\\powershell.exe"],
        "Image|contains": ["powershell"],
      },
    }),
  );
  // 2. Bare field, no modifier (exact match — unsupported).
  rejects(ruleWith({ selection: { "ParentImage|endswith": ["\\winword.exe"], Image: ["x"] } }));
  // 3. Field outside Image/ParentImage. CRITICAL: command_line is empty in v0.1;
  //    a rule keyed on it must be REJECTED, not matched against "".
  rejects(
    ruleWith({
      selection: {
        "ParentImage|endswith": ["\\winword.exe"],
        "Image|endswith": ["\\powershell.exe"],
        "CommandLine|endswith": ["-enc"],
      },
    }),
  );
  // 4. condition other than "selection".
  rejects(ruleWith({ condition: "selection and not filter" }));
  // 5. a `filter` block in detection.
  rejects(ruleWith({ detectionExtra: { filter: { "Image|endswith": ["\\benign.exe"] } } }));
  // 6. logsource.category != process_creation.
  rejects(ruleWith({ category: "network_connection" }));
  // 7a. cg invalid (severity outside 0..6).
  rejects(
    ruleWith({
      cg: {
        heuristic_score: 0.9,
        severity_id: 99,
        cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
      },
    }),
  );
  // 7b. cg absent entirely.
  rejects({
    id: "rule.test",
    title: "Test rule",
    level: "high",
    logsource: { product: "windows", category: "process_creation" },
    detection: {
      selection: {
        "ParentImage|endswith": ["\\winword.exe"],
        "Image|endswith": ["\\powershell.exe"],
      },
      condition: "selection",
    },
  });
});
