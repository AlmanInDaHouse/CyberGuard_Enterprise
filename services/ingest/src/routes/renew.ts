import type { FastifyInstance } from "fastify";
import { authenticateAgentRequest, serialOf } from "../agent-request.js";
import { clientCertValidity, issueClientCert } from "../cert.js";
import { verifyEnvelopeSignature } from "../jcs.js";
import { RenewalLimiter } from "../rate-limit.js";
import { RenewRequestSchema } from "../schemas.js";
import type { Services } from "../services.js";

const RENEW_ENVELOPE_VERSION = "0.1.0";
/** A certificate is renewed with this much left, or less (ADR-0004 Amendment 2026-11-20, item 4). */
export const RENEWAL_WINDOW_MS = 45 * 24 * 60 * 60 * 1000;

/**
 * `POST /v1/agents/renew` (mTLS), SPEC-025 §Operational §4. After §2's and §3's checks, the
 * timestamp, the nonce and the current key's signature: the new key's signature, the
 * certificate's due date and the CA's end, the rate limit; then the CA signs a certificate
 * for the new key, stored as the agent's pending one and answered. The agent's certificate
 * changes at its first accepted request under the new one (§3); a renewal under the pending
 * certificate promotes it in the statement that stores the new one.
 */
export function registerRenewRoutes(app: FastifyInstance, services: Services): void {
  const { pool, ca } = services;
  const limiter = new RenewalLimiter();

  app.post("/v1/agents/renew", async (req, reply) => {
    const parsed = RenewRequestSchema.safeParse(req.body);
    if (!parsed.success) {
      req.log.warn({ reason: "invalid_request", response_status: 400 }, "renewal rejected");
      return reply.code(400).send({ error: "invalid_request" });
    }
    const env = parsed.data;
    const refuse = (status: number, error: string) => {
      req.log.warn(
        { agent_id: env.agent_id, reason: error, response_status: status },
        "renewal rejected",
      );
      return reply.code(status).send({ error });
    };

    // Both signatures sign JCS of the envelope without either (§Data contracts §2).
    const { new_key_signature: _newKeySignature, ...signed } = req.body as Record<string, unknown>;
    const checked = await authenticateAgentRequest({
      req,
      reply,
      services,
      agentId: env.agent_id,
      sentAt: env.sent_at,
      nonce: env.nonce,
      signature: env.signature,
      signed,
      rejected: "renewal rejected",
    });
    if (checked === null) return reply;
    const { row, presented } = checked;

    const newKey = new Uint8Array(Buffer.from(env.body.new_agent_pubkey, "base64url"));
    if (!(await verifyEnvelopeSignature(signed, env.new_key_signature, newKey))) {
      return refuse(401, "bad_new_key_signature");
    }

    const nowMs = Date.now();
    const next = clientCertValidity(ca, nowMs);
    if (
      presented.notAfter.getTime() - nowMs > RENEWAL_WINDOW_MS ||
      next.notAfter.getTime() <= presented.notAfter.getTime()
    ) {
      return refuse(409, "renewal_not_due");
    }
    if (!limiter.take(env.agent_id)) {
      return refuse(429, "rate_limited");
    }

    const issued = await issueClientCert(ca, env.agent_id, newKey, nowMs);
    let stored = false;
    try {
      const result =
        presented.kind === "current"
          ? await pool.query(
              `UPDATE agents SET pending_cert_pem = $2
                WHERE agent_id = $1 AND cert_pem = $3 AND revoked_at IS NULL`,
              [env.agent_id, issued.certPem, row.cert_pem],
            )
          : await pool.query(
              `UPDATE agents
                  SET pubkey = $2, cert_pem = $3, expires_at = $4, pending_cert_pem = $5
                WHERE agent_id = $1 AND cert_pem = $6 AND pending_cert_pem = $3
                  AND revoked_at IS NULL`,
              [
                env.agent_id,
                Buffer.from(presented.key),
                presented.certPem,
                presented.notAfter,
                issued.certPem,
                row.cert_pem,
              ],
            );
      stored = result.rowCount === 1;
    } catch (err) {
      req.log.error(
        { agent_id: env.agent_id, error: String(err) },
        "pending certificate not stored",
      );
    }
    if (!stored) {
      // No certificate is sent that the server did not record (§4).
      return refuse(503, "unavailable");
    }
    if (presented.kind === "pending") {
      req.log.info(
        {
          agent_id: env.agent_id,
          old_serial: serialOf(row.cert_pem),
          new_serial: serialOf(presented.certPem),
        },
        "agent certificate promoted",
      );
    }
    req.log.info(
      {
        agent_id: env.agent_id,
        serial: serialOf(issued.certPem),
        expires_at: issued.notAfter.toISOString(),
        response_status: 200,
      },
      "renewal issued",
    );
    return reply.code(200).send({
      envelope_version: RENEW_ENVELOPE_VERSION,
      agent_id: env.agent_id,
      client_certificate: issued.certPem,
      issued_at: issued.notBefore.toISOString(),
      expires_at: issued.notAfter.toISOString(),
    });
  });
}
