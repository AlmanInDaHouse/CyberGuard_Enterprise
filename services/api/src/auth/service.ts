import type { Redis } from "ioredis";
import pg, { type Pool, type PoolClient } from "pg";
import type { Config } from "../config.js";
import {
  AuthError,
  ConflictError,
  NotFoundError,
  RateLimitError,
  StoreUnavailableError,
} from "../errors.js";
import { asStore } from "../stores.js";
import { appendAudit } from "./audit.js";
import { constantWorkHash, hashPassword, verifyPassword } from "./password.js";
import {
  confirmFailed,
  loginFailed,
  loginSucceeded,
  passwordChangeFailed,
  releaseConfirm,
  releaseLogin,
  releasePasswordChange,
  reserveConfirm,
  reserveLogin,
  reservePasswordChange,
} from "./ratelimit.js";
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
 * 401/403/404/409/429; positive paths write append-only audit rows.
 *
 * SPEC-008 Amendment 2026-10-15: the user routes act in the session's org and write
 * their change and its audit row in one transaction; a role, status or password
 * change raises the user's authority version, and the login reads it twice, so a
 * login racing a change keeps no session with the old authority.
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
}
export type UserStatus = "active" | "disabled";

/** What a request carries into the service: its id, its address, and its session. */
export interface RequestContext {
  correlationId: string;
  sourceIp: string;
  session?: SessionInfo;
}

export interface AuthService {
  login(input: LoginInput, ctx: RequestContext): Promise<LoginResult>;
  logout(token: string, ctx: RequestContext): Promise<void>;
  resolveSession(token: string): Promise<SessionInfo | null>;
  createUser(
    input: CreateUserInput,
    ctx: RequestContext,
  ): Promise<{ userId: string; totpProvisioningUri: string }>;
  listUsers(
    ctx: RequestContext,
  ): Promise<Array<{ userId: string; email: string; role: string; status: string }>>;
  assignRole(userId: string, role: string, ctx: RequestContext): Promise<void>;
  setStatus(userId: string, status: UserStatus, ctx: RequestContext): Promise<void>;
  changePassword(
    input: { userId: string; currentPassword: string; newPassword: string; totp_code: string },
    ctx: RequestContext,
  ): Promise<void>;
  confirmTotpEnrollment(
    input: { userId: string; totp_code: string },
    ctx: RequestContext,
  ): Promise<void>;
}

interface AuthUserRow {
  user_id: string;
  org_id: string;
  role: string;
  status: string;
  password_hash: string;
  totp_enrolled: boolean;
  authority_version: string; // bigint, as pg returns it
  totp_secret: string; // decrypted base32 (pgp_sym_decrypt)
}

/** An address as the api compares it (SPEC-008 Amendment 2026-10-15 item 3). */
export function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}

/** The session a user route runs under; the hooks guarantee one. */
function sessionOf(ctx: RequestContext): SessionInfo {
  if (!ctx.session) throw new Error("unreachable: requireSession populates the session");
  return ctx.session;
}

/** Whether an error is the server's answer, with a SQLSTATE, rather than the connection's. */
function isServerAnswer(err: unknown): boolean {
  return err instanceof pg.DatabaseError;
}

/** An integrity violation (SQLSTATE class 23): the connection is sound. */
function isIntegrityViolation(err: unknown): boolean {
  return isServerAnswer(err) && String((err as { code?: unknown }).code).startsWith("23");
}

/** A Postgres unique violation, seen through the store view. */
function isUniqueViolation(err: unknown): boolean {
  return (
    err instanceof StoreUnavailableError && (err.cause as { code?: unknown })?.code === "23505"
  );
}

