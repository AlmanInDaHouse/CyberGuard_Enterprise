import "reflect-metadata";
import type { webcrypto } from "node:crypto";
import * as x509 from "@peculiar/x509";
import type { Pool } from "pg";

// @peculiar/x509 issues over Node's native WebCrypto (SPEC-004 FR-006,
// verified viable in ADR-0007 §Context). reflect-metadata is required by
// @peculiar/x509 v2's tsyringe DI and must be imported before use.
const webcryptoProvider = globalThis.crypto;
x509.cryptoProvider.set(webcryptoProvider);

const ALG = { name: "Ed25519" } as const;
const CA_SUBJECT = "CN=CyberGuard Ingest CA";
/** CA validity: 10 years — it must outlive every 90-day client cert it signs. */
const CA_VALIDITY_MS = 10 * 365 * 24 * 60 * 60 * 1000;

export interface ServerCa {
  /** The CA certificate PEM — agent trust anchor and server-cert issuer. */
  caCertPem: string;
  caCert: x509.X509Certificate;
  /** CA private key (Ed25519), used to sign client + server certs. */
  caPrivateKey: webcrypto.CryptoKey;
  /** CA public key, needed to re-attach to a loaded-from-PEM certificate. */
  caPublicKey: webcrypto.CryptoKey;
}

function pemToPkcs8Der(pem: string): Uint8Array {
  const b64 = pem
    .replace(/-----BEGIN [^-]+-----/g, "")
    .replace(/-----END [^-]+-----/g, "")
    .replace(/\s+/g, "");
  // new Uint8Array(buf) copies exactly the decoded bytes; do NOT use
  // `.buffer`, which exposes Node's shared 8 KB allocation pool.
  return new Uint8Array(Buffer.from(b64, "base64"));
}

async function exportPkcs8Pem(key: webcrypto.CryptoKey): Promise<string> {
  const der = await webcryptoProvider.subtle.exportKey("pkcs8", key);
  const b64 =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  return `-----BEGIN PRIVATE KEY-----\n${b64.join("\n")}\n-----END PRIVATE KEY-----\n`;
}

/**
 * Ensure the server CA exists: generate a self-signed Ed25519 root on first
 * run and store it in the single-row `ca` table (private key encrypted at
 * rest with pgcrypto `pgp_sym_encrypt` under `passphrase`); otherwise load
 * and decrypt the existing one. Idempotent and race-safe via
 * `INSERT … ON CONFLICT DO NOTHING` followed by a re-read.
 */
export async function ensureCa(pool: Pool, passphrase: string): Promise<ServerCa> {
  const existing = await loadCa(pool, passphrase);
  if (existing) {
    return existing;
  }

  const keys = (await webcryptoProvider.subtle.generateKey(ALG, true, [
    "sign",
    "verify",
  ])) as webcrypto.CryptoKeyPair;
  const now = new Date();
  const caCert = await x509.X509CertificateGenerator.createSelfSigned(
    {
      serialNumber: randomSerial(),
      name: CA_SUBJECT,
      notBefore: new Date(now.getTime() - 60_000),
      notAfter: new Date(now.getTime() + CA_VALIDITY_MS),
      signingAlgorithm: ALG,
      keys,
      extensions: [
        new x509.BasicConstraintsExtension(true, undefined, true),
        new x509.KeyUsagesExtension(
          x509.KeyUsageFlags.keyCertSign | x509.KeyUsageFlags.cRLSign,
          true,
        ),
      ],
    },
    webcryptoProvider,
  );

  const caCertPem = caCert.toString("pem");
  const keyPem = await exportPkcs8Pem(keys.privateKey);
  await pool.query(
    "INSERT INTO ca (id, cert_pem, private_key) VALUES (1, $1, pgp_sym_encrypt($2, $3)) ON CONFLICT (id) DO NOTHING",
    [caCertPem, keyPem, passphrase],
  );

  // Re-read: if a concurrent instance won the INSERT, adopt its CA so every
  // instance signs with the same root.
  const loaded = await loadCa(pool, passphrase);
  if (!loaded) {
    throw new Error("CA row vanished immediately after insert");
  }
  return loaded;
}

async function loadCa(pool: Pool, passphrase: string): Promise<ServerCa | null> {
  const res = await pool.query<{ cert_pem: string; private_key_pem: string }>(
    "SELECT cert_pem, pgp_sym_decrypt(private_key, $1) AS private_key_pem FROM ca WHERE id = 1",
    [passphrase],
  );
  const row = res.rows[0];
  if (!row) {
    return null;
  }
  const caCert = new x509.X509Certificate(row.cert_pem);
  const caPrivateKey = (await webcryptoProvider.subtle.importKey(
    "pkcs8",
    pemToPkcs8Der(row.private_key_pem),
    ALG,
    true,
    ["sign"],
  )) as webcrypto.CryptoKey;
  const caPublicKey = (await caCert.publicKey.export(webcryptoProvider)) as webcrypto.CryptoKey;
  return { caCertPem: row.cert_pem, caCert, caPrivateKey, caPublicKey };
}

function randomSerial(): string {
  return Buffer.from(webcryptoProvider.getRandomValues(new Uint8Array(16))).toString("hex");
}
