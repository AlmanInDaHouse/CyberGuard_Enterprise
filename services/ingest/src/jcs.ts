import { createRequire } from "node:module";

// canonicalize is CJS; load via createRequire for clean ESM interop. It
// implements RFC 8785 (JCS) — byte-identical to the agent's serde_jcs over
// the envelope's value space (ASCII strings, unsigned integers, nested
// objects; no floats), per SPEC-004 FR-010.
const canonicalize = createRequire(import.meta.url)("canonicalize") as (
  value: unknown,
) => string | undefined;

const subtle = globalThis.crypto.subtle;
const ALG = { name: "Ed25519" } as const;

/**
 * Verify the SPEC-003 signature: Ed25519 over JCS(outer envelope minus the
 * `signature` field), under the agent's raw 32-byte public key. Returns
 * false on any malformed input rather than throwing, so the caller maps it
 * to a clean `401`.
 */
export async function verifyEnvelopeSignature(
  envelope: Record<string, unknown>,
  signatureB64url: string,
  rawPubkey: Uint8Array,
): Promise<boolean> {
  const { signature: _omit, ...signedRegion } = envelope;
  const canonical = canonicalize(signedRegion);
  if (canonical === undefined) {
    return false;
  }
  let signature: Buffer;
  try {
    signature = Buffer.from(signatureB64url, "base64url");
  } catch {
    return false;
  }
  try {
    const key = await subtle.importKey("raw", rawPubkey, ALG, false, ["verify"]);
    return await subtle.verify(ALG, key, signature, new TextEncoder().encode(canonical));
  } catch {
    return false;
  }
}
