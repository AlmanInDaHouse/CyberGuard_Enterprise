import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { parse as parseYaml } from "yaml";
import { z } from "zod";
import type { NormalizedProcessEvent, RuleMatch, SigmaRule } from "./types.js";

// SPEC-006 5c — the rule evaluator. Loads Sigma rules (rules/windows/*.yml) and
// evaluates normalized events against them. The MVP supports ONLY a single
// `selection` with `|endswith` over `Image` and `ParentImage`, `condition:
// selection`, `logsource.category: process_creation` (ADR-0012 §1; the general
// Sigma-to-Go engine is deferred). Anything outside that subset is REJECTED at
// load time (UnsupportedRuleError) — a security evaluator that silently accepts
// a rule it cannot evaluate is worse than one that does not detect.
//
// Frontier: this is 5c. evaluateRule returns a RuleMatch (the rule's
// contribution); it does NOT score (5d) or assemble/persist the alert (5e).

/** Thrown when a rule uses a Sigma construct outside the MVP evaluator's subset. */
export class UnsupportedRuleError extends Error {
  constructor(detail: string) {
    super(
      `SPEC-006 unsupported Sigma construct (the MVP evaluator handles only \`|endswith\` over Image/ParentImage with \`condition: selection\` and \`logsource.category: process_creation\`): ${detail}`,
    );
    this.name = "UnsupportedRuleError";
  }
}

// zod schema for the supported subset. `.strict()` on `selection` and
// `detection` is the load-bearing defense: any extra key (an unsupported
// modifier like `|contains`, a bare field, a different field such as
// `CommandLine|endswith`, or a `filter` block) is rejected, not ignored.
const selectionSchema = z
  .object({
    "ParentImage|endswith": z.array(z.string().min(1)).nonempty(),
    "Image|endswith": z.array(z.string().min(1)).nonempty(),
  })
  .strict();

const detectionSchema = z
  .object({
    selection: selectionSchema,
    condition: z.literal("selection"),
  })
  .strict();

const ruleSchema = z
  .object({
    id: z.string().min(1),
    title: z.string().min(1),
    level: z.string().min(1),
    logsource: z.object({ category: z.literal("process_creation") }).passthrough(),
    detection: detectionSchema,
    cg: z.object({
      heuristic_score: z.number().min(0).max(1),
      severity_id: z.number().int().min(0).max(6),
      cg_mitre: z.object({
        tactics: z.array(z.string().min(1)).nonempty(),
        techniques: z.array(z.string().min(1)).nonempty(),
      }),
    }),
  })
  // Top-level metadata (status, description, references, tags, …) is ignored.
  .passthrough();

/** Validate and narrow a raw parsed rule into a SigmaRule, or throw UnsupportedRuleError. */
export function parseRule(raw: unknown): SigmaRule {
  const result = ruleSchema.safeParse(raw);
  if (!result.success) {
    const detail = result.error.issues
      .map((i) => `${i.path.join(".") || "(root)"}: ${i.message}`)
      .join("; ");
    throw new UnsupportedRuleError(detail);
  }
  const r = result.data;
  return {
    id: r.id,
    title: r.title,
    level: r.level,
    parentImageEndsWith: r.detection.selection["ParentImage|endswith"],
    imageEndsWith: r.detection.selection["Image|endswith"],
    heuristicScore: r.cg.heuristic_score,
    severityId: r.cg.severity_id,
    cgMitre: { tactics: r.cg.cg_mitre.tactics, techniques: r.cg.cg_mitre.techniques },
  };
}

/** Load + validate every `*.yml` / `*.yaml` rule in `rulesDir`. */
export function loadRules(rulesDir: string): SigmaRule[] {
  return readdirSync(rulesDir)
    .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
    .map((f) => parseRule(parseYaml(readFileSync(join(rulesDir, f), "utf-8"))));
}

/** Case-insensitive (Sigma default + Windows-name semantics) endsWith against any pattern. */
function endsWithAny(value: string, patterns: string[]): boolean {
  const v = value.toLowerCase();
  return patterns.some((p) => v.endsWith(p.toLowerCase()));
}

/**
 * Evaluate one event against one rule. Returns a RuleMatch on match, else null.
 *
 * - Explicit `activityId === 1` guard: this is a process_creation rule. The
 *   guard documents the intent rather than relying on the side effect that
 *   Terminate (activity_id=2) events carry no parent_pid.
 * - parentImage === null → no match (the parent Launch was not captured / is
 *   outside the correlation window; the documented v0.1 false-negative). Treated
 *   as a clean no-match, never an error.
 * - Match = ParentImage endsWith any office image AND Image endsWith any
 *   script-host image, case-insensitively.
 */
export function evaluateRule(rule: SigmaRule, event: NormalizedProcessEvent): RuleMatch | null {
  if (event.activityId !== 1) return null;
  if (event.parentImage === null) return null;
  const matches =
    endsWithAny(event.parentImage, rule.parentImageEndsWith) &&
    endsWithAny(event.imageFileName, rule.imageEndsWith);
  if (!matches) return null;
  return {
    ruleId: rule.id,
    heuristicScore: rule.heuristicScore,
    severityId: rule.severityId,
    cgMitre: rule.cgMitre,
    sourceEvent: event,
  };
}
