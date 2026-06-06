import type { FastifyInstance } from "fastify";
import { makeRequireSession } from "../auth/prehandlers.js";
import { buildForensicExport } from "../forensic/export.js";
import type { Services } from "../services.js";
import {
  getIncidentDetail,
  getIncidentEventTimeline,
  listAlerts,
  listIncidents,
} from "./queries.js";

/**
 * SPEC-009 §Operational §1/§2 — the read-API surface. Every read sits behind
 * SPEC-008's session preHandler (`makeRequireSession`): an unauthenticated/revoked
 * read is rejected with 401 BEFORE the handler. Role + org come from the session,
 * never the request (threat model :84; :103 restates it platform-wide); every
 * query is org-scoped to `session.org_id`. All three roles read in the MVP
 * (§Operational §2) — RBAC of reads is session + org-scope, with the role ×
 * read-capability matrix deferred (no role-restricted read exists in the MVP); so
 * no `makeRequireRole` gate restricts these routes.
 */
export function registerReadRoutes(app: FastifyInstance, services: Services): void {
  const requireSession = makeRequireSession(services);

  app.get("/v1/incidents", { preHandler: requireSession }, async (req) => {
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

  app.get("/v1/incidents/:id", { preHandler: requireSession }, async (req, reply) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const { id } = req.params as { id: string };
    let detail: Awaited<ReturnType<typeof getIncidentDetail>>;
    try {
      detail = await getIncidentDetail(services.pg, session.org_id, id);
    } catch {
      // Malformed id (e.g. not a UUID) → generic 404, no internal detail.
      return reply.code(404).send({ error: "not_found" });
    }
    if (!detail) return reply.code(404).send({ error: "not_found" });
    return detail;
  });

  // SPEC-010 — the forensic event drill (step 1): incident → raw cges_events
  // timeline. Nested resource (precedent: auth/routes.ts:63 /v1/users/:id/role); the
  // first read that crosses the Postgres→ClickHouse boundary (ADR-0015 — services.ch).
  // Same session preHandler + org-scope; read-only (no role gate, no CSRF, NFR-009-004).
  // 404 mirrors the detail handler above (malformed :id / not-found / cross-org).
  app.get("/v1/incidents/:id/events", { preHandler: requireSession }, async (req, reply) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const { id } = req.params as { id: string };
    let timeline: Awaited<ReturnType<typeof getIncidentEventTimeline>>;
    try {
      timeline = await getIncidentEventTimeline(services.pg, services.ch, session.org_id, id);
    } catch {
      // Malformed id (e.g. not a UUID) → generic 404, no internal detail.
      return reply.code(404).send({ error: "not_found" });
    }
    if (!timeline) return reply.code(404).send({ error: "not_found" });
    return timeline;
  });

  // SPEC-012 — the on-demand forensic snapshot export (escalón 3): the incident's
  // canonicalized evidence + the SHA-256 hash-chain head chain_N + the Ed25519
  // root_signature over chain_N + the embedded forensic public key. Same session
  // preHandler + org-scope as the drill; GET → read-only (no role gate, no CSRF).
  // 404 mirrors the detail/events handlers (malformed :id / not-found / cross-org).
  // The export shape carries NO private-key field, so this route can never serialize
  // the forensic private key. The embedded pubkey is a convenience, NOT a trust root
  // (SPEC-012 §Compliance line 108) — out-of-band anchoring is the deferred Open-Q.
  app.get(
    "/v1/incidents/:id/forensic-export",
    { preHandler: requireSession },
    async (req, reply) => {
      const session = req.authSession;
      if (!session) throw new Error("unreachable: requireSession populates authSession");
      const { id } = req.params as { id: string };
      let exportDoc: Awaited<ReturnType<typeof buildForensicExport>>;
      try {
        exportDoc = await buildForensicExport(services, session.org_id, id);
      } catch {
        // Malformed id (e.g. not a UUID) → generic 404, no internal detail.
        return reply.code(404).send({ error: "not_found" });
      }
      if (!exportDoc) return reply.code(404).send({ error: "not_found" });
      return exportDoc;
    },
  );

  app.get("/v1/alerts", { preHandler: requireSession }, async (req) => {
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
