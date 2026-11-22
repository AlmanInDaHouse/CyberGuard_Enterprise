import { createHash } from "node:crypto";

/**
 * SHA-256 of an enrollment token's text, its base64url characters as ASCII: the only form
 * of the token the database holds (SPEC-025 §Data contracts §1).
 */
export function tokenSha256(token: string): Buffer {
  return createHash("sha256").update(token, "utf8").digest();
}
