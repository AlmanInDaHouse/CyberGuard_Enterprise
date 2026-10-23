import type { Redis } from "ioredis";
import type pg from "pg";

/** Where a store's lost connection is logged (pino's `Logger` satisfies it). */
export interface StoreLog {
  warn(obj: Record<string, unknown>, msg: string): void;
  info(obj: Record<string, unknown>, msg: string): void;
}

/**
 * Listen to the pool's lost connections, so that none ends the process: a client's
 * connection that ends, Postgres restarting or unreachable, is an `error` event of the
 * client, idle or checked out, and of the pool when the client was idle; without a listener
 * each would be an uncaught exception. Each is logged once, from the client's listener; the
 * query that needed the connection fails where it is made and answers 503.
 */
export function watchPool(pool: pg.Pool, log?: StoreLog): void {
  pool.on("connect", (client) => {
    client.on("error", (err) =>
      log?.warn({ event: "pg_connection_lost", error: err.message }, "postgres connection lost"),
    );
  });
  // The pool re-emits an idle client's error, which the client's listener has logged.
  pool.on("error", () => {});
}

/**
 * Listen to the Redis client's connection errors, which ioredis emits at each failed
 * reconnection, and would print as unhandled: logged once when Redis becomes unreachable and
 * once when it is reachable again; the commands' failures reach their callers.
 */
export function watchRedis(redis: Redis, log?: StoreLog): void {
  let down = false;
  redis.on("error", (err) => {
    if (down) return;
    down = true;
    log?.warn({ event: "redis_unreachable", error: err.message }, "redis unreachable");
  });
  redis.on("ready", () => {
    if (!down) return;
    down = false;
    log?.info({ event: "redis_reachable" }, "redis reachable again");
  });
}
