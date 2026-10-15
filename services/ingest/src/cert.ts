import "reflect-metadata";
import {
  X509Certificate as NodeX509Certificate,
  createPrivateKey,
  type webcrypto,
} from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import * as x509 from "@peculiar/x509";
import type { ServerCa } from "./ca.js";

const webcryptoProvider = globalThis.crypto;
x509.cryptoProvider.set(webcryptoProvider);

const ALG = { name: "Ed25519" } as const;
const DAY_MS = 24 * 60 * 60 * 1000;
/** Client-cert TTL — 90 days per ADR-0004 / SPEC-004 FR-006. */
const CLIENT_VALIDITY_MS = 90 * DAY_MS;
/** The server's own certificate (SPEC-004 Amendment 2026-10-15): 90 days, reissued 30 days ahead. */
const SERVER_VALIDITY_MS = 90 * DAY_MS;
const SERVER_RENEW_BEFORE_MS = 30 * DAY_MS;
/**
 * The server certificate's `notBefore` lies an hour back: an agent whose clock lags
 * the server's by up to FR-010's 5 minutes must not see it as not yet valid (exit 6).
 */
const SERVER_BACKDATE_MS = 60 * 60 * 1000;
/** Item 7: connections are closed this long before the held certificate expires. */
export const SERVER_EXPIRY_MARGIN_MS = 10 * 60 * 1000;
/** How often a running server checks the certificate it serves. */
export const SERVER_CHECK_INTERVAL_MS = DAY_MS;
const SERVER_SUBJECT = "CN=localhost";
/** The self-issued SAN, so the agent's IP-based server-name check accepts it. */
const SERVER_SAN = [
  { type: "dns", value: "localhost" },
  { type: "ip", value: "127.0.0.1" },
] as const;
const EKU_SERVER_AUTH = "1.3.6.1.5.5.7.3.1";

function randomSerial(): string {
  return Buffer.from(webcryptoProvider.getRandomValues(new Uint8Array(16))).toString("hex");
}

export interface IssuedClientCert {
  certPem: string;
  notBefore: Date;
  notAfter: Date;
}

/**
 * Issue an Ed25519 X.509 client certificate (SPEC-004 FR-006): `CN=agentId`,
 * the agent's submitted 32-byte raw Ed25519 public key as the subject key,
 * signed by the CA's Ed25519 private key, 90-day TTL. `digitalSignature` key
 * usage only (no EKU — matches the verified FR-006 path; the server validates
 * the chain, not an EKU).
 */
export async function issueClientCert(
  ca: ServerCa,
  agentId: string,
  rawPubkey: Uint8Array,
): Promise<IssuedClientCert> {
  const subjectKey = (await webcryptoProvider.subtle.importKey("raw", rawPubkey, ALG, true, [
    "verify",
  ])) as webcrypto.CryptoKey;
  const notBefore = new Date(Date.now() - 60_000);
  const notAfter = new Date(Date.now() + CLIENT_VALIDITY_MS);
  const cert = await x509.X509CertificateGenerator.create(
    {
      serialNumber: randomSerial(),
      subject: `CN=${agentId}`,
      issuer: ca.caCert.subject,
      notBefore,
      notAfter,
      signingAlgorithm: ALG,
      publicKey: subjectKey,
      signingKey: ca.caPrivateKey,
      extensions: [new x509.KeyUsagesExtension(x509.KeyUsageFlags.digitalSignature, true)],
    },
    webcryptoProvider,
  );
  return { certPem: cert.toString("pem"), notBefore, notAfter };
}

export interface ServerIdentity {
  certPem: string;
  keyPem: string;
  notAfter: Date;
  /** Signed by the server's CA with the profile it issues, so the server renews it. */
  selfIssued: boolean;
}

/** Where the server certificate's lifecycle logs (SPEC-004 Amendment 2026-10-15). */
export interface CertLog {
  info(obj: Record<string, unknown>, msg: string): void;
  warn(obj: Record<string, unknown>, msg: string): void;
  error(obj: Record<string, unknown>, msg: string): void;
}

