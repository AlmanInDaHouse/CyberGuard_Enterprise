import type { Redis } from "ioredis";

/**
 * SPEC-008 §Data contracts §3 / ADR-0014 §2 — opaque server-side sessions in
 * Redis. The token is a high-entropy CSPRNG value (no client-trusted claims); all
 * authority is read server-side per request. Revocation is an immediate `DEL` —
 * the product invariant (auth_ac_005). Rotation re-issues a new token and deletes
 * the old (login + privilege change).
 */
export interface SessionInfo {
  user_id: string;
  org_id: string;
  role: string;
  csrf_token: string;
}

const PREFIX = "cgsess:";
/** NFR-008-002 and SPEC-008's second Amendment 2026-10-16: a session ends after 8 h
 *  without use, and 24 h after its login at the latest. */
export const SESSION_IDLE_MS = 8 * 60 * 60 * 1000;
export const SESSION_CAP_MS = 24 * 60 * 60 * 1000;

function randomToken(): string {
  return Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString("base64url");
}

export async function createSession(
  redis: Redis,
  info: { user_id: string; org_id: string; role: string },
): Promise<{ token: string; csrfToken: string }> {
  const token = randomToken();
  const csrfToken = randomToken();
  const now = new Date().toISOString();
  const payload = JSON.stringify({
    user_id: info.user_id,
    org_id: info.org_id,
    role: info.role,
    csrf_token: csrfToken,
    issued_at: now,
    last_seen: now,
  });
  await redis.set(`${PREFIX}${token}`, payload, "PX", SESSION_IDLE_MS);
  return { token, csrfToken };
}

/**
 * The session a token names, or null. Resolving it is a use (SPEC-008's second
 * Amendment 2026-10-16): the entry is rewritten with `last_seen` now and 8 h more,
 * within 24 h of `issued_at`, which never changes. A session past its cap, or whose
 * entry or `issued_at` cannot be read, is deleted and is no session. The rewrite is `XX`: a
 * revocation that deletes the entry between the read and the rewrite stays done
 * (auth_ac_005). A Redis error propagates, as the session check's store failure.
 */
export async function resolveSession(
  redis: Redis,
  token: string,
  now: number = Date.now(),
): Promise<SessionInfo | null> {
  if (!token) return null;
  const key = `${PREFIX}${token}`;
  const raw = await redis.get(key);
  if (!raw) return null;
  const p = entryOf(raw);
  const left =
    typeof p?.issued_at === "string" ? Date.parse(p.issued_at) + SESSION_CAP_MS - now : Number.NaN;
  if (p === null || !(left > 0)) {
    await redis.del(key);
    return null;
  }
  await redis.set(
    key,
    JSON.stringify({ ...p, last_seen: new Date(now).toISOString() }),
    "PX",
    Math.min(SESSION_IDLE_MS, left),
    "XX",
  );
  return { user_id: p.user_id, org_id: p.org_id, role: p.role, csrf_token: p.csrf_token };
}

/** The entry a token names, read without a use: for the logout, whose session check
 *  has already used it. */
export async function peekSession(redis: Redis, token: string): Promise<SessionInfo | null> {
  if (!token) return null;
  const raw = await redis.get(`${PREFIX}${token}`);
  const p = raw ? entryOf(raw) : null;
  return p && { user_id: p.user_id, org_id: p.org_id, role: p.role, csrf_token: p.csrf_token };
}

type Entry = SessionInfo & { issued_at?: unknown };

/** A stored entry, or null when it is not a JSON object. */
function entryOf(raw: string): Entry | null {
  try {
    const value: unknown = JSON.parse(raw);
    return typeof value === "object" && value !== null && !Array.isArray(value)
      ? (value as Entry)
      : null;
  } catch {
    return null;
  }
}

/** Immediate revocation (auth_ac_005): a deleted entry fails the next request. */
export async function revokeSession(redis: Redis, token: string): Promise<void> {
  if (token) await redis.del(`${PREFIX}${token}`);
}

/** Rotation on privilege change: drop every session a user holds. */
export async function revokeAllForUser(redis: Redis, userId: string): Promise<void> {
  let cursor = "0";
  do {
    const [next, keys] = await redis.scan(cursor, "MATCH", `${PREFIX}*`, "COUNT", 200);
    cursor = next;
    for (const k of keys) {
      const raw = await redis.get(k);
      if (raw && (JSON.parse(raw) as SessionInfo).user_id === userId) {
        await redis.del(k);
      }
    }
  } while (cursor !== "0");
}
