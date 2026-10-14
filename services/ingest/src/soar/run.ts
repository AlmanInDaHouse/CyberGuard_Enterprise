import { v7 as uuidv7 } from "uuid";
import { isSendTimeout, sendFailureCode } from "../notify/errors.js";
import type { NotifyConfig } from "../notify/index.js";
import { type UnlinkedStep, chainSteps } from "./chain.js";
import { type TemplateContext, renderSubject, renderTemplate } from "./template.js";
import {
  DEFAULT_SOAR_BOUNDS,
  type IncidentCreated,
  type Playbook,
  type PlaybookStep,
  type SoarBounds,
  type SoarConfig,
  type StepOutcome,
} from "./types.js";

// SPEC-022 §Operational §2–§6 / ADR-0021 — run the matching playbooks of a newly
// created incident. Called by runDetectionCycle after SPEC-014's notification and
// awaited; it never throws (ADR-0017 §2's fire-and-forget terms).

const DETAIL_MAX_CODE_POINTS = 500;

/** Cut to 500 Unicode code points (SPEC-022 §Data contracts §2). */
function cut(text: string): string {
  const points = Array.from(text);
  return points.length <= DETAIL_MAX_CODE_POINTS
    ? text
    : points.slice(0, DETAIL_MAX_CODE_POINTS).join("");
}

/** In the MVP only a rule-sourced incident matches, through `rule` (§Operational §2). */
export function playbookMatches(playbook: Playbook, incident: IncidentCreated): boolean {
  return (
    playbook.trustSources.includes("rule") &&
    incident.detectionSource === "rule" &&
    incident.severityId >= playbook.severityIdGte
  );
}

interface StepResult {
  outcome: StepOutcome;
  detail: string;
}

const TIMED_OUT = Symbol("timed out");

/** Race `work` against `ms`; the loser is left to settle on its own. */
async function withLimit<T>(work: Promise<T>, ms: number): Promise<T | typeof TIMED_OUT> {
  let timer: NodeJS.Timeout | undefined;
  const limit = new Promise<typeof TIMED_OUT>((resolve) => {
    timer = setTimeout(() => resolve(TIMED_OUT), ms);
  });
  try {
    return await Promise.race([work, limit]);
  } finally {
    clearTimeout(timer);
  }
}

async function executeStep(
  step: PlaybookStep,
  ctx: TemplateContext,
  notify: NotifyConfig | undefined,
  limitMs: number,
): Promise<StepResult | typeof TIMED_OUT> {
  if (step.action === "agent.isolate") {
    // dry_run only (ADR-0021 §3; the loader refuses a live one). Nothing leaves.
    return {
      outcome: "would_do",
      detail: `would isolate agent ${ctx.incident.agentId}; no command channel to the agent exists (ADR-0021 §Context 6)`,
    };
  }
  const params = step.params;
  if (!params) return { outcome: "failed", detail: "notify.email has no params" };
  const subject = renderSubject(params.subject, ctx);
  if (step.mode === "dry_run") return { outcome: "would_do", detail: `would send: ${subject}` };
  if (notify === undefined) {
    return { outcome: "skipped", detail: "notification is not configured" };
  }
  try {
    await notify.mailer.sendMail(
      { from: notify.from, to: notify.recipient, subject, text: renderTemplate(params.body, ctx) },
      limitMs,
    );
    return { outcome: "done", detail: "sent" };
  } catch (err) {
    if (isSendTimeout(err)) return TIMED_OUT;
    return { outcome: "failed", detail: `send failed: ${sendFailureCode(err)}` };
  }
}

