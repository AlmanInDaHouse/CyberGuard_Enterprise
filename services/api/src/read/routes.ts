import type { FastifyInstance } from "fastify";
import { makeRequireSession } from "../auth/prehandlers.js";
import { NotImplementedError } from "../errors.js";
import type { Services } from "../services.js";

/**
 * SPEC-009 §Operational §1/§2 — the read-API surface. Every read sits behind
 * SPEC-008's session preHandler (`makeRequireSession`): an unauthenticated/revoked
 * read is rejected with 401 BEFORE the handler (the enforcement point reused from
 * 008). Role + org come from the session, never the request (threat model :84;
 * :103 restates it platform-wide). All three roles read in the MVP (§Operational
 * §2), so no `makeRequireRole` gate restricts these routes — that primitive is
 * reused only WHERE a future capability restricts a read.
 *
 * Handlers are RED-gate stubs (NotImplementedError → 501). The GREEN gate projects
 * the read-models, org-scoped, from incidents/alerts.
 */
export function registerReadRoutes(app: FastifyInstance, services: Services): void {
  const requireSession = makeRequireSession(services);

  app.get("/v1/incidents", { preHandler: requireSession }, async () => {
    throw new NotImplementedError("read.incidents.list");
  });

  app.get("/v1/incidents/:id", { preHandler: requireSession }, async () => {
    throw new NotImplementedError("read.incidents.detail");
  });

  app.get("/v1/alerts", { preHandler: requireSession }, async () => {
    throw new NotImplementedError("read.alerts.list");
  });
}
