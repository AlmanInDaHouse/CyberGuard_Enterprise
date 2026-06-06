import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { ensureForensicKey } from "../src/forensic/key.js";

// SPEC-012 hashchain_ac_005 (SC-HC-005) — the forensic key at rest. Against the REAL
// 0003_forensic_key migration in the throwaway Postgres: ensureForensicKey round-trips
// encrypt → decrypt → sign; the stored private_key column is pgcrypto CIPHERTEXT (never the
// plaintext PKCS#8); and a wrong passphrase cannot decrypt. CI-able, no elevation.

let config: Config;
let pool: pg.Pool;

async function ed25519Verify(
  pubRaw: Uint8Array,
  sig: Uint8Array,
  msg: Uint8Array,
): Promise<boolean> {
  const key = await globalThis.crypto.subtle.importKey("raw", pubRaw, { name: "Ed25519" }, false, [
    "verify",
  ]);
  return globalThis.crypto.subtle.verify({ name: "Ed25519" }, key, sig, msg);
}

beforeAll(async () => {
  config = inject("apiConfig");
  pool = new pg.Pool({ connectionString: config.API_PG_URL });
});
afterAll(async () => {
  await pool?.end();
});

test("hashchain_ac_005: forensic key round-trips encrypt→decrypt→sign via pgcrypto", async () => {
  // Bootstrap-or-load on the REAL migration (idempotent; another suite may have created it).
  const key = await ensureForensicKey(pool, config.API_FORENSIC_PASSPHRASE);

  // The decrypted private key signs; the public key verifies — the full round-trip.
  const msg = new TextEncoder().encode("forensic-key-at-rest-ac005");
  const sig = await key.sign(msg);
  expect(sig.length).toBe(64); // Ed25519 signature
  expect(key.publicKeyRaw.length).toBe(32); // raw Ed25519 public key
  expect(await ed25519Verify(key.publicKeyRaw, sig, msg)).toBe(true);
});

test("hashchain_ac_005: private_key is ciphertext at rest, never plaintext", async () => {
  await ensureForensicKey(pool, config.API_FORENSIC_PASSPHRASE);

  // The raw stored bytes (PGP ciphertext) and the decrypted plaintext (the base64 PKCS#8).
  const stored = await pool.query<{ private_key: Buffer }>(
    "SELECT private_key FROM forensic_key WHERE id = 1",
  );
  const decrypted = await pool.query<{ pk: string }>(
    "SELECT pgp_sym_decrypt(private_key, $1) AS pk FROM forensic_key WHERE id = 1",
    [config.API_FORENSIC_PASSPHRASE],
  );
  const ciphertext = stored.rows[0]?.private_key;
  const plaintextB64 = decrypted.rows[0]?.pk;
  expect(ciphertext).toBeInstanceOf(Buffer);
  expect(plaintextB64?.length ?? 0).toBeGreaterThan(0);
  if (!ciphertext || !plaintextB64) {
    throw new Error("forensic_key row missing after ensureForensicKey");
  }

  // The ciphertext bytes never contain the plaintext base64 — the key is never stored clear.
  expect(ciphertext.toString("latin1")).not.toContain(plaintextB64);

  // The decrypted plaintext is a valid PKCS#8 Ed25519 private key (re-imports for signing).
  const reimported = await globalThis.crypto.subtle.importKey(
    "pkcs8",
    new Uint8Array(Buffer.from(plaintextB64, "base64")),
    { name: "Ed25519" },
    false,
    ["sign"],
  );
  expect(reimported.type).toBe("private");
});

test("hashchain_ac_005: a wrong passphrase cannot decrypt the private key", async () => {
  await ensureForensicKey(pool, config.API_FORENSIC_PASSPHRASE);

  // pgcrypto rejects a wrong passphrase ("Wrong key or corrupt data").
  await expect(
    pool.query("SELECT pgp_sym_decrypt(private_key, $1) AS pk FROM forensic_key WHERE id = 1", [
      "definitely-the-wrong-passphrase",
    ]),
  ).rejects.toThrow();
});
