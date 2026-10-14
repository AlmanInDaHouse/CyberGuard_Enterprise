import type { UnlinkedStep } from "./chain.js";
import type { IncidentCreated } from "./types.js";

// SPEC-022 §Data contracts §1 — a template's placeholders are a closed set, which
// the loader has already checked. Values are substituted as plain text.

export interface TemplateContext {
  incident: IncidentCreated;
  playbookId: string;
  executionId: string;
  /** The steps that ran before the one being rendered. */
  stepsSoFar: UnlinkedStep[];
}

/** `<step id> <action> <mode>: <outcome> — <detail>`, one line per step. */
export function stepLines(steps: UnlinkedStep[]): string {
  return steps
    .map((s) => `${s.step_id} ${s.action} ${s.mode}: ${s.outcome} — ${s.detail}`)
    .join("\n");
}

export function renderTemplate(template: string, ctx: TemplateContext): string {
  const values: Record<string, string> = {
    "incident.id": ctx.incident.incidentId,
    "incident.title": ctx.incident.title,
    "incident.severity_id": String(ctx.incident.severityId),
    "incident.agent_id": ctx.incident.agentId,
    "incident.org_id": ctx.incident.orgId,
    "playbook.id": ctx.playbookId,
    "execution.id": ctx.executionId,
    steps: stepLines(ctx.stepsSoFar),
  };
  return template.replace(/\{\{([^{}]*)\}\}/g, (_whole, name: string) => values[name] ?? "");
}

/**
 * A rendered subject with every line break replaced by a space: CR LF, CR, LF, and
 * the other Unicode line breaks (VT, FF, NEL, LS, PS).
 */
export function renderSubject(template: string, ctx: TemplateContext): string {
  return renderTemplate(template, ctx).replace(/\r\n|[\n\v\f\r\u0085\u2028\u2029]/g, " ");
}
