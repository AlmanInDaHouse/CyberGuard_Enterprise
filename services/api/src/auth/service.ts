import { createHash, randomBytes } from "node:crypto";
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
  invitationFailed,
  loginFailed,
  loginSucceeded,
  passwordChangeFailed,
  releaseConfirm,
  releaseInvitation,
  releaseLogin,
  releasePasswordChange,
  reserveConfirm,
  reserveInvitation,
  reserveLogin,
  reservePasswordChange,
} from "./ratelimit.js";
import {
  type SessionInfo,
  createSession,
  peekSession,
  resolveSession as resolveSessionRedis,
  revokeAllForUser,
  revokeSession,
  sessionLifetimes,
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
  role: string;
}
/** An invitation as the route that issues it answers it, once (SPEC-026 §Operational §3). */
export interface IssuedInvitation {
  token: string;
  expires_at: string;
}
export type UserStatus = "active" | "disabled";

/** What a request carries into the service: its id, its address, and its session. */
export interface RequestContext {
  correlationId: string;
  sourceIp: string;
  session?: SessionInfo;
}

export interface AuthService {
  /** `priorSession`, the token of the `cgsess` cookie the login carries: a successful
   *  login deletes it (SPEC-026 §Operational §2, debt #102). */
  login(input: LoginInput, ctx: RequestContext, priorSession?: string): Promise<LoginResult>;
  /** The session's user's id, email and role (SPEC-026 `GET /v1/auth/session`), or null
   *  when the session names no user of its org. */
  sessionUser(
    session: SessionInfo,
  ): Promise<{ user_id: string; email: string; role: string } | null>;
  logout(token: string, ctx: RequestContext): Promise<void>;
  resolveSession(token: string): Promise<SessionInfo | null>;
  /** SPEC-026 §Operational §3: a user with no credentials and an `invite` invitation. */
  createUser(
    input: CreateUserInput,
    ctx: RequestContext,
  ): Promise<{ user_id: string; invitation: IssuedInvitation }>;
  /** A new invitation for a user with no password and no confirmed factor. */
  issueInvitation(userId: string, ctx: RequestContext): Promise<IssuedInvitation>;
  withdrawInvitation(userId: string, ctx: RequestContext): Promise<void>;
  /** SPEC-026 §Operational §4: another active user's credentials cleared, and a reset
   *  invitation answered. */
  resetUser(userId: string, ctx: RequestContext): Promise<IssuedInvitation>;
  /** The invitation's holder: a new TOTP secret, its email and provisioning URI. */
  invitationTotp(
    input: { token: string },
    ctx: RequestContext,
  ): Promise<{ email: string; provisioning_uri: string }>;
  /** The invitation's holder: the password and the factor set, the invitation completed. */
  completeInvitation(
    input: { token: string; password: string; totp_code: string },
    ctx: RequestContext,
  ): Promise<void>;
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
  /** NULL, with `totp_secret`, for a user an invitation or a reset left without credentials
   *  (SPEC-026 §Data contracts §2). */
  password_hash: string | null;
  totp_enrolled: boolean;
  authority_version: string; // bigint, as pg returns it
  totp_secret: string | null; // decrypted base32 (pgp_sym_decrypt)
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

/** How long an invitation lives (SPEC-026 Decision record 8), a constant of the code. */
const INVITATION_HOURS = 72;

/** An invitation's text: 32 bytes from a cryptographic random source, in base64url. */
function newInvitationToken(): string {
  return randomBytes(32).toString("base64url");
}

/** The SHA-256 the api keeps of an invitation's text, its characters as ASCII (SPEC-026 §2). */
function invitationHash(token: string): Buffer {
  return createHash("sha256").update(token, "ascii").digest();
}

/** The one answer of an invitation the api refuses (SPEC-026 §Operational §3). */
function invitationRejected(): AuthError {
  return new AuthError("invitation rejected", "invitation_rejected");
}

/** What an invitation's refusal records: whom it was for, when the token named one. */
class InvitationRefusal extends Error {
  constructor(readonly who: { userId: string; orgId: string; purpose: string } | null) {
    super("invitation refused");
  }
}

export function createAuthService(rawPool: Pool, rawRedis: Redis, config: Config): AuthService {
  // Every Postgres and Redis call of the service fails as the store's: a 503.
  const pool = asStore(rawPool);
  const redis = asStore(rawRedis);
  const ORG = "default";
  // SPEC-023 §Data contracts §3: a session's idle lifetime and cap are settings.
  const lifetimes = sessionLifetimes(config);
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
   * recorded like any other. The last active admin counts only an admin with a password
   * and a confirmed factor (SPEC-026 §Data contracts §2), so that an admin whose
   * credentials were reset does not count.
   */
  async function changeUser(
    targetId: string,
    change: { role?: string; status?: UserStatus },
    ctx: RequestContext,
  ): Promise<void> {
    const session = sessionOf(ctx);
    await authorityChange(targetId, async (db) => {
      const locked = await db.query<{
        user_id: string;
        role: string;
        status: string;
        credentialed: boolean;
      }>(
        `SELECT user_id, role, status, (password_hash IS NOT NULL AND totp_enrolled) AS credentialed
           FROM users
          WHERE org_id = $1 AND ((role = 'admin' AND status = 'active') OR user_id = $2)
          ORDER BY user_id FOR UPDATE`,
        [session.org_id, targetId],
      );
      const target = locked.rows.find((r) => r.user_id === targetId);
      if (!target) throw new NotFoundError();
      const role = change.role ?? target.role;
      const status = change.status ?? target.status;
      const counts = (r: (typeof locked.rows)[number]) =>
        r.role === "admin" && r.status === "active" && r.credentialed;
      const countedAdmins = locked.rows.filter(counts);
      const removesAnAdmin = counts(target) && (role !== "admin" || status !== "active");
      if (removesAnAdmin && countedAdmins.length <= 1) throw new ConflictError();
      await db.query(
        `UPDATE users
            SET role = $2, status = $3, authority_version = authority_version + 1, updated_at = now()
          WHERE user_id = $1`,
        [targetId, role, status],
      );
      // SPEC-026 §Data contracts §3: a disable also withdraws the user's open invitation, with
      // its row, after the change's own row below.
      const disables = change.status === "disabled";
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
      if (disables) await withdrawOpen(db, targetId, session.org_id, ctx);
    });
  }

  /**
   * Withdraw a user's open invitation, if any, with its `invitation.withdraw` row, in the
   * caller's transaction, the user's row already locked. Its pending secret is cleared.
   */
  async function withdrawOpen(
    db: PoolClient,
    userId: string,
    orgId: string,
    ctx: RequestContext,
  ): Promise<boolean> {
    const open = await db.query<{ purpose: string }>(
      `UPDATE invitations SET withdrawn_at = now(), pending_totp_secret = NULL
        WHERE user_id = $1 AND completed_at IS NULL AND withdrawn_at IS NULL
        RETURNING purpose`,
      [userId],
    );
    const withdrawn = open.rows[0];
    if (!withdrawn) return false;
    await appendAudit(db, {
      userId: ctx.session?.user_id ?? null,
      orgId,
      action: "invitation.withdraw",
      targetId: userId,
      outcome: "ok",
      sourceIp: ctx.sourceIp,
      correlationId: ctx.correlationId,
      detail: { purpose: withdrawn.purpose },
    });
    return true;
  }

  /**
   * Issue an invitation for a user whose row the caller's transaction has locked (SPEC-026
   * §Operational §3): any open one is withdrawn, every earlier one's pending secret cleared,
   * and the new one written with the database's `now()` plus 72 hours, with its
   * `invitation.issue` row. Its text is answered once and kept nowhere.
   */
  async function issueFor(
    db: PoolClient,
    userId: string,
    orgId: string,
    purpose: "invite" | "reset",
    ctx: RequestContext,
  ): Promise<IssuedInvitation> {
    await withdrawOpen(db, userId, orgId, ctx);
    await db.query(
      `UPDATE invitations SET pending_totp_secret = NULL
        WHERE user_id = $1 AND pending_totp_secret IS NOT NULL`,
      [userId],
    );
    const token = newInvitationToken();
    const issued = await db.query<{ expires_at: Date }>(
      `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, expires_at)
       VALUES ($1, $2, $3, $4, $5, now() + make_interval(hours => $6))
       RETURNING expires_at`,
      [
        globalThis.crypto.randomUUID(),
        userId,
        orgId,
        purpose,
        invitationHash(token),
        INVITATION_HOURS,
      ],
    );
    const expiresAt = issued.rows[0]?.expires_at;
    if (!expiresAt) throw new Error("unreachable: INSERT ... RETURNING returned no row");
    const expires_at = expiresAt.toISOString();
    await appendAudit(db, {
      userId: ctx.session?.user_id ?? null,
      orgId,
      action: "invitation.issue",
      targetId: userId,
      outcome: "ok",
      sourceIp: ctx.sourceIp,
      correlationId: ctx.correlationId,
      detail: { purpose, expires_at },
    });
    return { token, expires_at };
  }

  /**
   * Lock, in the caller's transaction, the user an invitation's token names and then the
   * invitation (users before invitations, the order every writer of both takes), and check it
   * as SPEC-026 §Operational §3 says: known, neither completed nor withdrawn, not expired, its
   * user active. Any other is an InvitationRefusal, naming the user when the token named one.
   */
  async function lockLiveInvitation(
    db: PoolClient,
    hash: Buffer,
  ): Promise<{
    invitationId: string;
    userId: string;
    orgId: string;
    email: string;
    purpose: string;
    pendingSecret: string | null;
  }> {
    const named = await db.query<{ user_id: string }>(
      "SELECT user_id FROM invitations WHERE token_sha256 = $1",
      [hash],
    );
    const userId = named.rows[0]?.user_id;
    if (!userId) throw new InvitationRefusal(null);
    const user = await db.query<{ email: string; status: string }>(
      "SELECT email, status FROM users WHERE user_id = $1 FOR UPDATE",
      [userId],
    );
    const inv = await db.query<{
      invitation_id: string;
      org_id: string;
      purpose: string;
      live: boolean;
      pending: string | null;
    }>(
      `SELECT invitation_id, org_id, purpose,
              (completed_at IS NULL AND withdrawn_at IS NULL AND expires_at > now()) AS live,
              pgp_sym_decrypt(pending_totp_secret, $2)::text AS pending
         FROM invitations WHERE token_sha256 = $1 FOR UPDATE`,
      [hash, config.API_DB_ENC_PASSPHRASE],
    );
    const u = user.rows[0];
    const i = inv.rows[0];
    if (!u || !i) throw new InvitationRefusal(null);
    const who = { userId, orgId: i.org_id, purpose: i.purpose };
    if (!i.live || u.status !== "active") throw new InvitationRefusal(who);
    return {
      invitationId: i.invitation_id,
      userId,
      orgId: i.org_id,
      email: u.email,
      purpose: i.purpose,
      pendingSecret: i.pending,
    };
  }

  /**
   * Run one of the invitation's two routes behind its limits (SPEC-026 §Operational §3): the
   * invitation's own counter and the source address's, reserved before any work (SPEC-008
   * Amendment 2026-10-16 item 3); a refusal keeps them and answers `invitation_rejected`, a
   * success gives them back, and any other end gives them back.
   */
  async function behindInvitationLimits<T>(
    token: string,
    ctx: RequestContext,
    work: (hash: Buffer) => Promise<T>,
    onRefusal: (who: InvitationRefusal["who"]) => Promise<void>,
  ): Promise<T> {
    const hash = invitationHash(token);
    const key = hash.toString("hex");
    if (!(await reserveInvitation(redis, key, ctx.sourceIp))) throw new RateLimitError();
    let settled = false;
    try {
      const result = await work(hash);
      settled = true;
      await releaseInvitation(redis, key, ctx.sourceIp);
      return result;
    } catch (err) {
      if (!(err instanceof InvitationRefusal)) {
        if (!settled) await releaseInvitation(redis, key, ctx.sourceIp).catch(() => undefined);
        throw err;
      }
      settled = true;
      await invitationFailed(redis, key, ctx.sourceIp);
      await onRefusal(err.who);
      throw invitationRejected();
    }
  }

  return {
    async login(input, ctx, priorSession) {
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
        if (!user || user.status !== "active" || user.password_hash === null) {
          // The same work as a wrong password (Amendment 2026-10-15 item 11): the decrypt
          // a found row costs, and the Argon2 verification. A user with no credentials, an
          // invitation's or a reset's, does the unknown address's (SPEC-026 §Data
          // contracts §2): its row had no secret to decrypt.
          if (!user || user.password_hash === null) await decryptLikeARow();
          await verifyPassword(await constantWorkHash(), input.password);
          return reject();
        }
        if (!(await verifyPassword(user.password_hash, input.password))) return reject();
        if (!user.totp_enrolled || user.totp_secret === null) return reject();
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

        const { token, csrfToken } = await createSession(
          redis,
          {
            user_id: user.user_id,
            org_id: user.org_id,
            role: user.role,
          },
          lifetimes,
        );
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
        // SPEC-026 §Operational §2 (#102): every check has passed and the new session is
        // written, so the session the cookie named ends, whatever user it belonged to,
        // before the login answers. Its `auth.login.ok` is the record.
        if (priorSession && priorSession !== token) await revokeSession(redis, priorSession);
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

    async sessionUser(session) {
      const found = await pool.query<{ email: string }>(
        "SELECT email FROM users WHERE user_id = $1 AND org_id = $2",
        [session.user_id, session.org_id],
      );
      const row = found.rows[0];
      return row ? { user_id: session.user_id, email: row.email, role: session.role } : null;
    },

    async logout(token, ctx) {
      const sess = await peekSession(redis, token);
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
      return resolveSessionRedis(redis, token, Date.now(), lifetimes);
    },

    async createUser(input, ctx) {
      const session = sessionOf(ctx);
      const email = normalizeEmail(input.email);
      const userId = globalThis.crypto.randomUUID();
      try {
        const invitation = await inTransaction(async (db) => {
          // No password and no secret: the invitation's holder chooses the one and enrolls the
          // other (SPEC-026 §Operational §3, debt #98).
          await db.query(
            `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
             VALUES ($1, $2, $3, NULL, NULL, false, $4)`,
            [userId, session.org_id, email, input.role],
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
          return issueFor(db, userId, session.org_id, "invite", ctx);
        });
        return { user_id: userId, invitation };
      } catch (err) {
        if (isUniqueViolation(err)) throw new ConflictError();
        throw err;
      }
    },

    async issueInvitation(userId, ctx) {
      const session = sessionOf(ctx);
      return inTransaction(async (db) => {
        const found = await db.query<{ status: string; credentialed: boolean }>(
          `SELECT status, (password_hash IS NOT NULL OR totp_enrolled) AS credentialed
             FROM users WHERE user_id = $1 AND org_id = $2 FOR UPDATE`,
          [userId, session.org_id],
        );
        const user = found.rows[0];
        if (!user) throw new NotFoundError();
        if (user.status !== "active") throw new ConflictError("user disabled", "user_disabled");
        // A user with a password or a confirmed factor is reached by the reset alone.
        if (user.credentialed) {
          throw new ConflictError("user has credentials", "user_has_credentials");
        }
        return issueFor(db, userId, session.org_id, "invite", ctx);
      });
    },

    async withdrawInvitation(userId, ctx) {
      const session = sessionOf(ctx);
      await inTransaction(async (db) => {
        const found = await db.query(
          "SELECT 1 FROM users WHERE user_id = $1 AND org_id = $2 FOR UPDATE",
          [userId, session.org_id],
        );
        if (found.rowCount === 0) throw new NotFoundError();
        if (!(await withdrawOpen(db, userId, session.org_id, ctx))) {
          throw new ConflictError("no open invitation", "no_open_invitation");
        }
      });
    },

    /**
     * SPEC-026 §Operational §4 (ADR-0014 Amendment 2026-11-20): in one transaction, with
     * `user.reset`, `invitation.issue` and, when it replaces one, `invitation.withdraw`, the
     * user's password and secret are cleared, the factor unconfirmed, the authority version
     * raised, any open invitation withdrawn and a `reset` one issued; after the commit the
     * user's sessions are deleted, as a disable's are. An admin's own reset and a disabled
     * user's are refused, and change nothing.
     */
    async resetUser(userId, ctx) {
      const session = sessionOf(ctx);
      if (userId === session.user_id) {
        throw new ConflictError("cannot reset self", "cannot_reset_self");
      }
      let issued: IssuedInvitation | undefined;
      await authorityChange(userId, async (db) => {
        const found = await db.query<{ status: string }>(
          "SELECT status FROM users WHERE user_id = $1 AND org_id = $2 FOR UPDATE",
          [userId, session.org_id],
        );
        const user = found.rows[0];
        if (!user) throw new NotFoundError();
        if (user.status !== "active") throw new ConflictError("user disabled", "user_disabled");
        await db.query(
          `UPDATE users
              SET password_hash = NULL, totp_secret = NULL, totp_enrolled = false,
                  authority_version = authority_version + 1, updated_at = now()
            WHERE user_id = $1`,
          [userId],
        );
        await appendAudit(db, {
          userId: session.user_id,
          orgId: session.org_id,
          action: "user.reset",
          targetId: userId,
          outcome: "ok",
          sourceIp: ctx.sourceIp,
          correlationId: ctx.correlationId,
        });
        issued = await issueFor(db, userId, session.org_id, "reset", ctx);
      });
      if (!issued) throw new Error("unreachable: the reset committed without its invitation");
      return issued;
    },

    async invitationTotp(input, ctx) {
      return behindInvitationLimits(
        input.token,
        ctx,
        (hash) =>
          inTransaction(async (db) => {
            const inv = await lockLiveInvitation(db, hash);
            // A new secret at each request; it replaces any earlier one (Decision record 6).
            const secret = generateSecret();
            await db.query(
              `UPDATE invitations SET pending_totp_secret = pgp_sym_encrypt($2, $3)
                WHERE invitation_id = $1`,
              [inv.invitationId, secret, config.API_DB_ENC_PASSPHRASE],
            );
            await appendAudit(db, {
              userId: inv.userId,
              orgId: inv.orgId,
              action: "invitation.totp",
              targetId: inv.userId,
              outcome: "ok",
              sourceIp: ctx.sourceIp,
              correlationId: ctx.correlationId,
              detail: { purpose: inv.purpose },
            });
            return { email: inv.email, provisioning_uri: provisioningUri(inv.email, secret) };
          }),
        // A refused request for a secret writes no row (SPEC-026 §Data contracts §4).
        async () => {},
      );
    },

    async completeInvitation(input, ctx) {
      await behindInvitationLimits(
        input.token,
        ctx,
        (hash) =>
          inTransaction(async (db) => {
            const inv = await lockLiveInvitation(db, hash);
            const who = { userId: inv.userId, orgId: inv.orgId, purpose: inv.purpose };
            // The code against the secret last shown, marked used as a login's is.
            if (inv.pendingSecret === null || !verifyTotp(inv.pendingSecret, input.totp_code)) {
              throw new InvitationRefusal(who);
            }
            const firstUse = await redis.set(
              `totp:replay:${inv.userId}:${input.totp_code}`,
              "1",
              "EX",
              90,
              "NX",
            );
            if (firstUse === null) throw new InvitationRefusal(who);
            const ph = await hashPassword(input.password);
            await db.query(
              `UPDATE users
                  SET password_hash = $2,
                      totp_secret = (SELECT pending_totp_secret FROM invitations WHERE invitation_id = $3),
                      totp_enrolled = true,
                      authority_version = authority_version + 1,
                      updated_at = now()
                WHERE user_id = $1`,
              [inv.userId, ph, inv.invitationId],
            );
            await db.query(
              `UPDATE invitations SET completed_at = now(), pending_totp_secret = NULL
                WHERE invitation_id = $1`,
              [inv.invitationId],
            );
            await appendAudit(db, {
              userId: inv.userId,
              orgId: inv.orgId,
              action: "invitation.complete",
              targetId: inv.userId,
              outcome: "ok",
              sourceIp: ctx.sourceIp,
              correlationId: ctx.correlationId,
              detail: { purpose: inv.purpose },
            });
          }),
        // A failure writes its denied row, naming the invitation's user when the token named
        // one (SPEC-026 §Operational §3), after the transaction rolled back.
        async (who) => {
          await appendAudit(pool, {
            userId: who?.userId ?? null,
            orgId: who?.orgId ?? ORG,
            action: "invitation.complete",
            targetId: who?.userId ?? null,
            outcome: "denied",
            sourceIp: ctx.sourceIp,
            correlationId: ctx.correlationId,
            detail: who ? { purpose: who.purpose } : null,
          });
        },
      );
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
          password_hash: string | null;
          totp_secret: string | null;
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
        // A user with no credentials (SPEC-026 §Data contracts §2) has none to prove.
        if (
          !u ||
          u.password_hash === null ||
          u.totp_secret === null ||
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
          totp_secret: string | null;
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
        if (!u || u.totp_enrolled || u.totp_secret === null) {
          // The same work as a wrong code (item 12): the decrypt and the TOTP check. A user
          // with no secret is refused as an unknown one is (SPEC-026 §Operational §3).
          if (!u || u.totp_secret === null) await decryptLikeARow();
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
