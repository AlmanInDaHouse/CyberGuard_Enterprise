import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import otplib from "otplib";
import pg from "pg";
import { buildApp } from "../../src/app.js";
import { hashPassword } from "../../src/auth/password.js";
import type { Config } from "../../src/config.js";
import { buildServices } from "../../src/services.js";

/**
 * Direct-SQL / direct-Redis setup helpers for the auth-core harness. They write
 * the REAL tables (users / audit_log, created by the api migrations in
 * startBackends) and the REAL Redis session keyspace, so a test's PRECONDITIONS
 * exist and the assertion under test is the only variable. Mirrors the pattern of
 * services/ingest/test/helpers/db.ts (enrollTestAgent, insertCgesEvent).
 */

/** A valid base32 TOTP secret shared by the harness; the login / enrollment tests
 *  compute the current code from it via `currentTotp`. */
export const KNOWN_TOTP_SECRET = "JBSWY3DPEHPK3PXP";
/** The default password `insertUserDirect` hashes; the login tests send this. */
export const DEFAULT_PASSWORD = "correct horse battery";

/** The current RFC 6238 code for a secret (default: the shared harness secret). */
export function currentTotp(secret: string = KNOWN_TOTP_SECRET): string {
  return otplib.authenticator.generate(secret);
}

/** Insert a user row directly (the bootstrap-CLI equivalent) so login / RBAC /
 *  enrollment tests have a precondition principal with REAL credentials: a true
 *  Argon2id hash of `password` and the shared base32 TOTP secret, encrypted at
 *  rest exactly as createUser stores it. auth_ac_008 deliberately does NOT use
 *  this helper (it exercises the real createUser). Returns the user_id. */
export async function insertUserDirect(
  config: Config,
  opts: {
    email: string;
    role: "admin" | "analyst" | "viewer";
    orgId?: string;
    totpEnrolled?: boolean;
    status?: "active" | "disabled";
    userId?: string;
    password?: string;
    totpSecret?: string;
  },
): Promise<string> {
  const userId = opts.userId ?? globalThis.crypto.randomUUID();
  const passwordHash = await hashPassword(opts.password ?? DEFAULT_PASSWORD);
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO users
         (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role, status)
       VALUES ($1, $2, $3, $4, pgp_sym_encrypt($5, $6), $7, $8, $9)`,
      [
        userId,
        opts.orgId ?? "default",
        opts.email,
        passwordHash,
        opts.totpSecret ?? KNOWN_TOTP_SECRET,
        config.API_DB_ENC_PASSPHRASE,
        opts.totpEnrolled ?? true,
        opts.role,
        opts.status ?? "active",
      ],
    );
  } finally {
    await pool.end();
  }
  return userId;
}

export interface UserRow {
  user_id: string;
  email: string;
  role: string;
  status: string;
  totp_enrolled: boolean;
  password_hash: string;
  /** Raw stored bytes of totp_secret (ciphertext once encrypted). */
  totp_secret: Buffer;
}

export async function getUserByEmail(config: Config, email: string): Promise<UserRow | null> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<UserRow>(
      `SELECT user_id, email, role, status, totp_enrolled, password_hash, totp_secret
       FROM users WHERE email = $1`,
      [email],
    );
    return r.rows[0] ?? null;
  } finally {
    await pool.end();
  }
}

export interface AuditRow {
  action: string;
  user_id: string | null;
  outcome: string;
}

export async function getAuditRows(
  config: Config,
  opts: { action?: string } = {},
): Promise<AuditRow[]> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const where = opts.action ? "WHERE action = $1" : "";
    const params = opts.action ? [opts.action] : [];
    const r = await pool.query<AuditRow>(
      `SELECT action, user_id, outcome FROM audit_log ${where} ORDER BY id`,
      params,
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

export interface SeedSession {
  token: string;
  userId: string;
  role: "admin" | "analyst" | "viewer";
  orgId?: string;
  csrfToken?: string;
  ttlSeconds?: number;
}

/** Seed an opaque session directly into Redis (cgsess:<token>), the format
 *  SPEC-008 §Data contracts §3 specifies — so RBAC / revocation / CSRF tests can
 *  present an authenticated principal without depending on the absent login. */
export async function seedSession(config: Config, s: SeedSession): Promise<void> {
  const redis = new Redis(config.API_REDIS_URL);
  try {
    const payload = JSON.stringify({
      user_id: s.userId,
      org_id: s.orgId ?? "default",
      role: s.role,
      csrf_token: s.csrfToken ?? `csrf-${s.token}`,
      issued_at: "2026-06-01T00:00:00.000Z",
      last_seen: "2026-06-01T00:00:00.000Z",
    });
    await redis.set(`cgsess:${s.token}`, payload, "EX", s.ttlSeconds ?? 3600);
  } finally {
    await redis.quit();
  }
}

export async function sessionExists(config: Config, token: string): Promise<boolean> {
  const redis = new Redis(config.API_REDIS_URL);
  try {
    return (await redis.exists(`cgsess:${token}`)) === 1;
  } finally {
    await redis.quit();
  }
}

export async function revokeSessionDirect(config: Config, token: string): Promise<void> {
  const redis = new Redis(config.API_REDIS_URL);
  try {
    await redis.del(`cgsess:${token}`);
  } finally {
    await redis.quit();
  }
}

/** Build the api app + services for `app.inject()` (no listen). */
export async function buildTestApp(
  config: Config,
): Promise<{ app: FastifyInstance; close: () => Promise<void> }> {
  const services = await buildServices(config);
  const app = buildApp(services);
  await app.ready();
  return {
    app,
    async close() {
      await app.close();
      await services.close();
    },
  };
}
