import "reflect-metadata";
import type { webcrypto } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import * as x509 from "@peculiar/x509";
import type { ServerCa } from "./ca.js";

const webcryptoProvider = globalThis.crypto;
x509.cryptoProvider.set(webcryptoProvider);

const ALG = { name: "Ed25519" } as const;
/** Client-cert TTL — 90 days per ADR-0004 / SPEC-004 FR-006. */
const CLIENT_VALIDITY_MS = 90 * 24 * 60 * 60 * 1000;
const SERVER_VALIDITY_MS = 90 * 24 * 60 * 60 * 1000;
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
}

/**
 * Materialise the mTLS listener's server identity (SPEC-004 §Behavior > First
 * run / §Ratification decision 3). If both `certPath` and `keyPath` exist they
 * are used as-is and never overwritten (operator-provided cert honoured).
 * Otherwise a fresh Ed25519 server cert is issued by the CA — `CN=localhost`
 * with SAN `DNS:localhost` + `IP:127.0.0.1` so the agent's rustls IP-based
 * server-name verification (`ServerName::try_from("127.0.0.1")`) accepts it —
 * and written to disk. Exactly one of the two files existing is a fatal,
 * ambiguous state rather than a silent regenerate.
 */
export async function ensureServerCert(
  ca: ServerCa,
  certPath: string,
  keyPath: string,
): Promise<ServerIdentity> {
  const haveCert = existsSync(certPath);
  const haveKey = existsSync(keyPath);
  if (haveCert && haveKey) {
    return { certPem: readFileSync(certPath, "utf8"), keyPem: readFileSync(keyPath, "utf8") };
  }
  if (haveCert !== haveKey) {
    throw new Error(
      `incomplete server TLS identity: ${haveCert ? certPath : keyPath} exists but its pair does not; refusing to regenerate`,
    );
  }

  const keys = (await webcryptoProvider.subtle.generateKey(ALG, true, [
    "sign",
    "verify",
  ])) as webcrypto.CryptoKeyPair;
  const cert = await x509.X509CertificateGenerator.create(
    {
      serialNumber: randomSerial(),
      subject: "CN=localhost",
      issuer: ca.caCert.subject,
      notBefore: new Date(Date.now() - 60_000),
      notAfter: new Date(Date.now() + SERVER_VALIDITY_MS),
      signingAlgorithm: ALG,
      publicKey: keys.publicKey,
      signingKey: ca.caPrivateKey,
      extensions: [
        new x509.KeyUsagesExtension(x509.KeyUsageFlags.digitalSignature, true),
        new x509.ExtendedKeyUsageExtension([EKU_SERVER_AUTH], true),
        new x509.SubjectAlternativeNameExtension([
          { type: "dns", value: "localhost" },
          { type: "ip", value: "127.0.0.1" },
        ]),
      ],
    },
    webcryptoProvider,
  );

  const certPem = cert.toString("pem");
  const der = await webcryptoProvider.subtle.exportKey("pkcs8", keys.privateKey);
  const b64 =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  const keyPem = `-----BEGIN PRIVATE KEY-----\n${b64.join("\n")}\n-----END PRIVATE KEY-----\n`;

  for (const p of [certPath, keyPath]) {
    mkdirSync(dirname(p), { recursive: true });
  }
  writeFileSync(certPath, certPem);
  writeFileSync(keyPath, keyPem);
  return { certPem, keyPem };
}
