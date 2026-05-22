import type { webcrypto } from "node:crypto";
import { request as httpsRequest } from "node:https";
import { createRequire } from "node:module";

// A TypeScript stand-in for the cg-agent's crypto path, used to drive the
// server's rejection paths (AC-002 replay, AC-003 skew, AC-004 bad
// signature, AC-005 agent_id/CN mismatch) that the well-behaved real
// agent never produces. Native WebCrypto Ed25519 + canonicalize (RFC 8785)
// + Node mTLS, matching the agent and the server.

// canonicalize is CJS; load via createRequire for clean ESM interop.
const canonicalize = createRequire(import.meta.url)("canonicalize") as (
  value: unknown,
) => string | undefined;

const subtle = globalThis.crypto.subtle;
const ED = { name: "Ed25519" } as const;

export interface AgentIdentity {
  keyPair: webcrypto.CryptoKeyPair;
  agentId: string;
  certPem: string;
  keyPkcs8Pem: string;
}

export class EnrollError extends Error {
  constructor(
    readonly status: number,
    readonly bodyText: string,
  ) {
    super(`enroll failed: ${status}`);
  }
}

export interface EnvelopeOptions {
  sequenceNumber?: number;
  /** Override the nonce — same value twice exercises replay (AC-002). */
  nonceB64url?: string;
  /** Override sent_at — e.g. 6 min in the past exercises skew (AC-003). */
  sentAt?: string;
  /** Override the envelope agent_id to mismatch the cert CN (AC-005). */
  agentIdOverride?: string;
  status?: "online" | "going_offline";
}

export interface HeartbeatResult {
  status: number;
  bodyText: string;
}

function b64url(buf: ArrayBuffer | Uint8Array): string {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  return Buffer.from(bytes).toString("base64url");
}

function pemWrap(der: ArrayBuffer, label: string): string {
  const lines =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  return `-----BEGIN ${label}-----\n${lines.join("\n")}\n-----END ${label}-----\n`;
}

export async function generateKeyPair(): Promise<webcrypto.CryptoKeyPair> {
  return (await subtle.generateKey(ED, true, ["sign", "verify"])) as webcrypto.CryptoKeyPair;
}

export async function rawPubkeyB64url(keyPair: webcrypto.CryptoKeyPair): Promise<string> {
  return b64url(await subtle.exportKey("raw", keyPair.publicKey));
}

/** POST the SPEC-002 enrollment request (plain HTTP). Throws EnrollError on non-2xx. */
export async function enroll(
  enrollUrl: string,
  token: string,
  opts: { hostname?: string; keyPair?: webcrypto.CryptoKeyPair } = {},
): Promise<AgentIdentity> {
  const keyPair = opts.keyPair ?? (await generateKeyPair());
  const res = await fetch(`${enrollUrl}/v1/agents/enroll`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      envelope_version: "0.1.0",
      enrollment_token: token,
      agent_pubkey: await rawPubkeyB64url(keyPair),
      agent_hostname: opts.hostname ?? "TEST-PC",
      agent_platform: "linux",
      agent_version: "0.1.0",
    }),
  });
  if (!res.ok) {
    throw new EnrollError(res.status, await res.text());
  }
  const body = (await res.json()) as { agent_id: string; client_certificate: string };
  return {
    keyPair,
    agentId: body.agent_id,
    certPem: body.client_certificate,
    keyPkcs8Pem: pemWrap(await subtle.exportKey("pkcs8", keyPair.privateKey), "PRIVATE KEY"),
  };
}

/** Build + sign the SPEC-003 outer envelope (signature over JCS(envelope minus signature)). */
export async function buildSignedEnvelope(
  identity: AgentIdentity,
  opts: EnvelopeOptions = {},
): Promise<Record<string, unknown>> {
  const sequence = opts.sequenceNumber ?? 1;
  const nonce = opts.nonceB64url ?? b64url(globalThis.crypto.getRandomValues(new Uint8Array(16)));
  const sentAt = opts.sentAt ?? new Date().toISOString();
  const agentId = opts.agentIdOverride ?? identity.agentId;
  const signedRegion = {
    outer_envelope_version: "0.1.0",
    agent_id: agentId,
    sequence_number: sequence,
    nonce,
    sent_at: sentAt,
    body: {
      envelope_version: "0.1.0",
      agent: {
        agent_id: agentId,
        agent_version: "0.1.0",
        agent_platform: "linux",
        agent_hostname: "TEST-PC",
      },
      sequence_number: sequence,
      sent_at: sentAt,
      status: opts.status ?? "online",
      uptime_seconds: 0,
    },
  };
  const canonical = canonicalize(signedRegion);
  if (canonical === undefined) {
    throw new Error("canonicalize returned undefined");
  }
  const sig = await subtle.sign(
    ED,
    identity.keyPair.privateKey,
    new TextEncoder().encode(canonical),
  );
  return { ...signedRegion, signature: b64url(sig) };
}

/** mTLS POST of an outer envelope to `/v1/agents/heartbeat`. */
export function postHeartbeat(
  heartbeatUrl: string,
  args: {
    caCertPem: string;
    identity: AgentIdentity;
    envelope: Record<string, unknown>;
    tamperSignature?: boolean;
  },
): Promise<HeartbeatResult> {
  const url = new URL(`${heartbeatUrl}/v1/agents/heartbeat`);
  const envelope = args.tamperSignature
    ? { ...args.envelope, signature: flipFirstChar(String(args.envelope.signature)) }
    : args.envelope;
  const payload = Buffer.from(JSON.stringify(envelope));
  return new Promise<HeartbeatResult>((resolve, reject) => {
    const req = httpsRequest(
      {
        host: url.hostname,
        port: Number(url.port),
        path: url.pathname,
        method: "POST",
        ca: args.caCertPem,
        cert: args.identity.certPem,
        key: args.identity.keyPkcs8Pem,
        rejectUnauthorized: true,
        headers: { "content-type": "application/json", "content-length": payload.length },
      },
      (res) => {
        const chunks: Buffer[] = [];
        res.on("data", (c: Buffer) => chunks.push(c));
        res.on("end", () =>
          resolve({
            status: res.statusCode ?? 0,
            bodyText: Buffer.concat(chunks).toString("utf8"),
          }),
        );
      },
    );
    req.on("error", reject);
    req.end(payload);
  });
}

function flipFirstChar(s: string): string {
  if (s.length === 0) {
    return "A";
  }
  return (s[0] === "A" ? "B" : "A") + s.slice(1);
}
