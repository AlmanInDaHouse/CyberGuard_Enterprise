import "reflect-metadata";
import { X509Certificate as NodeX509Certificate, type webcrypto } from "node:crypto";
import { mkdtempSync } from "node:fs";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import * as x509 from "@peculiar/x509";
import type { FastifyInstance } from "fastify";
import pg from "pg";
import { buildEnrollApp, buildHeartbeatApp } from "../../src/app.js";
import type { ServerCa } from "../../src/ca.js";
import { ensureServerCert } from "../../src/cert.js";
import type { Config } from "../../src/config.js";
import { canonicalize } from "../../src/jcs.js";
import { TLS_CIPHER_SUITES } from "../../src/server.js";
import { type Services, buildServices } from "../../src/services.js";
import {
  type AgentIdentity,
  type HeartbeatResult,
  generateKeyPair,
  postMtls,
  rawPubkeyB64url,
} from "./test-client.js";

// SPEC-025's server half: the listeners as startIngest builds them, with their log lines
// captured; the certificates a test signs with the CA's key; the renewal request, signed by
// both keys (§Data contracts §2); and the agent's row as the server keeps it.

const subtle = globalThis.crypto.subtle;
const ED = { name: "Ed25519" } as const;
export const DAY_MS = 24 * 60 * 60 * 1000;
x509.cryptoProvider.set(globalThis.crypto);

/** One captured log line of the apps (pino's JSON). */
export type LogLine = Record<string, unknown> & { msg?: string };

export interface RenewHarness {
  enrollUrl: string;
  heartbeatUrl: string;
  caCertPem: string;
  ca: ServerCa;
  services: Services;
  enrollApp: FastifyInstance;
  /** Every line both apps logged, at info and above. */
  logs: LogLine[];
  close(): Promise<void>;
}

/**
 * Both listeners as `startIngest` builds them (TLS 1.3, ADR-0004's suites, the mTLS
 * listener asking for a client certificate the CA signed), with no detection driver, and
 * with their log lines at info captured. Enrollment in clear, as the suites post it.
 */
export async function startRenewHarness(config: Config): Promise<RenewHarness> {
  const services = await buildServices(config);
  const logs: LogLine[] = [];
  const logger = {
    level: "info",
    stream: {
      write(line: string) {
        logs.push(JSON.parse(line) as LogLine);
      },
    },
  };
  const dir = mkdtempSync(join(tmpdir(), "cg-renew-"));
  const identity = await ensureServerCert(
    services.ca,
    join(dir, "server.pem"),
    join(dir, "server-key.pem"),
    { now: Date.now, log: { info() {}, warn() {}, error() {} }, names: ["localhost", "127.0.0.1"] },
  );
  const enrollApp = buildEnrollApp(services, logger);
  const heartbeatApp = buildHeartbeatApp(
    services,
    {
      ca: services.ca.caCertPem,
      minVersion: "TLSv1.3",
      ciphers: TLS_CIPHER_SUITES,
      key: identity.keyPem,
      cert: identity.certPem,
      requestCert: true,
      rejectUnauthorized: true,
    },
    logger,
  );
  await enrollApp.listen({ host: "127.0.0.1", port: 0 });
  await heartbeatApp.listen({ host: "127.0.0.1", port: 0 });
  return {
    enrollUrl: `http://127.0.0.1:${(enrollApp.server.address() as AddressInfo).port}`,
    heartbeatUrl: `https://127.0.0.1:${(heartbeatApp.server.address() as AddressInfo).port}`,
    caCertPem: services.ca.caCertPem,
    ca: services.ca,
    services,
    enrollApp,
    logs,
    async close() {
      await Promise.allSettled([enrollApp.close(), heartbeatApp.close()]);
      await services.close();
    },
  };
}

function pemWrap(der: ArrayBuffer, label: string): string {
  const lines =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  return `-----BEGIN ${label}-----\n${lines.join("\n")}\n-----END ${label}-----\n`;
}

/** The identity's agent with another pair: `certPem` for `keyPair`. */
export async function withPair(
  identity: AgentIdentity,
  certPem: string,
  keyPair: webcrypto.CryptoKeyPair,
): Promise<AgentIdentity> {
  return {
    agentId: identity.agentId,
    keyPair,
    certPem,
    keyPkcs8Pem: pemWrap(await subtle.exportKey("pkcs8", keyPair.privateKey), "PRIVATE KEY"),
  };
}

/**
 * A client certificate as the CA would issue one, signed with the CA's key by the test:
 * `CN=agentId`, the key pair's public key, the validity given.
 */
export async function signAgentCert(
  ca: ServerCa,
  agentId: string,
  keyPair: webcrypto.CryptoKeyPair,
  validity: { notBefore: Date; notAfter: Date },
): Promise<string> {
  const cert = await x509.X509CertificateGenerator.create(
    {
      serialNumber: Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(16))).toString(
        "hex",
      ),
      subject: `CN=${agentId}`,
      issuer: ca.caCert.subject,
      notBefore: validity.notBefore,
      notAfter: validity.notAfter,
      signingAlgorithm: ED,
      publicKey: keyPair.publicKey,
      signingKey: ca.caPrivateKey,
      extensions: [new x509.KeyUsagesExtension(x509.KeyUsageFlags.digitalSignature, true)],
    },
    globalThis.crypto,
  );
  return cert.toString("pem");
}

