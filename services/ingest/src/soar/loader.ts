import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Ajv2020 as Ajv2020Class, ErrorObject, ValidateFunction } from "ajv/dist/2020.js";
import { parse } from "yaml";
import type { LoadedPlaybooks, Playbook, PlaybookStep, SoarAction, StepMode } from "./types.js";

// SPEC-022 §Operational §1 — the playbook loader. It runs first in startIngest,
// before any client is built, at every boot. A refused file stops the boot; an
// absent or empty directory leaves SOAR idle (ADR-0021 §2).

// ajv is CJS; load it via createRequire, the pattern ingest uses for canonicalize
// and nodemailer. The module exports the class by its name as well.
const Ajv2020 = (
  createRequire(import.meta.url)("ajv/dist/2020") as { Ajv2020: typeof Ajv2020Class }
).Ajv2020;

const here = dirname(fileURLToPath(import.meta.url));
/** The repository root, from src/soar or dist/soar alike. */
const REPO_ROOT = join(here, "..", "..", "..", "..");

/** A playbook the loader refuses (SPEC-022 §Operational §1). */
export class PlaybookLoadError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PlaybookLoadError";
  }
}

/** The placeholders a template may hold (SPEC-022 §Data contracts §1). */
export const PLACEHOLDERS = [
  "incident.id",
  "incident.title",
  "incident.severity_id",
  "incident.agent_id",
  "incident.org_id",
  "playbook.id",
  "execution.id",
  "steps",
] as const;

const PLACEHOLDER = /\{\{([^{}]*)\}\}/g;

/** `<cwd>/<rel>` when the image bundles it, otherwise the repository's copy. */
function bundledOrRepo(...rel: string[]): string {
  const packaged = join(process.cwd(), ...rel);
  return existsSync(packaged) ? packaged : join(REPO_ROOT, ...rel);
}

/** The default playbooks directory, resolved as the rules' is (config.ts). */
export function defaultPlaybooksDir(): string {
  return bundledOrRepo("playbooks");
}

function compileSchema(): ValidateFunction {
  const schemaPath = bundledOrRepo("schemas", "playbooks", "v0.1", "playbook.json");
  const ajv = new Ajv2020({ allErrors: false, strict: true });
  return ajv.compile(JSON.parse(readFileSync(schemaPath, "utf8")));
}

function describeSchemaError(err: ErrorObject | undefined): string {
  if (!err) return "does not match the playbook schema";
  const where = err.instancePath === "" ? "the document" : err.instancePath;
  const extra =
    err.keyword === "additionalProperties"
      ? ` (${String((err.params as { additionalProperty?: unknown }).additionalProperty)})`
      : "";
  return `${where} ${err.message ?? "is invalid"}${extra}`;
}

/** The placeholders of a template, or why the template is refused. */
function checkTemplate(text: string, field: "subject" | "body"): string | null {
  const allowed = new Set<string>(PLACEHOLDERS);
  if (field === "subject") allowed.delete("steps");
  for (const match of text.matchAll(PLACEHOLDER)) {
    // Written {{name}}, with no spaces inside (SPEC-022 §Data contracts §1).
    const name = match[1] ?? "";
    if (!allowed.has(name))
      return `${field} holds a placeholder that is not allowed there: {{${name}}}`;
  }
  const rest = text.replace(PLACEHOLDER, "");
  if (rest.includes("{{") || rest.includes("}}")) return `${field} holds a malformed placeholder`;
  return null;
}

interface RawStep {
  id: string;
  action: SoarAction;
  mode?: StepMode;
  params?: { subject: string; body: string };
}

interface RawPlaybook {
  id: string;
  version: number;
  name: string;
  trust_sources: string[];
  trigger: { on: string; when: { severity_id_gte: number } };
  steps: RawStep[];
}

/** The policy JSON Schema cannot state (SPEC-022 §Operational §1). */
function checkPolicy(doc: RawPlaybook): string | null {
  const stepIds = new Set<string>();
  for (const step of doc.steps) {
    if (stepIds.has(step.id)) return `step id ${step.id} is not unique`;
    stepIds.add(step.id);
    if (step.action === "agent.isolate" && step.mode === "live") {
      return `step ${step.id}: agent.isolate runs in dry_run only (ADR-0021 §3)`;
    }
    if (step.params) {
      const why =
        checkTemplate(step.params.subject, "subject") ?? checkTemplate(step.params.body, "body");
      if (why) return `step ${step.id}: ${why}`;
    }
  }
  return null;
}

function toPlaybook(doc: RawPlaybook, file: string): Playbook {
  return {
    id: doc.id,
    version: doc.version,
    name: doc.name,
    trustSources: [...doc.trust_sources],
    severityIdGte: doc.trigger.when.severity_id_gte,
    steps: doc.steps.map(
      (s): PlaybookStep => ({
        id: s.id,
        action: s.action,
        mode: s.mode ?? "dry_run",
        ...(s.params ? { params: { subject: s.params.subject, body: s.params.body } } : {}),
      }),
    ),
    file,
  };
}

/**
 * Load the playbooks of `configuredDir`, or of the default directory when it is
 * unset. Throws PlaybookLoadError, naming the file, on a playbook that fails the
 * schema or the policy; returns `idle` when there is nothing to run.
 */
export function loadPlaybooks(configuredDir: string | undefined): LoadedPlaybooks {
  const dir = configuredDir ?? defaultPlaybooksDir();
  if (!existsSync(dir) || !statSync(dir).isDirectory()) {
    return {
      dir,
      playbooks: [],
      idle: {
        reason: "directory_absent",
        dir,
        level: configuredDir === undefined ? "info" : "warn",
      },
    };
  }
  const names = readdirSync(dir)
    .filter((name) => /\.ya?ml$/.test(name) && statSync(join(dir, name)).isFile())
    .sort();
  if (names.length === 0) {
    return { dir, playbooks: [], idle: { reason: "no_playbooks", dir, level: "info" } };
  }

  const validate = compileSchema();
  const seen = new Map<string, string>();
  const playbooks = names.map((name) => {
    const file = join(dir, name);
    const refuse = (why: string) => new PlaybookLoadError(`playbook ${name}: ${why}`);
    let doc: unknown;
    try {
      doc = parse(readFileSync(file, "utf8"));
    } catch (err) {
      throw refuse(
        `is not YAML: ${err instanceof Error ? err.message.split("\n")[0] : String(err)}`,
      );
    }
    if (!validate(doc)) throw refuse(describeSchemaError(validate.errors?.[0]));
    const raw = doc as RawPlaybook;
    const why = checkPolicy(raw);
    if (why) throw refuse(why);
    const earlier = seen.get(raw.id);
    if (earlier) throw refuse(`playbook id ${raw.id} is not unique (also in ${earlier})`);
    seen.set(raw.id, name);
    return toPlaybook(raw, file);
  });
  return { dir, playbooks };
}
