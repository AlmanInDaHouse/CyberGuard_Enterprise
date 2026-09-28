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

// SPEC-015 / SPEC-016 — the generalized Sigma-subset rule evaluator and its
// loader. Loads the `*.yml` / `*.yaml` rules directly under rules/windows/ (the
// loader does not recurse), validates each against the SPEC-015 §Scope subset
// and the SPEC-016 §Data contracts §1 loader contract, and evaluates normalized
// events against them. The subset:
//   - fields Image and ParentImage — the populated v0.1 process fields (SPEC-006
//     §Data contracts; ADR-0012:61). CommandLine / User are rejected (empty in
//     v0.1, deferred to B2).
//   - modifiers: exact match (no modifier), endswith, startswith, contains,
//     all case-insensitive. An explicit `|exact` is rejected (Sigma has none).
//   - one or more named detection blocks (a block is the AND of its fields; a
//     field is the OR of its values) combined by a boolean `condition` over
//     and / or / not / parentheses (parsed by ./condition.js).
//   - logsource.category resolved against a dispatch table; only
//     `process_creation` is implemented (roadmap §D adds the others).
// The loader contract, on top of the subset:
//   - `id` is `rule.<file name without extension>` (`^rule\.[a-z0-9_]+$`), and
//     no two files carry the same id;
//   - `logsource.product` is `windows`;
//   - `level` is informational / low / medium / high / critical, and
//     `cg.severity_id` is its OCSF value (1–5);
//   - `cg` holds only heuristic_score, severity_id, cg_detection_source and
//     cg_mitre (tactics, techniques); cg_detection_source is always `rule`
//     (ADR-0012 §Compliance); tactics are ATT&CK Enterprise tactic names and
//     techniques TNNNN[.NNN], none repeated;
//   - no value contains `\\` (one escaped backslash in Sigma; values are
//     matched literally here);
//   - the condition is false when every block is false: a rule must need at
//     least one block to match.
// Anything outside is REJECTED at load (UnsupportedRuleError naming the
// construct): a security evaluator that silently accepts a rule it cannot
// evaluate is worse than one that does not detect. The SPEC-006 MVP rule
// (rules/windows/office_spawns_script_host.yml) is valid under both and
// evaluates identically.
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

/** SPEC-016 §Data contracts §1 — a rule id. */
const RULE_ID_RE = /^rule\.[a-z0-9_]+$/;

/** Sigma `level` → OCSF severity_id (schemas/cges/v0.1/common/ocsf_severity.json). */
const LEVEL_SEVERITY: ReadonlyMap<string, number> = new Map([
  ["informational", 1],
  ["low", 2],
  ["medium", 3],
  ["high", 4],
  ["critical", 5],
]);

/** The fourteen MITRE ATT&CK Enterprise tactics, in the kebab-case names of common/cg_mitre.json. */
const ATTACK_TACTICS: ReadonlySet<string> = new Set([
  "reconnaissance",
  "resource-development",
  "initial-access",
  "execution",
  "persistence",
  "privilege-escalation",
  "defense-evasion",
  "credential-access",
  "discovery",
  "lateral-movement",
  "collection",
  "command-and-control",
  "exfiltration",
  "impact",
]);

/** An ATT&CK technique id, TNNNN or TNNNN.NNN (common/cg_mitre.json). */
const TECHNIQUE_RE = /^T[0-9]{4}(\.[0-9]{3})?$/;

// The stable rule shell: scalar metadata, the logsource, the `cg:` block, and an
// opaque `detection` object whose dynamic keys are hand-validated below. `cg` and
// `cg_mitre` are strict: an unknown key is rejected, not dropped (SPEC-016).
const cgSchema = z
  .object({
    heuristic_score: z.number().min(0).max(1),
    severity_id: z.number().int().min(0).max(6),
    // Checked by hand in checkRuleMetadata, so a missing or wrong value is named.
    cg_detection_source: z.unknown(),
    cg_mitre: z
      .object({
        tactics: z.array(z.string().min(1)).nonempty(),
        techniques: z.array(z.string().min(1)).nonempty(),
      })
      .strict(),
  })
  .strict();

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

type RuleShell = z.infer<typeof ruleShellSchema>;

function zodDetail(error: z.ZodError): string {
  return error.issues.map((i) => `${i.path.join(".") || "(root)"}: ${i.message}`).join("; ");
}

/** A metadata value for an error message: `missing`, or its JSON form. */
function describe(value: unknown): string {
  return value === undefined ? "missing" : JSON.stringify(value);
}

/** Reject the first repeated entry of a cg_mitre list. */
function assertNoRepeats(list: readonly string[], where: string): void {
  const seen = new Set<string>();
  for (const item of list) {
    if (seen.has(item)) {
      throw new UnsupportedRuleError(`${where} "${item}" is repeated`);
    }
    seen.add(item);
  }
}

/**
 * The metadata half of the SPEC-016 §Data contracts §1 loader contract: id shape,
 * product, level ↔ severity_id, cg_detection_source, and the ATT&CK vocabulary.
 */
