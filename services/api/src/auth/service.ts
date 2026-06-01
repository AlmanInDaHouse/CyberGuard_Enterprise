import type { Redis } from "ioredis";
import type { Pool } from "pg";
import type { Config } from "../config.js";
import { AuthError, RateLimitError } from "../errors.js";
import { appendAudit } from "./audit.js";
import { hashPassword, verifyPassword } from "./password.js";
import { isAllowed, recordFailure, resetAccount } from "./ratelimit.js";
import {
  type SessionInfo,
  createSession,
  resolveSession as resolveSessionRedis,
  revokeAllForUser,
  revokeSession,
} from "./session.js";
import { generateSecret, provisioningUri, verifyTotp } from "./totp.js";

/**
 * SPEC-008 auth-core service — the realisation the GREEN gate fills. Every method
 * is a real control backed by Postgres (users/audit_log) + Redis (sessions /
 * rate-limit / TOTP-replay). Negative branches throw typed errors the app maps to
 * 401/403/429; positive paths write append-only audit rows.
 */
export interface LoginInput {
  email: string;
  password: string;
  totp_code: string;
}
export interface LoginResult {
  token: string;
  csrfToken: string;
  session: SessionInfo;
}
export interface CreateUserInput {
  email: string;
  password: string;
  role: string;
  org_id?: string;
}

export interface AuthService {
  login(input: LoginInput, ip: string): Promise<LoginResult>;
  logout(token: string): Promise<void>;
  resolveSession(token: string): Promise<SessionInfo | null>;
  createUser(input: CreateUserInput): Promise<{ userId: string; totpProvisioningUri: string }>;
  listUsers(): Promise<Array<{ userId: string; email: string; role: string }>>;
  assignRole(userId: string, role: string): Promise<void>;
  changePassword(input: {
    userId: string;
    currentPassword: string;
    newPassword: string;
    totp_code: string;
  }): Promise<void>;
  confirmTotpEnrollment(input: { userId: string; totp_code: string }): Promise<void>;
}

interface AuthUserRow {
  user_id: string;
  role: string;
  status: string;
  password_hash: string;
  totp_enrolled: boolean;
  totp_secret: string; // decrypted base32 (pgp_sym_decrypt)
}