/** The raw 32-byte public key of a key pair. */
export async function rawPubkey(keyPair: webcrypto.CryptoKeyPair): Promise<Buffer> {
  return Buffer.from(await subtle.exportKey("raw", keyPair.publicKey));
}

/**
 * Give the agent, as the owner role would, a certificate the test signed for its own key
 * with `daysLeft` days left: the row's `pubkey`, `cert_pem` and `expires_at` become it.
 * Returns the identity that presents it.
 */
export async function ageAgentCert(
  config: Config,
  ca: ServerCa,
  identity: AgentIdentity,
  daysLeft: number,
): Promise<AgentIdentity> {
  const notAfter = new Date(Date.now() + daysLeft * DAY_MS);
  const certPem = await signAgentCert(ca, identity.agentId, identity.keyPair, {
    notBefore: new Date(Date.now() - 60_000),
    notAfter,
  });
  await withPool(config, async (pool) =>
    pool.query(
      "UPDATE agents SET pubkey = $2, cert_pem = $3, expires_at = $4 WHERE agent_id = $1",
      [identity.agentId, await rawPubkey(identity.keyPair), certPem, notAfter],
    ),
  );
  return { ...identity, certPem };
}

export interface RenewOptions {
  nonceB64url?: string;
  sentAt?: string;
  /** Sign `new_key_signature` with another key than the new one. */
  badNewKeySignature?: boolean;
  /** Sign `signature` with this key instead of the identity's. */
  signingKey?: webcrypto.CryptoKeyPair;
}

/**
 * The renewal request (SPEC-025 §Data contracts §2): SPEC-003's outer envelope with the
 * body `{ renew_version, new_agent_pubkey }`, `signature` by the current key and
 * `new_key_signature` by the new one, both over JCS of the envelope without either.
 */
export async function buildRenewRequest(
  identity: AgentIdentity,
  newKeyPair: webcrypto.CryptoKeyPair,
  opts: RenewOptions = {},
): Promise<Record<string, unknown>> {
  const signedRegion = {
    outer_envelope_version: "0.1.0",
    agent_id: identity.agentId,
    sequence_number: 1,
    nonce:
      opts.nonceB64url ??
      Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(16))).toString("base64url"),
    sent_at: opts.sentAt ?? new Date().toISOString(),
    body: { renew_version: "1", new_agent_pubkey: await rawPubkeyB64url(newKeyPair) },
  };
  const canonical = canonicalize(signedRegion);
  if (canonical === undefined) throw new Error("canonicalize returned undefined");
  const bytes = new TextEncoder().encode(canonical);
  const signer = opts.signingKey ?? identity.keyPair;
  const newSigner = opts.badNewKeySignature ? await generateKeyPair() : newKeyPair;
  const signature = Buffer.from(await subtle.sign(ED, signer.privateKey, bytes)).toString(
    "base64url",
  );
  const newKeySignature = Buffer.from(await subtle.sign(ED, newSigner.privateKey, bytes)).toString(
    "base64url",
  );
  return { ...signedRegion, signature, new_key_signature: newKeySignature };
}

/** mTLS POST of a renewal request to `/v1/agents/renew` under the identity's pair. */
export function postRenew(
  heartbeatUrl: string,
  args: { caCertPem: string; identity: AgentIdentity; envelope: Record<string, unknown> },
): Promise<HeartbeatResult> {
  return postMtls(heartbeatUrl, "/v1/agents/renew", args);
}

/** The renewal's answer, parsed (SPEC-002's enrollment answer). */
export interface RenewAnswer {
  envelope_version: string;
  agent_id: string;
  client_certificate: string;
  issued_at: string;
  expires_at: string;
}

/** The `error` a refusal's body names. */
export function errorOf(res: HeartbeatResult): string | undefined {
  try {
    return (JSON.parse(res.bodyText) as { error?: string }).error;
  } catch {
    return undefined;
  }
}

/** Run `fn` with a pool on the config's Postgres, ended after it. */
export async function withPool<T>(config: Config, fn: (pool: pg.Pool) => Promise<T>): Promise<T> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  pool.on("error", () => {});
  try {
    return await fn(pool);
  } finally {
    await pool.end();
  }
}

/** Every column of the agent's row, or null. */
export async function agentRow(
  config: Config,
  agentId: string,
): Promise<Record<string, unknown> | null> {
  return withPool(config, async (pool) => {
    const r = await pool.query("SELECT * FROM agents WHERE agent_id = $1", [agentId]);
    return (r.rows[0] as Record<string, unknown> | undefined) ?? null;
  });
}

/** The DER of a PEM certificate. */
export function derOf(certPem: string): Buffer {
  return Buffer.from(new NodeX509Certificate(certPem).raw);
}

/** A certificate's serial, as Node prints it. */
export function serialOf(certPem: string): string {
  return new NodeX509Certificate(certPem).serialNumber;
}

/** A Process Activity element (SPEC-005) with the event id given. */
export function processEvent(eventId: string): Record<string, unknown> {
  return {
    event_id: eventId,
    class_uid: 1007,
    activity_id: 1,
    process: {
      pid: 4242,
      uid: `renew-${eventId}`,
      name: "renew.exe",
      created_time: null,
      parent_pid: null,
      command_line: "",
      subject_user_sid: "",
      image_file_name: "C:\\renew.exe",
    },
    time: String(BigInt(Date.now()) * 1_000_000n),
  };
}
