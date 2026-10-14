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

let fixedHash: Promise<string> | undefined;

/**
 * A fixed Argon2id hash of the service's parameters, made once per process: the login
 * verifies against it when no active account holds the address, so its work does not
 * tell whether one does (SPEC-008 Amendment 2026-10-15 item 11, NFR-008-003).
 */
export function constantWorkHash(): Promise<string> {
  fixedHash ??= hashPassword(globalThis.crypto.randomUUID());
  return fixedHash;
}