export function createAuthService(pool: Pool, redis: Redis, config: Config): AuthService {
  const ORG = "default";
  return {
    async login(input, ip) {
      // Rate-limit gate BEFORE any credential work (real counter, two axes).
      if (!(await isAllowed(redis, ORG, input.email, ip))) {
        throw new RateLimitError();
      }
      const res = await pool.query<AuthUserRow>(
        `SELECT user_id, role, status, password_hash, totp_enrolled,
                pgp_sym_decrypt(totp_secret, $1)::text AS totp_secret
         FROM users WHERE org_id = $2 AND email = $3`,
        [config.API_DB_ENC_PASSPHRASE, ORG, input.email],
      );
      const user = res.rows[0];
      // Single generic negative branch — no user-enumeration, no per-factor oracle.
      const reject = async (): Promise<never> => {
        await recordFailure(redis, ORG, input.email, ip);
        await appendAudit(pool, {
          userId: user?.user_id ?? null,
          orgId: ORG,
          action: "auth.login.fail",
          outcome: "denied",
          sourceIp: ip,
        });
        throw new AuthError();
      };
      if (!user || user.status !== "active") return reject();
      if (!(await verifyPassword(user.password_hash, input.password))) return reject();
      if (!user.totp_enrolled) return reject();
      if (!verifyTotp(user.totp_secret, input.totp_code)) return reject();
      // Replay rejection: the accepted code is single-use per user within its window.
      const firstUse = await redis.set(
        `totp:replay:${user.user_id}:${input.totp_code}`,
        "1",
        "EX",
        90,
        "NX",
      );
      if (firstUse === null) return reject();

      await resetAccount(redis, ORG, input.email);
      const { token, csrfToken } = await createSession(redis, {
        user_id: user.user_id,
        org_id: ORG,
        role: user.role,
      });
      await pool.query("UPDATE users SET last_login_at = now() WHERE user_id = $1", [user.user_id]);
      await appendAudit(pool, {
        userId: user.user_id,
        orgId: ORG,
        action: "auth.login.ok",
        outcome: "ok",
        sourceIp: ip,
      });
      return {
        token,
        csrfToken,
        session: { user_id: user.user_id, org_id: ORG, role: user.role, csrf_token: csrfToken },
      };
    },

    async logout(token) {
      const sess = await resolveSessionRedis(redis, token);
      await revokeSession(redis, token);
      await appendAudit(pool, {
        userId: sess?.user_id ?? null,
        action: "auth.logout",
        outcome: "ok",
      });
    },

    resolveSession(token) {
      return resolveSessionRedis(redis, token);
    },

    async createUser(input) {
      const userId = globalThis.crypto.randomUUID();
      const ph = await hashPassword(input.password);
      const secret = generateSecret();
      await pool.query(
        `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
         VALUES ($1, $2, $3, $4, pgp_sym_encrypt($5, $6), false, $7)`,
        [
          userId,
          input.org_id ?? ORG,
          input.email,
          ph,
          secret,
          config.API_DB_ENC_PASSPHRASE,
          input.role,
        ],
      );
      await appendAudit(pool, { userId, action: "user.create", targetId: userId, outcome: "ok" });
      return { userId, totpProvisioningUri: provisioningUri(input.email, secret) };
    },

    async listUsers() {
      const res = await pool.query<{ user_id: string; email: string; role: string }>(
        "SELECT user_id, email, role FROM users ORDER BY created_at",
      );
      return res.rows.map((r) => ({ userId: r.user_id, email: r.email, role: r.role }));
    },

    async assignRole(userId, role) {
      await pool.query("UPDATE users SET role = $2, updated_at = now() WHERE user_id = $1", [
        userId,
        role,
      ]);
      await revokeAllForUser(redis, userId); // privilege change rotates the user's sessions
      await appendAudit(pool, { userId, action: "role.change", targetId: userId, outcome: "ok" });
    },

    async changePassword(input) {
      const res = await pool.query<{ password_hash: string; totp_secret: string }>(
        `SELECT password_hash, pgp_sym_decrypt(totp_secret, $1)::text AS totp_secret
         FROM users WHERE user_id = $2`,
        [config.API_DB_ENC_PASSPHRASE, input.userId],
      );
      const u = res.rows[0];
      if (
        !u ||
        !(await verifyPassword(u.password_hash, input.currentPassword)) ||
        !verifyTotp(u.totp_secret, input.totp_code)
      ) {
        throw new AuthError();
      }
      const ph = await hashPassword(input.newPassword);
      await pool.query(
        "UPDATE users SET password_hash = $2, updated_at = now() WHERE user_id = $1",
        [input.userId, ph],
      );
      await revokeAllForUser(redis, input.userId);
      await appendAudit(pool, {
        userId: input.userId,
        action: "user.password.change",
        outcome: "ok",
      });
    },

    async confirmTotpEnrollment(input) {
      const res = await pool.query<{ user_id: string; totp_secret: string }>(
        `SELECT user_id, pgp_sym_decrypt(totp_secret, $1)::text AS totp_secret
         FROM users WHERE user_id = $2`,
        [config.API_DB_ENC_PASSPHRASE, input.userId],
      );
      const u = res.rows[0];
      if (!u || !verifyTotp(u.totp_secret, input.totp_code)) {
        throw new AuthError();
      }
      await pool.query(
        "UPDATE users SET totp_enrolled = true, updated_at = now() WHERE user_id = $1",
        [input.userId],
      );
      await appendAudit(pool, { userId: input.userId, action: "user.totp.enroll", outcome: "ok" });
    },
  };
}
