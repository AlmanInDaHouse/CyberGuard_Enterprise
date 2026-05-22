import type { FastifyInstance } from "fastify";
import { v7 as uuidv7 } from "uuid";
import { issueClientCert } from "../cert.js";
import { EnrollRequestSchema } from "../schemas.js";
import type { Services } from "../services.js";

const ENROLL_ENVELOPE_VERSION = "0.1.0";

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

/**
 * `POST /v1/agents/enroll` (plain HTTP) + `GET /health`. SPEC-004 FR-003–FR-007:
 * validate the single-use token, issue an Ed25519 client cert, persist the
 * agent — token consumption and the agents insert in one transaction.
 */
export function registerEnrollRoutes(app: FastifyInstance, services: Services): void {
  const { pool, ca } = services;

  app.get("/health", async () => ({ status: "ok" }));

  app.post("/v1/agents/enroll", async (req, reply) => {
    const parsed = EnrollRequestSchema.safeParse(req.body);
    if (!parsed.success) {
      req.log.warn({ reason: "invalid_request", response_status: 400 }, "enroll rejected");
      return reply.code(400).send({ error: "invalid_request" });
    }
    const body = parsed.data;
    const rawPubkey = Buffer.from(body.agent_pubkey, "base64url");
    const agentId = uuidv7();

    const client = await pool.connect();
    try {
      await client.query("BEGIN");

      const issued = await issueClientCert(ca, agentId, new Uint8Array(rawPubkey));

      await client.query(
        `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, enrolled_at, expires_at)
         VALUES ($1, 'default', $2, $3, now(), $4)`,
        [agentId, rawPubkey, issued.certPem, issued.notAfter],
      );

      const consume = await client.query(
        `UPDATE enrollment_tokens
            SET state = 'consumed', consumed_at = now(), consumed_by_agent_id = $2
          WHERE token = $1 AND state = 'issued' AND expires_at > now()
          RETURNING token`,
        [body.enrollment_token, agentId],
      );

      if (consume.rowCount === 0) {
        await client.query("ROLLBACK");
        const st = await pool.query<{ state: string }>(
          "SELECT state FROM enrollment_tokens WHERE token = $1",
          [body.enrollment_token],
        );
        if (st.rows[0]?.state === "consumed") {
          req.log.warn({ reason: "token_already_used", response_status: 409 }, "enroll rejected");
          return reply.code(409).send({ error: "token_already_used" });
        }
        req.log.warn({ reason: "token_rejected", response_status: 401 }, "enroll rejected");
        return reply.code(401).send({ error: "token_rejected" });
      }

      await client.query("COMMIT");
      req.log.info(
        { agent_id: agentId, org_id: "default", expires_at: issued.notAfter.toISOString() },
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