export interface CertCheckOptions {
  /** The clock the checks read; issuance always reads the system clock. */
  now: () => number;
  log: CertLog;
}

type IssueReason = "absent" | "expiring" | "expired" | "key_mismatch";

/** A new self-issued pair: a fresh Ed25519 key, the server profile, 90 days. */
async function issueServerPair(ca: ServerCa): Promise<ServerIdentity> {
  const keys = (await webcryptoProvider.subtle.generateKey(ALG, true, [
    "sign",
    "verify",
  ])) as webcrypto.CryptoKeyPair;
  const notAfter = new Date(Date.now() + SERVER_VALIDITY_MS);
  const cert = await x509.X509CertificateGenerator.create(
    {
      serialNumber: randomSerial(),
      subject: SERVER_SUBJECT,
      issuer: ca.caCert.subject,
      notBefore: new Date(Date.now() - SERVER_BACKDATE_MS),
      notAfter,
      signingAlgorithm: ALG,
      publicKey: keys.publicKey,
      signingKey: ca.caPrivateKey,
      extensions: [
        new x509.KeyUsagesExtension(x509.KeyUsageFlags.digitalSignature, true),
        new x509.ExtendedKeyUsageExtension([EKU_SERVER_AUTH], true),
        new x509.SubjectAlternativeNameExtension([...SERVER_SAN]),
      ],
    },
    webcryptoProvider,
  );

  const der = await webcryptoProvider.subtle.exportKey("pkcs8", keys.privateKey);
  const b64 =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  const keyPem = `-----BEGIN PRIVATE KEY-----\n${b64.join("\n")}\n-----END PRIVATE KEY-----\n`;
  return { certPem: cert.toString("pem"), keyPem, notAfter, selfIssued: true };
}

/**
 * The self-issued SAN as Node's `X509Certificate.subjectAltName` prints it, derived
 * from the SAN issuance writes, so the two cannot drift apart.
 */
const SERVER_SAN_PRINTED = SERVER_SAN.map((n) =>
  n.type === "dns" ? `DNS:${n.value}` : `IP Address:${n.value}`,
);

/**
 * Self-issued: signed by the server's CA and carrying the profile the server
 * issues. A signature that cannot be checked under the CA's key (an issuer that
 * does not sign with Ed25519) counts as operator-provided, as does another subject
 * or SAN.
 */
function isSelfIssued(ca: ServerCa, cert: NodeX509Certificate): boolean {
  if (cert.subject !== SERVER_SUBJECT) return false;
  const names = (cert.subjectAltName ?? "").split(", ").sort();
  if (names.join(",") !== [...SERVER_SAN_PRINTED].sort().join(",")) return false;
  try {
    return cert.verify(new NodeX509Certificate(ca.caCertPem).publicKey);
  } catch {
    return false;
  }
}

/** Whether the key PEM holds the certificate's private key; a key it cannot parse does not. */
function keyMatches(certPem: string, keyPem: string): boolean {
  try {
    return new NodeX509Certificate(certPem).checkPrivateKey(createPrivateKey(keyPem));
  } catch {
    return false;
  }
}

/**
 * Read a pair. The certificate is parsed as the TLS context parses it (OpenSSL,
 * through Node), which takes the file's first certificate and skips other PEM
 * blocks, so a file that served before still serves. One it cannot parse throws,
 * a fatal start-up error, as building the TLS context would.
 */
async function describePair(
  ca: ServerCa,
  certPem: string,
  keyPem: string,
): Promise<ServerIdentity> {
  const cert = new NodeX509Certificate(certPem);
  return {
    certPem,
    keyPem,
    notAfter: new Date(cert.validTo),
    selfIssued: isSelfIssued(ca, cert),
  };
}

function renewalReason(identity: ServerIdentity, nowMs: number): "expired" | "expiring" | null {
  const left = identity.notAfter.getTime() - nowMs;
  if (left <= 0) return "expired";
  if (left < SERVER_RENEW_BEFORE_MS) return "expiring";
  return null;
}

