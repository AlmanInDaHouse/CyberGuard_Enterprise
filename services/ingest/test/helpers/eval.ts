import type { NormalizedProcessEvent } from "../../src/detect/types.js";

// SPEC-015 eval_ac_* helpers: a normalized process-creation event and a raw
// (unparsed) rule document, both with sensible defaults a test overrides.

export const IMAGE_PS = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
export const IMAGE_WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";

/** A normalized process-creation event: winword -> powershell by default. */
export function evt(over: Partial<NormalizedProcessEvent> = {}): NormalizedProcessEvent {
  return {
    eventId: "01934abc-def0-4000-89ab-000000000001",
    agentId: "01934abc-def0-7000-89ab-000000000001",
    activityId: 1,
    pid: 4099,
    uid: "uid",
    processName: "powershell.exe",
    imageFileName: IMAGE_PS,
    parentPid: 4012,
    parentImage: IMAGE_WINWORD,
    time: "2026-05-31 16:00:01.000000000",
    ...over,
  };
}

/**
 * A raw (unparsed) rule document with a valid `cg:` block. Pass the `detection`
 * object; `over` replaces top-level keys (e.g. a different `logsource`).
 */
export function rawRule(detection: unknown, over: Record<string, unknown> = {}): unknown {
  return {
    id: "rule.test",
    title: "Test rule",
    level: "high",
    logsource: { product: "windows", category: "process_creation" },
    detection,
    cg: {
      heuristic_score: 0.9,
      severity_id: 4,
      cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
    },
    ...over,
  };
}
