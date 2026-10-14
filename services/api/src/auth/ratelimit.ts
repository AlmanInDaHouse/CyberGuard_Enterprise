import type { Redis } from "ioredis";
import { StoreUnavailableError } from "../errors.js";

/**
 * A transaction's EXEC resolves with each command's error in its results rather than
 * rejecting (a WRONGTYPE, say), so the results are read: any error is the store's, a
 * 503 (SPEC-008 Amendment 2026-10-15 item 14).
 */
async function execOrFail(tx: ReturnType<Redis["multi"]>): Promise<void> {
  const results = await tx.exec();
  const failed = results?.find(([err]) => err);
  if (results === null || failed) {
    throw new StoreUnavailableError(failed?.[0] ?? new Error("the transaction was discarded"));
  }
}

/**
 * SPEC-008 §Operational §3 — login rate-limit on two independent dimensions
 * (per account + per source IP), Redis counters with a sliding window (ADR-0003
 * §Decision: rate-limit → Redis). Thresholds are NFR-008-004 (per-org
 * configurable in a later increment). A real counter gate — not simulated:
 * `isAllowed` is checked BEFORE credential verification, and a failure
 * increments both counters (auth_ac_004).
 */
const ACCOUNT_MAX = 5;
const IP_MAX = 10;
const WINDOW_SECONDS = 15 * 60;

function acctKey(org: string, email: string): string {
  return `rl:acct:${org}:${email}`;
}
function ipKey(ip: string): string {
  return `rl:ip:${ip}`;
}
/** The TOTP confirmation's per-user counter (SPEC-008 Amendment 2026-10-15 item 12). */
function userKey(userId: string): string {
  return `rl:user:${userId}`;
}

/** The confirmation's gate: the user's own counter, and the login's per-IP counter. */
export async function isConfirmAllowed(redis: Redis, userId: string, ip: string): Promise<boolean> {
  const [u, i] = await Promise.all([redis.get(userKey(userId)), redis.get(ipKey(ip))]);
  return Number(u ?? 0) < ACCOUNT_MAX && Number(i ?? 0) < IP_MAX;
}

export async function recordConfirmFailure(
  redis: Redis,
  userId: string,
  ip: string,
): Promise<void> {
  const tx = redis.multi();
  tx.incr(userKey(userId)).expire(userKey(userId), WINDOW_SECONDS);
  tx.incr(ipKey(ip)).expire(ipKey(ip), WINDOW_SECONDS);
  await tx.exec();
}

export async function isAllowed(
  redis: Redis,
  org: string,
  email: string,
  ip: string,
): Promise<boolean> {
  const [a, i] = await Promise.all([redis.get(acctKey(org, email)), redis.get(ipKey(ip))]);
  return Number(a ?? 0) < ACCOUNT_MAX && Number(i ?? 0) < IP_MAX;
}

export async function recordFailure(
  redis: Redis,
  org: string,
  email: string,
  ip: string,
): Promise<void> {
  const tx = redis.multi();
  tx.incr(acctKey(org, email)).expire(acctKey(org, email), WINDOW_SECONDS);
  tx.incr(ipKey(ip)).expire(ipKey(ip), WINDOW_SECONDS);
  await execOrFail(tx);
}

/** A successful login clears the account's consecutive-failure counter. */
export async function resetAccount(redis: Redis, org: string, email: string): Promise<void> {
  await redis.del(acctKey(org, email));
}
