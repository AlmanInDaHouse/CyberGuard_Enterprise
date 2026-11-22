import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import { v7 as uuidv7 } from "uuid";
import { cutAgentText } from "../agent-request.js";
import { issueClientCert } from "../cert.js";
import { EnrollmentLimiter } from "../rate-limit.js";
import { EnrollRequestSchema } from "../schemas.js";
import type { Services } from "../services.js";
import { tokenSha256 } from "../token.js";

const ENROLL_ENVELOPE_VERSION = "0.1.0";
/** The MVP's one org (SPEC-025 §Operational §1). */
const MVP_ORG = "default";

/** Connection-level failure (PG down) ⇒ 503; a real query/logic error ⇒ 500. */
function isConnectivityError(e: unknown): boolean {
  const code = (e as { code?: string }).code;
  return (
    code === "ECONNREFUSED" ||
    code === "ETIMEDOUT" ||
    code === "ENOTFOUND" ||
    code === "57P01" || // admin_shutdown
    code === "08006" || // connection_failure
    code === "08001" || // sqlclient_unable_to_establish_sqlconnection
    /terminat|connection|ECONN/i.test(String((e as { message?: string }).message ?? ""))
  );
}

interface TokenRow {
  token_id: string;
  org_id: string;
  scope: string;
  uses: number;
  max_uses: number;
  withdrawn: boolean;
  live: boolean;
}

/**
 * `POST /v1/agents/enroll` + `GET /health`, on the enroll listener (over TLS unless
 * INGEST_ENROLL_TLS=false, SPEC-023 §Operational §6). SPEC-004 FR-003–FR-007 as SPEC-025
 * §Operational §1 amends them: in one transaction, the token's row is found by its SHA-256
 * and locked, and checked before the CA signs anything; then the agent is inserted with the
 * token's org and the token's use counted. An address whose enrollments failed ten times in
 * 15 minutes is refused before its token is read (§Operational §5).
 */
export function registerEnrollRoutes(app: FastifyInstance, services: Services): void {
  const { pool, ca } = services;
  const limiter = new EnrollmentLimiter();

  app.get("/health", async () => ({ status: "ok" }));

  /** A refusal for the token: counted as a failed enrollment of the address. */
  function refuseToken(
    req: FastifyRequest,
    reply: FastifyReply,
    status: 401 | 409,
    error: "token_rejected" | "token_already_used",
  ) {
    limiter.recordFailure(req.ip);
    req.log.warn({ reason: error, response_status: status }, "enroll rejected");
    return reply.code(status).send({ error });
  }

  app.post("/v1/agents/enroll", async (req, reply) => {
    const parsed = EnrollRequestSchema.safeParse(req.body);
    if (!parsed.success) {
      req.log.warn({ reason: "invalid_request", response_status: 400 }, "enroll rejected");
      return reply.code(400).send({ error: "invalid_request" });
    }
    if (limiter.refuses(req.ip)) {
      req.log.warn({ reason: "rate_limited", response_status: 429 }, "enroll rejected");
      return reply.code(429).send({ error: "rate_limited" });
    }
    const body = parsed.data;
    const rawPubkey = Buffer.from(body.agent_pubkey, "base64url");

    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      const found = await client.query<TokenRow>(
        `SELECT token_id, org_id, scope, uses, max_uses,
                withdrawn_at IS NOT NULL AS withdrawn, expires_at > now() AS live
           FROM enrollment_tokens WHERE token_sha256 = $1 FOR UPDATE`,
        [tokenSha256(body.enrollment_token)],
      );
      const token = found.rows[0];
      // In SPEC-025 §Operational §1's order, the first failure refusing; nothing is signed.
      const refusal: [401 | 409, "token_rejected" | "token_already_used"] | null =
        !token || token.scope !== "enroll" || token.org_id !== MVP_ORG
          ? [401, "token_rejected"]
          : token.uses >= token.max_uses
            ? [409, "token_already_used"]
            : token.withdrawn || !token.live
              ? [401, "token_rejected"]
              : null;
      if (refusal || !token) {
        await client.query("ROLLBACK");
        const [status, error] = refusal ?? [401, "token_rejected"];
        return refuseToken(req, reply, status, error);
      }

      const agentId = uuidv7();
      const issued = await issueClientCert(ca, agentId, new Uint8Array(rawPubkey));
      await client.query(
        `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, enrolled_at, expires_at,
                             enrolled_by_token_id, hostname, platform, agent_version)
         VALUES ($1, $2, $3, $4, now(), $5, $6, $7, $8, $9)`,
        [
          agentId,
          token.org_id,
          rawPubkey,
          issued.certPem,
          issued.notAfter,
          token.token_id,
          cutAgentText(body.agent_hostname),
          cutAgentText(body.agent_platform),
          cutAgentText(body.agent_version),
        ],
      );
      await client.query(
        `UPDATE enrollment_tokens
            SET uses = uses + 1, consumed_at = now(),
                state = CASE WHEN uses + 1 >= max_uses THEN 'consumed'::token_state ELSE state END
          WHERE token_id = $1`,
        [token.token_id],
      );
      await client.query("COMMIT");
      req.log.info(
        { agent_id: agentId, org_id: token.org_id, expires_at: issued.notAfter.toISOString() },
        "enroll succeeded",
      );
      return reply.code(200).send({
        envelope_version: ENROLL_ENVELOPE_VERSION,
        agent_id: agentId,
        client_certificate: issued.certPem,
        issued_at: issued.notBefore.toISOString(),
        expires_at: issued.notAfter.toISOString(),
      });
    } catch (err) {
      await client.query("ROLLBACK").catch(() => {});
      if (isConnectivityError(err)) {
        req.log.error({ backend: "postgres", error: String(err) }, "backend unavailable");
        return reply.code(503).send({ error: "unavailable" });
      }
      req.log.error({ error: String(err) }, "enroll internal error");
      return reply.code(500).send({ error: "internal" });
    } finally {
      client.release();
    }
  });
}
