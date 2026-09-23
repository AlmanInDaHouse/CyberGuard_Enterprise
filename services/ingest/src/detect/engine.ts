import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { parse as parseYaml } from "yaml";
import { z } from "zod";
import { IDENTIFIER_RE, RESERVED_WORDS, evaluateCondition, parseCondition } from "./condition.js";
import { UnsupportedRuleError } from "./errors.js";
import type {
  NormalizedProcessEvent,
  RuleMatch,
  SigmaBlock,
  SigmaFieldMatcher,
  SigmaModifier,
  SigmaRule,
} from "./types.js";

// SPEC-015 — the generalized Sigma-subset rule evaluator. Loads rules
// (rules/windows/*.yml), validates each against the SPEC-015 §Scope subset, and
// evaluates normalized events against them. The subset:
//   - fields Image and ParentImage — the populated v0.1 process fields (SPEC-006
//     §Data contracts; ADR-0012:61). CommandLine / User are rejected (empty in
//     v0.1, deferred to B2).
//   - modifiers exact / endswith / startswith / contains, all case-insensitive.
//   - one or more named detection blocks (a block is the AND of its fields; a
//     field is the OR of its values) combined by a boolean `condition` over
//     and / or / not / parentheses (parsed by ./condition.js).
//   - logsource.category resolved against a dispatch table; only
//     `process_creation` is implemented (roadmap §D adds the others).
// Anything outside the subset is REJECTED at load (UnsupportedRuleError naming
// the construct): a security evaluator that silently accepts a rule it cannot
// evaluate is worse than one that does not detect. The one MVP rule
// (rules/windows/office_spawns_script_host.yml) is SPEC-006's — valid unchanged
// and evaluated identically.
//
// evaluateRule returns a RuleMatch (unchanged shape); it does NOT score (scorer)
// or assemble/persist the alert (alerts).

// `UnsupportedRuleError` lives in ./errors.js; re-exported here so existing
// imports from ./engine.js keep working unchanged.
export { UnsupportedRuleError };

/** An evaluator for one logsource category (SPEC-015 §Scope logsource dispatch). */
type CategoryEvaluator = (rule: SigmaRule, event: NormalizedProcessEvent) => RuleMatch | null;

/** The explicit modifiers (the no-modifier case is `exact`). */
const SUPPORTED_MODIFIERS: ReadonlySet<string> = new Set(["endswith", "startswith", "contains"]);

// The stable rule shell: scalar metadata, the logsource, the `cg:` block, and an
// opaque `detection` object whose dynamic keys are hand-validated below.
const cgSchema = z.object({
  heuristic_score: z.number().min(0).max(1),
  severity_id: z.number().int().min(0).max(6),
  cg_mitre: z.object({
    tactics: z.array(z.string().min(1)).nonempty(),
    techniques: z.array(z.string().min(1)).nonempty(),
  }),
});

const ruleShellSchema = z
  .object({
    id: z.string().min(1),
    title: z.string().min(1),
    level: z.string().min(1),
    logsource: z.object({ category: z.string().min(1) }).passthrough(),
    detection: z.record(z.unknown()),
    cg: cgSchema,
  })
  // Top-level metadata (status, description, references, tags, …) is ignored.
  .passthrough();

function zodDetail(error: z.ZodError): string {
  return error.issues.map((i) => `${i.path.join(".") || "(root)"}: ${i.message}`).join("; ");
}

/** Validate a field's values: a string or a non-empty list of non-empty, wildcard-free strings. */
function normalizeValues(field: string, block: string, raw: unknown): string[] {
  const list = Array.isArray(raw) ? raw : [raw];
  if (list.length === 0) {
    throw new UnsupportedRuleError(`field "${field}" in block "${block}" has an empty value list`);
  }
  const values: string[] = [];
  for (const v of list) {
    if (typeof v !== "string") {
      throw new UnsupportedRuleError(`field "${field}" in block "${block}" has a non-string value`);
    }
    if (v.length === 0) {
      throw new UnsupportedRuleError(`field "${field}" in block "${block}" has an empty value`);
    }
    if (v.includes("*") || v.includes("?")) {
      throw new UnsupportedRuleError(
        `field "${field}" in block "${block}" value "${v}" contains a wildcard (* or ?)`,
      );
    }
    values.push(v.toLowerCase());
  }
  return values;
}

function resolveModifier(field: string, modifier: string, block: string): SigmaModifier {
  if (modifier === "exact") {
    return "exact";
  }
  if (SUPPORTED_MODIFIERS.has(modifier)) {
    return modifier as SigmaModifier;
  }
  throw new UnsupportedRuleError(
    `unsupported modifier "${modifier}" on ${field} in block "${block}"`,
  );
}

/** Parse one `Field` / `Field|modifier` key + its values into a matcher. */
function parseFieldMatcher(block: string, key: string, raw: unknown): SigmaFieldMatcher {
  const pipeCount = (key.match(/\|/g) ?? []).length;
  if (pipeCount > 1) {
    throw new UnsupportedRuleError(`field "${key}" in block "${block}" has more than one modifier`);
  }
  const sep = key.indexOf("|");
  const field = sep === -1 ? key : key.slice(0, sep);
  const modifierName = sep === -1 ? "exact" : key.slice(sep + 1);

  if (field === "CommandLine" || field === "User") {
    throw new UnsupportedRuleError(
      `field "${field}" in block "${block}" is empty in CGES v0.1 and is deferred to B2`,
    );
  }
  if (field !== "Image" && field !== "ParentImage") {
    throw new UnsupportedRuleError(
      `unsupported field "${field}" in block "${block}" (only Image and ParentImage)`,
    );
  }
  return {
    field,
    modifier: resolveModifier(field, modifierName, block),
    values: normalizeValues(field, block, raw),
  };
}