/** Item 7's window: the held certificate has ten minutes or less left, or has expired. */
function inExpiryWindow(identity: ServerIdentity, nowMs: number): boolean {
  return identity.notAfter.getTime() - nowMs <= SERVER_EXPIRY_MARGIN_MS;
}

/** The certificate held, in item 7's window, that the server cannot replace by itself. */
function logExpired(identity: ServerIdentity, certPath: string, log: CertLog): void {
  log.error(
    {
      event: "server_cert_expired",
      not_after: identity.notAfter.toISOString(),
      cert_path: certPath,
    },
    "server certificate expired, and the server cannot replace it",
  );
}

/** An operator-provided pair is never touched; its expiry is logged (item 8). */
function logOperatorExpiry(
  identity: ServerIdentity,
  nowMs: number,
  certPath: string,
  log: CertLog,
): void {
  if (inExpiryWindow(identity, nowMs)) {
    logExpired(identity, certPath, log);
  } else if (renewalReason(identity, nowMs) === "expiring") {
    log.warn(
      {
        event: "server_cert_expiring",
        not_after: identity.notAfter.toISOString(),
        cert_path: certPath,
      },
      "operator server certificate expiring",
    );
  }
}

/**
 * Write a pair: each file to `<path>.tmp`, a leftover one removed first, then
 * each renamed over `<path>`, the key first. The key file is created owner-only.
 * The temporary file is created exclusively (`wx`): a file or link that appears
 * at its path after the removal fails the write, so the key never lands in a
 * file someone else placed there.
 */
function writePair(certPath: string, keyPath: string, certPem: string, keyPem: string): void {
  const files = [
    { path: keyPath, data: keyPem, mode: 0o600 },
    { path: certPath, data: certPem, mode: 0o644 },
  ];
  try {
    for (const { path, data, mode } of files) {
      mkdirSync(dirname(path), { recursive: true });
      rmSync(`${path}.tmp`, { force: true });
      writeFileSync(`${path}.tmp`, data, { mode, flag: "wx" });
    }
    for (const { path } of files) {
      renameSync(`${path}.tmp`, path);
    }
  } catch (err) {
    // No new private key is left behind in a temporary file.
    for (const { path } of files) {
      try {
        rmSync(`${path}.tmp`, { force: true });
      } catch {
        // A leftover that cannot be removed (a directory) is the failure itself.
      }
    }
    throw err;
  }
}

/**
 * Issue and write a new self-issued pair. A failure is logged and `current` is
 * kept; with no pair to keep (`current` null) the failure throws.
 */
async function reissue(
  ca: ServerCa,
  current: ServerIdentity | null,
  reason: IssueReason,
  certPath: string,
  keyPath: string,
  log: CertLog,
): Promise<ServerIdentity> {
  let fresh: ServerIdentity;
  try {
    fresh = await issueServerPair(ca);
    writePair(certPath, keyPath, fresh.certPem, fresh.keyPem);
  } catch (err) {
    log.error(
      {
        event: "server_cert_reissue_failed",
        error: err instanceof Error ? err.message : String(err),
        cert_path: certPath,
        ...(current ? { not_after: current.notAfter.toISOString() } : {}),
      },
      "server certificate reissue failed",
    );
    if (!current) throw err;
    return current;
  }
  log.info(
    {
      event: "server_cert_issued",
      reason,
      not_after: fresh.notAfter.toISOString(),
      cert_path: certPath,
    },
    "server certificate issued",
  );
  return fresh;
}

/**
 * Materialise the mTLS listener's server identity at start (SPEC-004 §Behavior >
 * First run, Amendment 2026-10-15). Both files absent: a self-issued pair —
 * `CN=localhost` with SAN `DNS:localhost` + `IP:127.0.0.1`, so the agent's
 * IP-based server-name verification (`ServerName::try_from("127.0.0.1")`)
 * accepts it. Exactly one present, a file that cannot be read or a certificate
 * that cannot be parsed: fatal, rather than a silent regenerate. An
 * operator-provided pair is used as-is and never overwritten. A self-issued pair
 * is reissued when fewer than 30 days remain, it has expired, or its key does
 * not match.
 */