function checkRuleMetadata(r: RuleShell): void {
  if (!RULE_ID_RE.test(r.id)) {
    throw new UnsupportedRuleError(`id "${r.id}" does not match ^rule\\.[a-z0-9_]+$`);
  }
  const product: unknown = r.logsource.product;
  if (product !== "windows") {
    throw new UnsupportedRuleError(
      `logsource.product ${describe(product)} is not supported (only "windows")`,
    );
  }
  const severity = LEVEL_SEVERITY.get(r.level);
  if (severity === undefined) {
    throw new UnsupportedRuleError(
      `level "${r.level}" is not one of ${[...LEVEL_SEVERITY.keys()].join(", ")}`,
    );
  }
  if (r.cg.severity_id !== severity) {
    throw new UnsupportedRuleError(
      `cg.severity_id ${r.cg.severity_id} does not match level "${r.level}" (OCSF severity_id ${severity})`,
    );
  }
  const source = r.cg.cg_detection_source;
  if (source !== "rule") {
    throw new UnsupportedRuleError(
      `cg.cg_detection_source ${describe(source)} is not supported (only "rule": ml and hybrid need a model pairing that no rule can carry)`,
    );
  }
  for (const tactic of r.cg.cg_mitre.tactics) {
    if (!ATTACK_TACTICS.has(tactic)) {
      throw new UnsupportedRuleError(
        `cg.cg_mitre.tactics "${tactic}" is not a MITRE ATT&CK Enterprise tactic name`,
      );
    }
  }
  assertNoRepeats(r.cg.cg_mitre.tactics, "cg.cg_mitre.tactics");
  for (const technique of r.cg.cg_mitre.techniques) {
    if (!TECHNIQUE_RE.test(technique)) {
      throw new UnsupportedRuleError(
        `cg.cg_mitre.techniques "${technique}" does not match ^T[0-9]{4}(\\.[0-9]{3})?$`,
      );
    }
  }
  assertNoRepeats(r.cg.cg_mitre.techniques, "cg.cg_mitre.techniques");
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
    if (v.includes("\\\\")) {
      throw new UnsupportedRuleError(
        `field "${field}" in block "${block}" value "${v}" contains "\\\\" (Sigma reads it as one escaped backslash; values are matched literally)`,
      );
    }
    values.push(v.toLowerCase());
  }
  return values;
}

/**
 * Resolve an EXPLICIT `|modifier`. The no-modifier case (`exact`) is the
 * caller's; an explicit `|exact` is rejected — Sigma has no `|exact`, and exact
 * match is written without a modifier (SPEC-015 §Scope).
 */
function resolveModifier(field: string, modifier: string, block: string): SigmaModifier {
  if (SUPPORTED_MODIFIERS.has(modifier)) {
    return modifier as SigmaModifier;
  }
  if (modifier === "exact") {
    throw new UnsupportedRuleError(
      `unsupported modifier "exact" on ${field} in block "${block}" (exact match is written without a modifier)`,
    );
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
    // No `|` ⇒ exact (written without a modifier); an explicit `|modifier` must
    // be one of SUPPORTED_MODIFIERS (resolveModifier rejects an explicit exact).
    modifier: sep === -1 ? "exact" : resolveModifier(field, key.slice(sep + 1), block),
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
  checkRuleMetadata(r);

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
  // SPEC-016 §Data contracts §1: a rule must need at least one block to match —
  // `not filter`, or `selection or not filter`, would fire on almost every process.
  const noBlockMatches = new Map(blocks.map((b): [string, boolean] => [b.name, false]));
  if (evaluateCondition(ast, noBlockMatches)) {
    throw new UnsupportedRuleError(
      `condition "${conditionRaw}" is true when no block matches (a rule must need at least one block to match)`,
    );
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

/**
 * Load + validate every `*.yml` / `*.yaml` rule directly in `rulesDir` (no
 * recursion), in file-name order. On top of parseRule, each `id` must be
 * `rule.<file name without extension>` and unique across the directory
 * (SPEC-016 §Data contracts §1). A rejection names the file and the construct.
 */
export function loadRules(rulesDir: string): SigmaRule[] {
  const rules: SigmaRule[] = [];
  const loaded = new Set<string>();
  const files = readdirSync(rulesDir)
    .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
    .sort();
  for (const file of files) {
    let rule: SigmaRule;
    try {
      rule = parseRule(parseYaml(readFileSync(join(rulesDir, file), "utf-8")));
    } catch (err) {
      if (err instanceof UnsupportedRuleError) {
        throw new UnsupportedRuleError(`rule file "${file}": ${err.detail}`);
      }
      throw err;
    }
    const expectedId = `rule.${file.replace(/\.ya?ml$/, "")}`;
    if (rule.id !== expectedId) {
      throw new UnsupportedRuleError(
        `rule file "${file}": id "${rule.id}" does not match its file name (expected "${expectedId}")`,
      );
    }
    if (loaded.has(rule.id)) {
      throw new UnsupportedRuleError(`rule file "${file}": id "${rule.id}" is already loaded`);
    }
    loaded.add(rule.id);
    rules.push(rule);
  }
  return rules;
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