/** Parse one named detection block (a plain object of at least one field). */
function parseBlock(name: string, value: unknown): SigmaBlock {
  if (!IDENTIFIER_RE.test(name)) {
    throw new UnsupportedRuleError(`invalid block name "${name}"`);
  }
  if (RESERVED_WORDS.has(name)) {
    throw new UnsupportedRuleError(`block name "${name}" is a reserved word`);
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new UnsupportedRuleError(
      `block "${name}" must be a map of fields (a list, string, or number is unsupported)`,
    );
  }
  const entries = Object.entries(value as Record<string, unknown>);
  if (entries.length === 0) {
    throw new UnsupportedRuleError(`block "${name}" has no fields`);
  }
  return { name, fields: entries.map(([key, raw]) => parseFieldMatcher(name, key, raw)) };
}

function fieldValue(
  field: SigmaFieldMatcher["field"],
  event: NormalizedProcessEvent,
): string | null {
  return field === "Image" ? event.imageFileName : event.parentImage;
}

function matchOne(value: string, modifier: SigmaModifier, pattern: string): boolean {
  if (modifier === "exact") {
    return value === pattern;
  }
  if (modifier === "endswith") {
    return value.endsWith(pattern);
  }
  if (modifier === "startswith") {
    return value.startsWith(pattern);
  }
  return value.includes(pattern); // contains
}

/** A matcher over a null field is false; otherwise the OR of its values, case-insensitively. */
function matcherMatches(matcher: SigmaFieldMatcher, event: NormalizedProcessEvent): boolean {
  const raw = fieldValue(matcher.field, event);
  if (raw === null) {
    return false;
  }
  const value = raw.toLowerCase();
  return matcher.values.some((pattern) => matchOne(value, matcher.modifier, pattern));
}

/** A block is the AND of its field matchers. */
function blockMatches(block: SigmaBlock, event: NormalizedProcessEvent): boolean {
  return block.fields.every((matcher) => matcherMatches(matcher, event));
}

function evaluateProcessCreation(rule: SigmaRule, event: NormalizedProcessEvent): RuleMatch | null {
  // process_creation rules evaluate on Launch (activity_id = 1) only; the guard
  // documents the intent rather than relying on Terminate carrying no parent.
  if (event.activityId !== 1) {
    return null;
  }
  const results = new Map<string, boolean>();
  for (const block of rule.blocks) {
    results.set(block.name, blockMatches(block, event));
  }
  if (!evaluateCondition(rule.condition, results)) {
    return null;
  }
  return {
    ruleId: rule.id,
    heuristicScore: rule.heuristicScore,
    severityId: rule.severityId,
    cgMitre: rule.cgMitre,
    sourceEvent: event,
  };
}

/** logsource.category → evaluator. Only process_creation is implemented (roadmap §D). */
const CATEGORY_DISPATCH: Record<string, CategoryEvaluator> = {
  process_creation: evaluateProcessCreation,
};

/** Validate and narrow a raw parsed rule into a SigmaRule, or throw UnsupportedRuleError. */
export function parseRule(raw: unknown): SigmaRule {
  const shell = ruleShellSchema.safeParse(raw);
  if (!shell.success) {
    throw new UnsupportedRuleError(zodDetail(shell.error));
  }
  const r = shell.data;

  const category = r.logsource.category;
  if (!Object.hasOwn(CATEGORY_DISPATCH, category)) {
    throw new UnsupportedRuleError(
      `unsupported logsource.category "${category}" (no evaluator; only "process_creation")`,
    );
  }

  const detection = r.detection;
  const conditionRaw = detection.condition;
  if (typeof conditionRaw !== "string") {
    throw new UnsupportedRuleError("detection.condition is required and must be a string");
  }
  if (Object.hasOwn(detection, "timeframe")) {
    throw new UnsupportedRuleError("detection.timeframe (correlation over time) is not supported");
  }

  const blocks: SigmaBlock[] = [];
  for (const [name, value] of Object.entries(detection)) {
    if (name === "condition") {
      continue;
    }
    blocks.push(parseBlock(name, value));
  }
  if (blocks.length === 0) {
    throw new UnsupportedRuleError("detection has no blocks");
  }

  const { ast, identifiers } = parseCondition(conditionRaw);
  const blockNames = new Set(blocks.map((b) => b.name));
  for (const id of identifiers) {
    if (!blockNames.has(id)) {
      throw new UnsupportedRuleError(`condition references undefined block "${id}"`);
    }
  }
  for (const name of blockNames) {
    if (!identifiers.has(name)) {
      throw new UnsupportedRuleError(`block "${name}" is not referenced by the condition`);
    }
  }

  return {
    id: r.id,
    title: r.title,
    level: r.level,
    logsourceCategory: category,
    blocks,
    condition: ast,
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

/**
 * Evaluate one event against one rule, dispatched by logsource category. Returns
 * a RuleMatch on match, else null. For process_creation: the `activity_id = 1`
 * guard holds; each block is the AND of its matchers (a matcher over a null
 * field is false — the documented v0.1 unknown-parent case, which never
 * suppresses); the parsed condition combines the block results.
 */
export function evaluateRule(rule: SigmaRule, event: NormalizedProcessEvent): RuleMatch | null {
  const evaluator = CATEGORY_DISPATCH[rule.logsourceCategory];
  if (evaluator === undefined) {
    return null;
  }
  return evaluator(rule, event);
}
