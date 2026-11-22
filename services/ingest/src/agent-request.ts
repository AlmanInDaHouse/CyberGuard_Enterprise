import { X509Certificate } from "node:crypto";
import type { TLSSocket } from "node:tls";
import type { FastifyBaseLogger, FastifyReply, FastifyRequest } from "fastify";
import type pg from "pg";
import { verifyEnvelopeSignature } from "./jcs.js";
import type { Services } from "./services.js";

/**
 * SPEC-025 §Operational §2 and §3 — what every request of an agent passes, on the heartbeat
 * and on the renewal, after the envelope's structure: the client certificate's `CN` equal to
 * `agent_id`, the agent's row read once, its revocation, the certificate it presents compared
 * with the agent's and its pending one, and then SPEC-004 FR-010's timestamp, nonce and
 * signature, under the key the certificate check gave. And the two signals of a copied
 * identity (§6).
 */

/** ±5 min skew window (SPEC-004 FR-010 step 3). */
const SKEW_MS = 5 * 60 * 1000;
/** Nonce TTL: 10 min, comfortably above the skew window (FR-011). */
const NONCE_TTL_SECONDS = 600;
/** What the agent sends is kept cut to this many code points (SPEC-025 §Data contracts §1). */
export const AGENT_TEXT_MAX = 255;
/** Two process starts this close are one process (SPEC-025 §Operational §6). */
export const PROCESS_START_WINDOW_MS = 120_000;

/** `text` cut to its first 255 code points. */
export function cutAgentText(text: string): string {
  const points = [...text];
  return points.length <= AGENT_TEXT_MAX ? text : points.slice(0, AGENT_TEXT_MAX).join("");
}

/** The agent's row as a request reads it (§Operational §2). */
export interface AgentRequestRow {
  pubkey: Buffer;
  cert_pem: string;
  pending_cert_pem: string | null;
  org_id: string;
  revoked_at: Date | null;
  process_started_at: Date | null;
  previous_process_started_at: Date | null;
  duplicate_signal: string | null;
}

/** The certificate a request presented, and the key its envelope is verified under. */
export type Presented =
  | { kind: "current"; key: Uint8Array; certPem: string; notAfter: Date }
  | { kind: "pending"; key: Uint8Array; certPem: string; notAfter: Date };

export interface AuthenticatedAgentRequest {
  row: AgentRequestRow;
  presented: Presented;
}

/** The raw 32-byte Ed25519 public key a certificate carries. */
export function certificateRawKey(cert: X509Certificate): Buffer {
  const jwk = cert.publicKey.export({ format: "jwk" }) as { crv?: string; x?: string };
  if (jwk.crv !== "Ed25519" || typeof jwk.x !== "string") {
    throw new Error("the certificate's key is not Ed25519");
  }
  return Buffer.from(jwk.x, "base64url");
}

/** The DER of a stored PEM certificate; null when it does not parse. */
function storedDer(pem: string | null): Buffer | null {
  if (pem === null) return null;
  try {
    return Buffer.from(new X509Certificate(pem).raw);
  } catch {
    return null;
  }
}

/** The CN of the validated client certificate (mTLS already passed). */
function peerCertificateCn(socket: TLSSocket): string | null {
  if (typeof socket.getPeerCertificate !== "function") return null;
  const cn = socket.getPeerCertificate()?.subject?.CN;
  return typeof cn === "string" && cn.length > 0 ? cn : null;
}

/** The DER of the certificate the TLS socket presented. */
function peerCertificateDer(socket: TLSSocket): Buffer | null {
  if (typeof socket.getPeerCertificate !== "function") return null;
  const raw = socket.getPeerCertificate()?.raw;
  return Buffer.isBuffer(raw) && raw.length > 0 ? raw : null;
}

/** A certificate's serial, as Node prints it. */
export function serialOf(certPem: string): string {
  return new X509Certificate(certPem).serialNumber;
}

/**
 * The first signal of a copy sets `duplicate_signal` and its time, and is logged; a later
 * one changes neither (§Operational §6). A failure to record it is logged and refuses
 * nothing.
 */
export async function raiseCopySignal(
  pool: pg.Pool,
  log: FastifyBaseLogger,
  agentId: string,
  signal: "superseded_certificate" | "interleaved_process",
): Promise<void> {
  try {
    const raised = await pool.query(
      `UPDATE agents SET duplicate_signal = $2, duplicate_signal_at = now()
        WHERE agent_id = $1 AND duplicate_signal IS NULL`,
      [agentId, signal],
    );
    if (raised.rowCount === 1) {
      log.warn({ agent_id: agentId, signal }, "agent duplicate suspected");
    }
  } catch (err) {
    log.error({ agent_id: agentId, signal, error: String(err) }, "duplicate signal not recorded");
  }
}

/**
 * The process columns after an accepted heartbeat whose process started at `startMs`, and
 * whether it raises `interleaved_process` (§Operational §6): the first start is taken; one
 * within 120 s of the last changes nothing; any other shifts the columns, and raises the
 * signal when it is within 120 s of the one before the last.
 */
