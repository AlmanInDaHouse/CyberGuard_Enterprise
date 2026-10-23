import { createHash } from "node:crypto";
import type { FastifyInstance, FastifyRequest } from "fastify";
import { appendAudit } from "../auth/audit.js";
import { requestContext } from "../auth/context.js";
import { makeRequireRole, makeRequireSession } from "../auth/prehandlers.js";
import { buildForensicExport } from "../forensic/export.js";
import { composeReport } from "../forensic/report.js";
import type { Services } from "../services.js";
import { fromStore } from "../stores.js";
import {
  getIncidentDetail,
  getIncidentEventTimeline,
  getIncidentPlaybookExecutions,
  isUuid,
  listAlerts,
  listIncidents,
} from "./queries.js";

/** The `:id` of an incident route, lower-cased, or null when it is not a canonical UUID. */
function incidentId(req: FastifyRequest): string | null {
  const { id } = req.params as { id: string };
  return isUuid(id) ? id.toLowerCase() : null;
}

/**
 * SPEC-009 §Operational §1/§2 — the read-API surface. Every read sits behind
 * SPEC-008's session preHandler (`makeRequireSession`): an unauthenticated/revoked
 * read is rejected with 401 BEFORE the handler. Role + org come from the session,
 * never the request (threat model :84; :103 restates it platform-wide); every
 * query is org-scoped to `session.org_id`. All three roles read in the MVP
 * (§Operational §2), except the forensic export and report, which only an admin or an
 * analyst may request (SPEC-009's third Amendment 2026-10-15).
 *
 * Errors (SPEC-009 Amendment 2026-10-14): a malformed `:id` is a 404 decided before any
 * store is read; a malformed list parameter (400) and a store that cannot answer (503)
 * reach the app's error handler.
 */
