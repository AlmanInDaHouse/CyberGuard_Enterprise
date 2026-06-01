import { Algorithm, hash, verify } from "@node-rs/argon2";

/**
 * SPEC-008 §1 / ADR-0014 §1 — password hashing with Argon2id. Parameters at the
 * library defaults (≥ OWASP guidance, NFR-008-001); per-password random salt is
 * handled by the encoder. The returned string is the self-describing `$argon2id$`
 * encoding stored in `users.password_hash`.
 */
export function hashPassword(password: string): Promise<string> {
  return hash(password, { algorithm: Algorithm.Argon2id });
}

export async function verifyPassword(stored: string, password: string): Promise<boolean> {
  try {
    return await verify(stored, password);
  } catch {
    return false;
  }
}