export function createAuthService(rawPool: Pool, rawRedis: Redis, config: Config): AuthService {
  // Every Postgres and Redis call of the service fails as the store's: a 503.
  const pool = asStore(rawPool);
  const redis = asStore(rawRedis);
  const ORG = "default";
  // The constant-work material (Amendment 2026-10-15 items 11 and 12), made at start so
  // the first refused login or confirmation of a process does no extra work.
  constantWorkHash().catch(() => {});
  const constantWorkSecret = generateSecret();
  let dummyCiphertext: Promise<Buffer> | undefined;

  /** The pgcrypto decrypt a found row costs, on a path that found no row (items 11, 12). */
  async function decryptLikeARow(): Promise<void> {
    dummyCiphertext ??= pool
      .query<{ c: Buffer }>("SELECT pgp_sym_encrypt('constant work', $1) AS c", [
        config.API_DB_ENC_PASSPHRASE,
      ])
      .then((r) => r.rows[0]?.c ?? Buffer.alloc(0))
      .catch((err: unknown) => {
        dummyCiphertext = undefined;
        throw err;
      });
    const c = await dummyCiphertext;
    await pool.query("SELECT pgp_sym_decrypt($1::bytea, $2) AS x", [
      c,
      config.API_DB_ENC_PASSPHRASE,
    ]);
  }

  /**
   * Run `work` in one transaction on one client; any failure rolls it back.
   * - A store failure leaves the pool with its client (item 15), a statement cancelled
   *   by `statement_timeout` included; only an integrity violation (a taken address)
   *   returns the connection to the pool.
   * - A failure of the connection itself, not an answer of the server (a timeout, a
   *   reset), is not followed by a ROLLBACK, which would wait behind the query that
   *   hung: closing the connection rolls the transaction back.
   * - A COMMIT that got no answer leaves the outcome unknown, and is marked so. A COMMIT
   *   the server refused rolled back, and is not.
   */
  async function inTransaction<T>(work: (db: PoolClient) => Promise<T>): Promise<T> {
    const db = asStore(await pool.connect());
    let broken = false;
    let committing = false;
    try {
      await db.query("BEGIN");
      const result = await work(db);
      committing = true;
      await db.query("COMMIT");
      return result;
    } catch (err) {
      const store = err instanceof StoreUnavailableError;
      const answered = store && isServerAnswer(err.cause);
      broken = store && !isIntegrityViolation(err.cause);
      if (!store || answered) await db.query("ROLLBACK").catch(() => {});
      if (committing && store && !answered) {
        Object.assign(err, { commitUncertain: true });
      }
      throw err;
    } finally {
      db.release(broken);
    }
  }

  /**
   * A change of a user's authority, then the deletion of its sessions (item 9). Only a
   * COMMIT whose answer was lost leaves the change possibly made; then the deletion is
   * tried anyway, since by then the change found its user in the session's org and
   * wrote its row inside the transaction. Any earlier failure, and a refusal (404,
   * 409), changed nothing and deletes nothing.
   */
  async function authorityChange(
    userId: string,
    work: (db: PoolClient) => Promise<void>,
  ): Promise<void> {
    try {
      await inTransaction(work);
    } catch (err) {
      if ((err as { commitUncertain?: boolean }).commitUncertain) {
        await revokeAllForUser(redis, userId).catch(() => {});
      }
      throw err;
    }
    await deleteSessionsAfterChange(userId);
  }

  /**
   * Delete a user's sessions after a change committed (SPEC-008 §2; Amendment
   * 2026-10-15 item 9). If Redis cannot, the change stands and the request is a 503,
   * logged with the user's id so the sessions left can be found.
   */
  async function deleteSessionsAfterChange(userId: string): Promise<void> {
    try {
      await revokeAllForUser(redis, userId);
    } catch (err) {
      const cause = err instanceof StoreUnavailableError ? err.cause : err;
      const code = (cause as { code?: unknown })?.code;
      throw new StoreUnavailableError(
        Object.assign(
          new Error(`the sessions of user ${userId} were not deleted after its change committed`),
          { code },
        ),
      );
    }
  }

  /**
   * A role or status change (SPEC-008 Amendment 2026-10-15 items 5, 7, 8, 10): the org's
   * active admins and the target are locked in user-id order, the last active admin is
   * kept, the change raises the authority version and writes its row, and the user's
   * sessions are deleted after the commit. A change to the value already held is
   * recorded like any other.
   */
  async function changeUser(
    targetId: string,
    change: { role?: string; status?: UserStatus },
    ctx: RequestContext,
  ): Promise<void> {
    const session = sessionOf(ctx);
    await authorityChange(targetId, async (db) => {
      const locked = await db.query<{ user_id: string; role: string; status: string }>(
        `SELECT user_id, role, status FROM users
          WHERE org_id = $1 AND ((role = 'admin' AND status = 'active') OR user_id = $2)
          ORDER BY user_id FOR UPDATE`,
        [session.org_id, targetId],
      );
      const target = locked.rows.find((r) => r.user_id === targetId);
      if (!target) throw new NotFoundError();
      const role = change.role ?? target.role;
      const status = change.status ?? target.status;
      const activeAdmins = locked.rows.filter((r) => r.role === "admin" && r.status === "active");
      const removesAnAdmin =
        target.role === "admin" &&
        target.status === "active" &&
        (role !== "admin" || status !== "active");
      if (removesAnAdmin && activeAdmins.length <= 1) throw new ConflictError();
      await db.query(
        `UPDATE users
            SET role = $2, status = $3, authority_version = authority_version + 1, updated_at = now()
          WHERE user_id = $1`,
        [targetId, role, status],
      );
      const action =
        change.role !== undefined
          ? "role.change"
          : status === "disabled"
            ? "user.disable"
            : "user.enable";
      const detail =
        change.role !== undefined
          ? { from: target.role, to: role }
          : { from: target.status, to: status };
      await appendAudit(db, {
        userId: session.user_id,
        orgId: session.org_id,
        action,
        targetId,
        outcome: "ok",
        sourceIp: ctx.sourceIp,
        correlationId: ctx.correlationId,
        detail,
      });
    });
  }

  return {
    async login(input, ctx) {
      const email = normalizeEmail(input.email);
      // Rate-limit gate BEFORE any credential work: the attempt reserves its slots on
      // both counters at once (Amendment 2026-10-16 item 3).
      if (!(await reserveLogin(redis, ORG, email, ctx.sourceIp))) {
        throw new RateLimitError();
      }
      // The slots settle when the attempt fails (they are kept) or succeeds (they are
      // given back); an attempt that ends otherwise, in a store failure, gives them back
      // (Amendment 2026-10-16 item 3).
      let settled = false;
      try {
        const res = await pool.query<AuthUserRow>(
          `SELECT user_id, org_id, role, status, password_hash, totp_enrolled, authority_version,
                pgp_sym_decrypt(totp_secret, $1)::text AS totp_secret
         FROM users WHERE org_id = $2 AND email = $3`,
          [config.API_DB_ENC_PASSPHRASE, ORG, email],
        );
        const user = res.rows[0];
        // Single generic negative branch — no user-enumeration, no per-factor oracle.
        // A failure keeps its slots, which count it, and restarts their windows; one that
        // counts no failure gives them back.
        const reject = async (count = true): Promise<never> => {
          settled = true;
          if (count) await loginFailed(redis, ORG, email, ctx.sourceIp);
          else await releaseLogin(redis, ORG, email, ctx.sourceIp);
          await appendAudit(pool, {
            userId: user?.user_id ?? null,
            orgId: user?.org_id ?? ORG,
            action: "auth.login.fail",
            outcome: "denied",
            sourceIp: ctx.sourceIp,
            correlationId: ctx.correlationId,
          });
          throw new AuthError();
        };
        if (!user || user.status !== "active") {
          // The same work as a wrong password (Amendment 2026-10-15 item 11): the decrypt
          // a found row costs, and the Argon2 verification.
          if (!user) await decryptLikeARow();
          await verifyPassword(await constantWorkHash(), input.password);
          return reject();
        }
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

        const { token, csrfToken } = await createSession(redis, {
          user_id: user.user_id,
          org_id: user.org_id,
          role: user.role,
        });
        // Read the row again (Amendment 2026-10-15 item 9): a change that committed since
        // the first read raised the version, and its deletion may have run before this
        // session existed. Then this session goes too.
        const again = await pool.query<{ authority_version: string; status: string }>(
          "SELECT authority_version, status FROM users WHERE user_id = $1",
          [user.user_id],
        );
        const now = again.rows[0];
        if (!now || now.authority_version !== user.authority_version || now.status !== "active") {
          // The credentials were right: the row changed under the login. Its row is
          // written, and nothing counts against the user's limits.
          await revokeSession(redis, token);
          return reject(false);
        }
        // Only a login that keeps its session clears the account's failures.
        settled = true;
        // A give-back Redis cannot take keeps the slots until their window ends, and is
        // a 503, logged (the first Amendment 2026-10-15 item 14).
        await loginSucceeded(redis, ORG, email, ctx.sourceIp);
        await pool.query("UPDATE users SET last_login_at = now() WHERE user_id = $1", [
          user.user_id,
        ]);
        await appendAudit(pool, {
          userId: user.user_id,
          orgId: user.org_id,
          action: "auth.login.ok",
          outcome: "ok",
          sourceIp: ctx.sourceIp,
          correlationId: ctx.correlationId,
        });
        return {
          token,
          csrfToken,
          session: {
            user_id: user.user_id,
            org_id: user.org_id,
            role: user.role,
            csrf_token: csrfToken,
          },
        };
      } catch (err) {
        if (!settled) await releaseLogin(redis, ORG, email, ctx.sourceIp).catch(() => undefined);
        throw err;
      }
    },

    async logout(token, ctx) {
      const sess = await resolveSessionRedis(redis, token);
      await revokeSession(redis, token);
      await appendAudit(pool, {
        userId: sess?.user_id ?? null,
        orgId: sess?.org_id ?? ORG,
        action: "auth.logout",
        outcome: "ok",
        sourceIp: ctx.sourceIp,
        correlationId: ctx.correlationId,
      });
    },

    resolveSession(token) {
      return resolveSessionRedis(redis, token);
    },

    async createUser(input, ctx) {
      const session = sessionOf(ctx);
      const email = normalizeEmail(input.email);
      const userId = globalThis.crypto.randomUUID();
      const ph = await hashPassword(input.password);
      const secret = generateSecret();
      try {
        await inTransaction(async (db) => {
          await db.query(
            `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
             VALUES ($1, $2, $3, $4, pgp_sym_encrypt($5, $6), false, $7)`,
            [userId, session.org_id, email, ph, secret, config.API_DB_ENC_PASSPHRASE, input.role],
          );
          await appendAudit(db, {
            userId: session.user_id,
            orgId: session.org_id,
            action: "user.create",
            targetId: userId,
            outcome: "ok",
            sourceIp: ctx.sourceIp,
            correlationId: ctx.correlationId,
            detail: { role: input.role },
          });
        });
      } catch (err) {
        if (isUniqueViolation(err)) throw new ConflictError();
        throw err;
      }
      return { userId, totpProvisioningUri: provisioningUri(email, secret) };
    },

    async listUsers(ctx) {
      const session = sessionOf(ctx);
      const res = await pool.query<{
        user_id: string;
        email: string;
        role: string;
        status: string;
      }>("SELECT user_id, email, role, status FROM users WHERE org_id = $1 ORDER BY created_at", [
        session.org_id,
      ]);
      return res.rows.map((r) => ({
        userId: r.user_id,
        email: r.email,
        role: r.role,
        status: r.status,
      }));
    },

    assignRole(userId, role, ctx) {
      return changeUser(userId, { role }, ctx);
    },

    setStatus(userId, status, ctx) {
      return changeUser(userId, { status }, ctx);
    },

    /**
     * SPEC-008 §8 and Amendment 2026-10-16 item 2 (debt #100): limited on a per-user
     * counter of its own, a failure recorded, and the accepted code used up, so the
     * password change adds no faster way to try codes, and its code opens no session.
     */
    async changePassword(input, ctx) {
      if (!(await reservePasswordChange(redis, input.userId))) {
        throw new RateLimitError();
      }
      // The slots settle when the attempt fails (they are kept) or succeeds (they are
      // given back); an attempt that ends otherwise, in a store failure, gives them back
      // (Amendment 2026-10-16 item 3).
      let settled = false;
      try {
        const res = await pool.query<{
          org_id: string;
          password_hash: string;
          totp_secret: string;
        }>(
          `SELECT org_id, password_hash, pgp_sym_decrypt(totp_secret, $1)::text AS totp_secret
         FROM users WHERE user_id = $2`,
          [config.API_DB_ENC_PASSPHRASE, input.userId],
        );
        const u = res.rows[0];
        // A failure keeps its slots, restarts their windows and writes its row.
        const reject = async (): Promise<never> => {
          settled = true;
          await passwordChangeFailed(redis, input.userId);
          await appendAudit(pool, {
            userId: input.userId,
            orgId: ctx.session?.org_id ?? u?.org_id ?? ORG,
            action: "user.password.change",
            outcome: "denied",
            sourceIp: ctx.sourceIp,
            correlationId: ctx.correlationId,
          });
          throw new AuthError();
        };
        if (
          !u ||
          !(await verifyPassword(u.password_hash, input.currentPassword)) ||
          !verifyTotp(u.totp_secret, input.totp_code)
        ) {
          return reject();
        }
        // The accepted code is used up, as the login's is (§1.3).
        const firstUse = await redis.set(
          `totp:replay:${input.userId}:${input.totp_code}`,
          "1",
          "EX",
          90,
          "NX",
        );
        if (firstUse === null) return reject();
        const ph = await hashPassword(input.newPassword);
        await authorityChange(input.userId, async (db) => {
          await db.query(
            `UPDATE users
              SET password_hash = $2, authority_version = authority_version + 1, updated_at = now()
            WHERE user_id = $1`,
            [input.userId, ph],
          );
          await appendAudit(db, {
            userId: input.userId,
            orgId: ctx.session?.org_id ?? u.org_id,
            action: "user.password.change",
            outcome: "ok",
            sourceIp: ctx.sourceIp,
            correlationId: ctx.correlationId,
          });
        });
        settled = true;
        await releasePasswordChange(redis, input.userId);
      } catch (err) {
        if (!settled) await releasePasswordChange(redis, input.userId).catch(() => undefined);
        throw err;
      }
    },

    /**
     * SPEC-008 §8 and Amendment 2026-10-15 item 12: confirms only a user whose TOTP is
     * not yet confirmed, behind the login's limits, and marks the accepted code used, so
     * the confirmation is no way to try codes against an enrolled user.
     */
    async confirmTotpEnrollment(input, ctx) {
      if (!(await reserveConfirm(redis, input.userId, ctx.sourceIp))) {
        throw new RateLimitError();
      }
      // The slots settle when the attempt fails (they are kept) or succeeds (they are
      // given back); an attempt that ends otherwise, in a store failure, gives them back
      // (Amendment 2026-10-16 item 3).
      let settled = false;
      try {
        const res = await pool.query<{
          user_id: string;
          org_id: string;
          totp_enrolled: boolean;
          totp_secret: string;
        }>(
          `SELECT user_id, org_id, totp_enrolled, pgp_sym_decrypt(totp_secret, $1)::text AS totp_secret
         FROM users WHERE user_id = $2`,
          [config.API_DB_ENC_PASSPHRASE, input.userId],
        );
        const u = res.rows[0];
        const reject = async (): Promise<never> => {
          settled = true;
          await confirmFailed(redis, input.userId, ctx.sourceIp);
          await appendAudit(pool, {
            userId: u?.user_id ?? null,
            orgId: u?.org_id ?? ORG,
            action: "user.totp.enroll",
            outcome: "denied",
            sourceIp: ctx.sourceIp,
            correlationId: ctx.correlationId,
          });
          throw new AuthError();
        };
        if (!u || u.totp_enrolled) {
          // The same work as a wrong code (item 12): the decrypt and the TOTP check.
          if (!u) await decryptLikeARow();
          verifyTotp(constantWorkSecret, input.totp_code);
          return reject();
        }
        if (!verifyTotp(u.totp_secret, input.totp_code)) return reject();
        // The accepted code is used up, as the login's is: it cannot open a session.
        const firstUse = await redis.set(
          `totp:replay:${u.user_id}:${input.totp_code}`,
          "1",
          "EX",
          90,
          "NX",
        );
        if (firstUse === null) return reject();
        await pool.query(
          "UPDATE users SET totp_enrolled = true, updated_at = now() WHERE user_id = $1",
          [input.userId],
        );
        await appendAudit(pool, {
          userId: input.userId,
          orgId: u.org_id,
          action: "user.totp.enroll",
          outcome: "ok",
          sourceIp: ctx.sourceIp,
          correlationId: ctx.correlationId,
        });
        settled = true;
        await releaseConfirm(redis, input.userId, ctx.sourceIp);
      } catch (err) {
        if (!settled)
          await releaseConfirm(redis, input.userId, ctx.sourceIp).catch(() => undefined);
        throw err;
      }
    },
  };
}