export function nextProcessStarts(
  row: Pick<AgentRequestRow, "process_started_at" | "previous_process_started_at">,
  startMs: number,
): { processStartedAt: Date; previousProcessStartedAt: Date | null; interleaved: boolean } {
  const last = row.process_started_at;
  const before = row.previous_process_started_at;
  if (last === null) {
    return {
      processStartedAt: new Date(startMs),
      previousProcessStartedAt: before,
      interleaved: false,
    };
  }
  if (Math.abs(startMs - last.getTime()) <= PROCESS_START_WINDOW_MS) {
    return { processStartedAt: last, previousProcessStartedAt: before, interleaved: false };
  }
  return {
    processStartedAt: new Date(startMs),
    previousProcessStartedAt: last,
    interleaved: before !== null && Math.abs(startMs - before.getTime()) <= PROCESS_START_WINDOW_MS,
  };
}

/**
 * Run §Operational §2's checks on a request of an agent, in its order, before the nonce is
 * spent: the `CN` (`401 unknown_agent`), the row (`401 unknown_agent`), the revocation (`403
 * agent_revoked`), the certificate (`403 agent_superseded`, with the copy signal); then the
 * timestamp (`422`), the nonce (`409`) and the signature over `signed` (`401`). Sends the
 * refusal and returns null on the first failure.
 */
export async function authenticateAgentRequest(args: {
  req: FastifyRequest;
  reply: FastifyReply;
  services: Services;
  agentId: string;
  sentAt: string;
  nonce: string;
  signature: string;
  /** The envelope whose JCS, without `signature`, the signature signs. */
  signed: Record<string, unknown>;
  /** The log line of a refusal: "heartbeat rejected" or "renewal rejected". */
  rejected: string;
}): Promise<AuthenticatedAgentRequest | null> {
  const { req, reply, services, agentId, rejected } = args;
  const { pool, redis } = services;
  const refuse = (status: number, error: string) => {
    req.log.warn({ agent_id: agentId, reason: error, response_status: status }, rejected);
    return reply.code(status).send({ error });
  };

  const socket = req.raw.socket as TLSSocket;
  if (peerCertificateCn(socket) !== agentId) {
    await refuse(401, "unknown_agent");
    return null;
  }

  let row: AgentRequestRow | undefined;
  try {
    const found = await pool.query<AgentRequestRow>(
      `SELECT pubkey, cert_pem, pending_cert_pem, org_id, revoked_at, process_started_at,
              previous_process_started_at, duplicate_signal
         FROM agents WHERE agent_id = $1`,
      [agentId],
    );
    row = found.rows[0];
  } catch (err) {
    req.log.error({ backend: "postgres", error: String(err) }, "backend unavailable");
    await reply.code(503).send({ error: "unavailable" });
    return null;
  }
  if (!row) {
    await refuse(401, "unknown_agent");
    return null;
  }
  if (row.revoked_at !== null) {
    await refuse(403, "agent_revoked");
    return null;
  }

  // §3: the presented certificate by its DER, the agent's or its pending one, else neither.
  const der = peerCertificateDer(socket);
  let presented: Presented | null = null;
  if (der !== null) {
    if (storedDer(row.cert_pem)?.equals(der)) {
      presented = {
        kind: "current",
        key: new Uint8Array(row.pubkey),
        certPem: row.cert_pem,
        notAfter: new Date(new X509Certificate(row.cert_pem).validTo),
      };
    } else if (row.pending_cert_pem !== null && storedDer(row.pending_cert_pem)?.equals(der)) {
      const pending = new X509Certificate(row.pending_cert_pem);
      presented = {
        kind: "pending",
        key: new Uint8Array(certificateRawKey(pending)),
        certPem: row.pending_cert_pem,
        notAfter: new Date(pending.validTo),
      };
    }
  }
  if (presented === null) {
    await raiseCopySignal(pool, req.log, agentId, "superseded_certificate");
    await refuse(403, "agent_superseded");
    return null;
  }

  // SPEC-004 FR-010 step 3: timestamp window.
  const sentAtMs = new Date(args.sentAt).getTime();
  if (Number.isNaN(sentAtMs) || Math.abs(Date.now() - sentAtMs) > SKEW_MS) {
    await refuse(422, "stale_timestamp");
    return null;
  }

  // FR-010 step 4 / FR-011: atomic anti-replay. SET NX both checks and persists the nonce —
  // the first cross-store write (§Behavior order).
  let nonceWasNew: string | null;
  try {
    nonceWasNew = await redis.set(`nonce:${args.nonce}`, "1", "EX", NONCE_TTL_SECONDS, "NX");
  } catch (err) {
    req.log.error({ backend: "redis", error: String(err) }, "backend unavailable");
    await reply.code(503).send({ error: "unavailable" });
    return null;
  }
  if (nonceWasNew === null) {
    await refuse(409, "replay");
    return null;
  }

  // FR-010 step 5: the signature over JCS(envelope minus signature), against the raw received
  // body so the canonical bytes match the agent's, under the presented certificate's key.
  if (!(await verifyEnvelopeSignature(args.signed, args.signature, presented.key))) {
    await refuse(401, "bad_signature");
    return null;
  }
  return { row, presented };
}