export function registerReadRoutes(app: FastifyInstance, services: Services): void {
  const requireSession = makeRequireSession(services);
  const requireForensicRole = makeRequireRole("admin", "analyst");

  /** SPEC-009's third Amendment 2026-10-15, item 3: the row that records a forensic
   *  document and what the key signed for it, written before the document is sent. A
   *  store that cannot write it is a 503, and the document is not sent. */
  async function recordDocument(
    req: FastifyRequest,
    action: "forensic.export" | "forensic.report",
    incident: string,
    detail: Record<string, string>,
  ): Promise<void> {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const ctx = requestContext(req, services.trustedProxies);
    await fromStore(() =>
      appendAudit(services.pg, {
        userId: session.user_id,
        orgId: session.org_id,
        action,
        targetId: incident,
        outcome: "ok",
        sourceIp: ctx.sourceIp,
        correlationId: ctx.correlationId,
        detail,
      }),
    );
  }

  app.get("/v1/incidents", { onRequest: requireSession }, async (req) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const q = req.query as {
      status?: string;
      agent_id?: string;
      limit?: string;
      cursor?: string;
    };
    return listIncidents(services.pg, session.org_id, {
      status: q.status,
      agentId: q.agent_id,
      limit: q.limit,
      cursor: q.cursor,
    });
  });

  app.get("/v1/incidents/:id", { onRequest: requireSession }, async (req, reply) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const id = incidentId(req);
    if (!id) return reply.code(404).send({ error: "not_found" });
    const detail = await getIncidentDetail(services.pg, session.org_id, id);
    if (!detail) return reply.code(404).send({ error: "not_found" });
    return detail;
  });

  // SPEC-010 — the forensic event drill (step 1): incident → raw cges_events
  // timeline. Nested resource (precedent: auth/routes.ts:92-93 /v1/users/:id/role); the
  // first read that crosses the Postgres→ClickHouse boundary (ADR-0015 — services.ch).
  // Same session preHandler + org-scope; read-only (no role gate, no CSRF, NFR-009-004).
  // 404 mirrors the detail handler above (malformed :id / not-found / cross-org).
  app.get("/v1/incidents/:id/events", { onRequest: requireSession }, async (req, reply) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const id = incidentId(req);
    if (!id) return reply.code(404).send({ error: "not_found" });
    const timeline = await getIncidentEventTimeline(services.pg, services.ch, session.org_id, id);
    if (!timeline) return reply.code(404).send({ error: "not_found" });
    return timeline;
  });

  // SPEC-012 — the on-demand forensic snapshot export (escalón 3): the incident's
  // canonicalized evidence + the SHA-256 hash-chain head chain_N + the Ed25519
  // root_signature over chain_N + the embedded forensic public key. Same session
  // preHandler + org-scope as the drill, then an admin or analyst role, before the :id
  // is read; an audit row before the export is sent; no HEAD form, which would sign
  // without sending (SPEC-009's third Amendment 2026-10-15). No CSRF.
  // 404 mirrors the detail/events handlers (malformed :id / not-found / cross-org).
  // The export shape carries NO private-key field, so this route can never serialize
  // the forensic private key. The embedded pubkey is a convenience, NOT a trust root
  // (SPEC-012 §Compliance line 107): the anchor is the fingerprint recorded at
  // installation, which verify-export.js checks it against (SPEC-023 §Operational §11).
  app.get(
    "/v1/incidents/:id/forensic-export",
    { onRequest: [requireSession, requireForensicRole], exposeHeadRoute: false },
    async (req, reply) => {
      const session = req.authSession;
      if (!session) throw new Error("unreachable: requireSession populates authSession");
      const id = incidentId(req);
      if (!id) return reply.code(404).send({ error: "not_found" });
      const exportDoc = await buildForensicExport(services, session.org_id, id);
      if (!exportDoc) return reply.code(404).send({ error: "not_found" });
      await recordDocument(req, "forensic.export", id, {
        events: String(exportDoc.events.length),
        chain_root: exportDoc.chain_root,
      });
      return exportDoc;
    },
  );

  // SPEC-013 — the forensic report render (escalón 4): the per-incident PDF composing the
  // SPEC-010 timeline + SPEC-011 severity + the SPEC-012 seal (chain_root + root_signature +
  // forensic_pubkey, transcribed), built programmatically via @react-pdf/renderer. Same session
  // preHandler, role gate, audit row and absent HEAD form as the export; no CSRF. 404
  // mirrors them (malformed :id / not-found / cross-org). This is the FIRST non-JSON response in
  // the api: `reply.type("application/pdf")`. `composeReport` carries only the public seal — the
  // forensic private key is never serialized (inherited from SPEC-012).
  app.get(
    "/v1/incidents/:id/report",
    { onRequest: [requireSession, requireForensicRole], exposeHeadRoute: false },
    async (req, reply) => {
      const session = req.authSession;
      if (!session) throw new Error("unreachable: requireSession populates authSession");
      const id = incidentId(req);
      if (!id) return reply.code(404).send({ error: "not_found" });
      const report = await composeReport(services, session.org_id, id);
      if (!report) return reply.code(404).send({ error: "not_found" });
      await recordDocument(req, "forensic.report", id, {
        events: String(report.seal.events.length),
        chain_root: report.seal.chain_root,
        report_sha256: createHash("sha256").update(report.pdf).digest("hex"),
      });
      return reply.type("application/pdf").send(report.pdf);
    },
  );

  // SPEC-022 §Data contracts §4 — the playbook executions of an incident (ADR-0021
  // §4). Same session preHandler and org scope as the sibling reads; GET, read-only.
  // 404 for a malformed :id, an absent incident or another org's; 503 and 500 as
  // SPEC-009's Amendment 2026-10-14 states.
  app.get(
    "/v1/incidents/:id/playbook-executions",
    { onRequest: requireSession },
    async (req, reply) => {
      const session = req.authSession;
      if (!session) throw new Error("unreachable: requireSession populates authSession");
      const id = incidentId(req);
      if (!id) return reply.code(404).send({ error: "not_found" });
      const executions = await getIncidentPlaybookExecutions(services.pg, session.org_id, id);
      if (!executions) return reply.code(404).send({ error: "not_found" });
      return executions;
    },
  );

  app.get("/v1/alerts", { onRequest: requireSession }, async (req) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const q = req.query as {
      status?: string;
      agent_id?: string;
      severity_id?: string;
      limit?: string;
      cursor?: string;
    };
    return listAlerts(services.pg, session.org_id, {
      status: q.status,
      agentId: q.agent_id,
      severityId: q.severity_id,
      limit: q.limit,
      cursor: q.cursor,
    });
  });
}
