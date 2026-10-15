import type { FastifyRequest } from "fastify";
import type { RequestContext } from "./service.js";
import { type TrustedProxies, sourceAddress } from "./source-address.js";

/**
 * A request's id, source address and session, for the limits and the audit rows
 * (SPEC-008 Amendment 2026-10-15 item 6; its second Amendment 2026-10-15, the source
 * address behind a proxy, items 2 and 3).
 */
export function requestContext(
  req: FastifyRequest,
  trusted: TrustedProxies | null,
): RequestContext {
  return {
    correlationId: req.id,
    sourceIp: sourceAddress(req.ip, req.headers["x-forwarded-for"], trusted),
    session: req.authSession,
  };
}
