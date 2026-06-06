import type { webcrypto } from "node:crypto";
import type { Pool } from "pg";

// SPEC-012 §Data contracts §4 / ADR-0016 §5 — the DEDICATED forensic Ed25519 signing
// key, owned ENTIRELY by services/api. It signs the evidence hash-chain head `chain_N`
// (the `root_signature`; gate C). It is NEVER the ingest CA (ADR-0016 §5 + §Compliance
// line 103 — the ADR-0014 human↔agent boundary): this module lives wholly in
// services/api and never reaches into services/ingest for key material. Key-at-rest is
// a 1:1 mirror of the ingest CA pgcrypto pattern (services/ingest/src/ca.ts:53, :85-88,
// :101) — single-row table, private key `pgp_sym_encrypt`'d, decrypt-on-load — but it is
// REIMPLEMENTED here, not imported (the boundary). SHA-256 / Ed25519 use native
// WebCrypto (the ca.ts:9-10 + hashchain.ts:10 precedent; Node >= 22 per package.json).

const ALG = { name: "Ed25519" } as const;
const subtle = globalThis.crypto.subtle;

/**
 * The loaded forensic key: a `sign(bytes)` capability over an in-memory, NON-extractable
 * private CryptoKey, plus the raw 32-byte public key for embedding in the export
 * (SPEC-012 §3). The private key is never re-exportable from this object (it is imported
 * `extractable=false`) — defence in depth against accidental serialization (the §Compliance
 * "private key never crosses the boundary" rule, hashchain_ac_006).
 */
export interface ForensicKey {
  /** Raw 32-byte Ed25519 public key; the export embeds it base64url'd. */
  readonly publicKeyRaw: Uint8Array;
  /** Ed25519-sign arbitrary bytes (the 32-byte `chain_N`) → 64-byte signature. */
  sign(data: Uint8Array): Promise<Uint8Array>;
}

function toForensicKey(privateKey: webcrypto.CryptoKey, publicKeyRaw: Uint8Array): ForensicKey {
  return {
    publicKeyRaw,
    async sign(data: Uint8Array): Promise<Uint8Array> {
      return new Uint8Array(await subtle.sign(ALG, privateKey, data));
    },
  };
}

async function loadForensicKey(pool: Pool, passphrase: string): Promise<ForensicKey | null> {
  const res = await pool.query<{ private_key_b64: string; public_key: Buffer }>(
    "SELECT pgp_sym_decrypt(private_key, $1) AS private_key_b64, public_key FROM forensic_key WHERE id = 1",
    [passphrase],
  );
  const row = res.rows[0];
  if (!row) {
    return null;
  }
  const pkcs8 = new Uint8Array(Buffer.from(row.private_key_b64, "base64"));
  // extractable=false: once loaded, the private key cannot be re-exported.
  const privateKey = (await subtle.importKey("pkcs8", pkcs8, ALG, false, [
    "sign",
  ])) as webcrypto.CryptoKey;
  return toForensicKey(privateKey, new Uint8Array(row.public_key));
}

/**
 * Ensure the dedicated forensic signing key exists: generate an Ed25519 keypair on first
 * run and persist it (private key PKCS#8 → base64 → `pgp_sym_encrypt`'d under `passphrase`;
 * public key raw 32 bytes, plaintext), otherwise load and decrypt the existing one.
 * Idempotent and race-safe via `INSERT … ON CONFLICT DO NOTHING` followed by a re-read —
 * a 1:1 mirror of ensureCa (services/ingest/src/ca.ts:53). The returned key always wraps a
 * NON-extractable private CryptoKey (both paths return via loadForensicKey).
 *
 * Requires the `0003_forensic_key` migration to have run (the production entrypoint runs
 * migrations before buildServices, server.ts:21-24; the test harness in global-setup).
 */
export async function ensureForensicKey(pool: Pool, passphrase: string): Promise<ForensicKey> {
  const existing = await loadForensicKey(pool, passphrase);
  if (existing) {
    return existing;
  }

  const keys = (await subtle.generateKey(ALG, true, ["sign", "verify"])) as webcrypto.CryptoKeyPair;
  const pkcs8 = new Uint8Array(await subtle.exportKey("pkcs8", keys.privateKey));
  const publicKeyRaw = new Uint8Array(await subtle.exportKey("raw", keys.publicKey));
  const privateKeyB64 = Buffer.from(pkcs8).toString("base64");

  await pool.query(
    "INSERT INTO forensic_key (id, private_key, public_key) VALUES (1, pgp_sym_encrypt($1, $2), $3) ON CONFLICT (id) DO NOTHING",
    [privateKeyB64, passphrase, Buffer.from(publicKeyRaw)],
  );

  // Re-read: if a concurrent instance won the INSERT, adopt its key so every instance
  // signs with the same forensic key (mirrors ensureCa's re-read).
  const loaded = await loadForensicKey(pool, passphrase);
  if (!loaded) {
    throw new Error("forensic_key row vanished immediately after insert");
  }
  return loaded;
}
