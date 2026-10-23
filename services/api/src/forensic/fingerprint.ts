import { toHex } from "./hashchain.js";

/**
 * The fingerprint of the forensic public key (ADR-0016 Amendment 2026-10-19 item 1):
 * `sha256:` and the SHA-256 of its 32 bytes in lowercase hexadecimal. The installation
 * records it outside CyberGuard; the export and the report carry it beside the key.
 */
export async function fingerprintOf(publicKeyRaw: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", publicKeyRaw));
  return `sha256:${toHex(digest)}`;
}