async function runOne(
  soar: SoarConfig,
  notify: NotifyConfig | undefined,
  playbook: Playbook,
  incident: IncidentCreated,
  bounds: SoarBounds,
): Promise<void> {
  const ids = { playbook_id: playbook.id, incident_id: incident.incidentId };
  const executionId = uuidv7();

  // 1. Claim: the unique key is the idempotency key (§Operational §3).
  let claimed: boolean;
  try {
    const claim = await soar.pg.query<{ execution_id: string }>(
      `INSERT INTO playbook_executions
         (execution_id, org_id, incident_id, agent_id, playbook_id, playbook_version)
       VALUES ($1, $2, $3, $4, $5, $6)
       ON CONFLICT (org_id, playbook_id, incident_id) DO NOTHING
       RETURNING execution_id`,
      [
        executionId,
        incident.orgId,
        incident.incidentId,
        incident.agentId,
        playbook.id,
        playbook.version,
      ],
    );
    claimed = claim.rows.length === 1;
  } catch (err) {
    soar.log.warn(
      { event: "soar_run_claim_failed", ...ids, code: sendFailureCode(err) },
      "SOAR run not claimed",
    );
    return;
  }
  if (!claimed) {
    soar.log.debug({ event: "soar_run_skipped_claimed", ...ids }, "SOAR run already claimed");
    return;
  }

  // 2. Steps, each under its bound; a failed step ends the run.
  const runDeadline = Date.now() + bounds.runMs;
  const steps: UnlinkedStep[] = [];
  let failed = false;
  for (const step of playbook.steps) {
    const started = new Date().toISOString();
    const record = (outcome: StepOutcome, detail: string): void => {
      steps.push({
        step_id: step.id,
        action: step.action,
        mode: step.mode,
        outcome,
        detail: cut(detail),
        started_at: started,
        finished_at: new Date().toISOString(),
      });
    };
    if (failed) {
      record("skipped", "an earlier step failed");
      continue;
    }
    const remaining = runDeadline - Date.now();
    if (remaining <= 0) {
      failed = true;
      record("failed", `run timed out after ${bounds.runMs} ms`);
      continue;
    }
    const cutByRun = remaining < bounds.stepMs;
    const limitMs = Math.min(bounds.stepMs, remaining);
    const ctx: TemplateContext = {
      incident,
      playbookId: playbook.id,
      executionId,
      stepsSoFar: [...steps],
    };
    const result = await withLimit(executeStep(step, ctx, notify, limitMs), limitMs);
    if (result === TIMED_OUT) {
      failed = true;
      record(
        "failed",
        cutByRun ? `run timed out after ${bounds.runMs} ms` : `timed out after ${bounds.stepMs} ms`,
      );
      continue;
    }
    if (result.outcome === "failed") failed = true;
    record(result.outcome, result.detail);
  }

  // 3. Complete, once.
  const chained = chainSteps(steps);
  const status = failed ? "failed" : "completed";
  let recorded = true;
  try {
    await soar.pg.query(
      `UPDATE playbook_executions
          SET status = $2, finished_at = now(), steps = $3::jsonb, chain_head = $4
        WHERE execution_id = $1`,
      [executionId, status, JSON.stringify(chained.steps), chained.head],
    );
  } catch (err) {
    soar.log.warn(
      {
        event: "soar_run_record_failed",
        ...ids,
        execution_id: executionId,
        code: sendFailureCode(err),
      },
      "SOAR run record not completed",
    );
    recorded = false;
  }

  // 4. Log, whether or not the record was completed.
  soar.log.info(
    {
      event: "soar_run",
      ...ids,
      execution_id: executionId,
      status,
      recorded,
      steps: chained.steps.map((s) => `${s.step_id}:${s.outcome}`),
    },
    "SOAR run",
  );
}

/**
 * Run each matching playbook of a newly created incident, one after another in id
 * order. Never throws.
 */
export async function runPlaybooks(
  soar: SoarConfig,
  notify: NotifyConfig | undefined,
  incident: IncidentCreated,
): Promise<void> {
  const bounds: SoarBounds = { ...DEFAULT_SOAR_BOUNDS, ...soar.bounds };
  const playbooks = [...soar.playbooks].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  for (const playbook of playbooks) {
    if (!playbookMatches(playbook, incident)) continue;
    try {
      await runOne(soar, notify, playbook, incident, bounds);
    } catch (err) {
      soar.log.warn(
        {
          event: "soar_run_error",
          playbook_id: playbook.id,
          incident_id: incident.incidentId,
          code: sendFailureCode(err),
        },
        "SOAR run failed unexpectedly",
      );
    }
  }
}
