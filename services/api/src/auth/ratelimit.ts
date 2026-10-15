import type { Redis } from "ioredis";

/**
 * SPEC-008 §Operational §3 — login rate-limit on two independent dimensions
 * (per account + per source IP), Redis counters with a sliding window (ADR-0003
 * §Decision: rate-limit → Redis). Thresholds are NFR-008-004 (per-org
 * configurable in a later increment). A real counter gate — not simulated: an
 * attempt reserves its slots BEFORE credential verification, and a failure keeps
 * them (auth_ac_004; SPEC-008 Amendment 2026-10-16 item 3).
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

/** The password change's per-user counter, its own (SPEC-008 Amendment 2026-10-16
 *  item 2): neither the TOTP confirmation's nor the login's per-IP counter, which a
 *  request with no session can fill. */
function passwordKey(userId: string): string {
  return `rl:pw:${userId}`;
}

/**
 * SPEC-008 Amendment 2026-10-16 item 3 (debt #101): an attempt first reserves one slot on
 * each of its counters, in one script, so that no burst of parallel attempts passes a
 * threshold. The script checks every counter against its threshold and, only if none has
 * reached it, increments each, starting the window of a counter it creates. A refused
 * attempt reserves nothing and changes no window. KEYS are the counters; ARGV[1] is the
 * window, ARGV[1 + i] the threshold of KEYS[i].
 */
const RESERVE = `
for i, key in ipairs(KEYS) do
  if tonumber(redis.call('GET', key) or '0') >= tonumber(ARGV[i + 1]) then return 0 end
end
for _, key in ipairs(KEYS) do
  if redis.call('INCR', key) == 1 then redis.call('EXPIRE', key, ARGV[1]) end
end
return 1
`;

/** A failure keeps its slots and restarts their windows, as a failure did before; a
 *  counter whose window ended while the attempt was in flight is counted afresh at 1. */
const FAILED = `
for _, key in ipairs(KEYS) do
  if redis.call('EXISTS', key) == 1 then
    redis.call('EXPIRE', key, ARGV[1])
  else
    redis.call('SET', key, 1, 'EX', ARGV[1])
  end
end
return 1
`;

/** A success gives one slot back on each counter, and removes a counter that reaches 0. */
const RELEASE = `
for _, key in ipairs(KEYS) do
  if tonumber(redis.call('GET', key) or '0') > 0 then
    if redis.call('DECR', key) <= 0 then redis.call('DEL', key) end
  end
end
return 1
`;

async function reserve(redis: Redis, counters: [key: string, max: number][]): Promise<boolean> {
  const result = await redis.eval(
    RESERVE,
    counters.length,
    ...counters.map(([key]) => key),
    WINDOW_SECONDS,
    ...counters.map(([, max]) => max),
  );
  return result === 1;
}

async function failed(redis: Redis, keys: string[]): Promise<void> {
  await redis.eval(FAILED, keys.length, ...keys, WINDOW_SECONDS);
}

async function release(redis: Redis, keys: string[]): Promise<void> {
  await redis.eval(RELEASE, keys.length, ...keys);
}

/** The login's slots: the account's and the address's. */
export function reserveLogin(redis: Redis, org: string, email: string, ip: string) {
  return reserve(redis, [
    [acctKey(org, email), ACCOUNT_MAX],
    [ipKey(ip), IP_MAX],
  ]);
}

export function loginFailed(redis: Redis, org: string, email: string, ip: string) {
  return failed(redis, [acctKey(org, email), ipKey(ip)]);
}

/** A login that keeps its session clears the account's failures and gives the address's
 *  slot back. */
export async function loginSucceeded(redis: Redis, org: string, email: string, ip: string) {
  await redis.del(acctKey(org, email));
  await release(redis, [ipKey(ip)]);
}

/** A login that counts no failure (item 9's race) gives both slots back. */
export function releaseLogin(redis: Redis, org: string, email: string, ip: string) {
  return release(redis, [acctKey(org, email), ipKey(ip)]);
}

/** The TOTP confirmation's slots: the user's own counter and the login's per-IP one. */
export function reserveConfirm(redis: Redis, userId: string, ip: string) {
  return reserve(redis, [
    [userKey(userId), ACCOUNT_MAX],
    [ipKey(ip), IP_MAX],
  ]);
}

export function confirmFailed(redis: Redis, userId: string, ip: string) {
  return failed(redis, [userKey(userId), ipKey(ip)]);
}

export function releaseConfirm(redis: Redis, userId: string, ip: string) {
  return release(redis, [userKey(userId), ipKey(ip)]);
}

/** The password change's slot, on its own per-user counter alone: it needs a session, so
 *  it shares no counter that a request with no session fills. */
export function reservePasswordChange(redis: Redis, userId: string) {
  return reserve(redis, [[passwordKey(userId), ACCOUNT_MAX]]);
}

export function passwordChangeFailed(redis: Redis, userId: string) {
  return failed(redis, [passwordKey(userId)]);
}

export function releasePasswordChange(redis: Redis, userId: string) {
  return release(redis, [passwordKey(userId)]);
}