export async function ensureServerCert(
  ca: ServerCa,
  certPath: string,
  keyPath: string,
  options: CertCheckOptions,
): Promise<ServerIdentity> {
  const { now, log } = options;
  const haveCert = existsSync(certPath);
  const haveKey = existsSync(keyPath);
  if (!haveCert && !haveKey) {
    return reissue(ca, null, "absent", certPath, keyPath, log);
  }
  if (haveCert !== haveKey) {
    throw new Error(
      `incomplete server TLS identity: ${haveCert ? certPath : keyPath} exists but its pair does not; refusing to regenerate`,
    );
  }

  const current = await describePair(
    ca,
    readFileSync(certPath, "utf8"),
    readFileSync(keyPath, "utf8"),
  );
  if (!current.selfIssued) {
    logOperatorExpiry(current, now(), certPath, log);
    return current;
  }
  if (!keyMatches(current.certPem, current.keyPem)) {
    // A mismatched pair cannot serve TLS: if its reissue fails, there is no pair to keep.
    return reissue(ca, null, "key_mismatch", certPath, keyPath, log);
  }
  const reason = renewalReason(current, now());
  return reason ? reissue(ca, current, reason, certPath, keyPath, log) : current;
}

/**
 * What a running server finds on disk when the files are not the pair it serves
 * (SPEC-004 Amendment 2026-10-15 item 4): another instance's renewal to adopt, the
 * servers' own self-issued files to reissue over, or anything else, left alone.
 */
async function onDiskChange(
  ca: ServerCa,
  certPath: string,
  keyPath: string,
  nowMs: number,
): Promise<{ adopt: ServerIdentity } | "reissue" | "leave"> {
  let pair: ServerIdentity;
  try {
    pair = await describePair(ca, readFileSync(certPath, "utf8"), readFileSync(keyPath, "utf8"));
  } catch {
    return "leave";
  }
  if (!pair.selfIssued) return "leave";
  if (keyMatches(pair.certPem, pair.keyPem) && renewalReason(pair, nowMs) === null) {
    return { adopt: pair };
  }
  return "reissue";
}

/**
 * The running server's check of the certificate it serves (SPEC-004 Amendment
 * 2026-10-15 items 2, 4, 6 and 8). Returns the pair to serve after a reissue or
 * an adoption, or null when the listener keeps `served`. When the certificate
 * file is no longer the one served, it adopts a fresh self-issued pair (another
 * instance's reissue) and otherwise writes nothing: the next start classifies
 * the new files.
 */
export async function checkServerCert(
  ca: ServerCa,
  served: ServerIdentity,
  certPath: string,
  keyPath: string,
  options: CertCheckOptions,
): Promise<ServerIdentity | null> {
  const { now, log } = options;
  if (!served.selfIssued) {
    logOperatorExpiry(served, now(), certPath, log);
    return null;
  }
  const reason = renewalReason(served, now());
  if (!reason) return null;

  let onDisk: string | null;
  try {
    onDisk = readFileSync(certPath, "utf8");
  } catch {
    onDisk = null;
  }
  if (onDisk !== served.certPem) {
    const change = await onDiskChange(ca, certPath, keyPath, now());
    if (change === "leave") {
      if (inExpiryWindow(served, now())) {
        logExpired(served, certPath, log);
      } else {
        log.warn(
          { event: "server_cert_changed_on_disk", cert_path: certPath },
          "server certificate file changed while running; the next start reads it",
        );
      }
      return null;
    }
    if (change !== "reissue") {
      log.info(
        {
          event: "server_cert_adopted",
          not_after: change.adopt.notAfter.toISOString(),
          cert_path: certPath,
        },
        "server certificate adopted from disk",
      );
      return change.adopt;
    }
  }
  const next = await reissue(ca, served, reason, certPath, keyPath, log);
  return next === served ? null : next;
}
