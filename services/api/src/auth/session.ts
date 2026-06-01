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
const TTL_SECONDS = 8 * 60 * 60; // NFR-008-002 idle lifetime

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
  await redis.set(`${PREFIX}${token}`, payload, "EX", TTL_SECONDS);
  return { token, csrfToken };
}

export async function resolveSession(redis: Redis, token: string): Promise<SessionInfo | null> {
  if (!token) return null;
  const raw = await redis.get(`${PREFIX}${token}`);
  if (!raw) return null;
  const p = JSON.parse(raw) as SessionInfo;
  return { user_id: p.user_id, org_id: p.org_id, role: p.role, csrf_token: p.csrf_token };
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
