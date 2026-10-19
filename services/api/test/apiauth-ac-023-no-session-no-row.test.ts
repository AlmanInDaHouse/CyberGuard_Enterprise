import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { seedSession } from "./helpers/db.js";

// apiauth_ac_023 — SPEC-008 Amendment 2026-10-19: a request the session check refuses
// (`401`) is not an authentication event, and writes no audit_log row (debt #80's `401`
// half). The request with no cookie is apiauth_ac_019's case. Each case pins the
// behavior as built: the api already writes no row for any of them.

const HOUR = 60 * 60 * 1000;

let config: Config;
let redis: Redis;
let pool: pg.Pool;
let app: FastifyInstance;
let closeServices: () => Promise<void>;
let closed = false;
/** audit_log's row count when the file starts. */
let initial: number;

beforeAll(async () => {
  config = inject("apiConfig");
  redis = new Redis(config.API_REDIS_URL);
  pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const services = await buildServices(config);
  closeServices = services.close;
  app = buildApp(services);
  await app.ready();
  initial = await auditRows();
});
afterAll(async () => {
  if (!closed) {
    await app?.close();
    await closeServices?.();
  }
  await pool?.end();
  await redis?.quit();
});

const key = (token: string) => `cgsess:${token}`;

async function auditRows(): Promise<number> {
  return (await pool.query("SELECT count(*)::int AS n FROM audit_log")).rows[0].n;
}

async function seeded(token: string, issuedAt: string): Promise<void> {
  await seedSession(config, {
    token,
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: "ac023-org",
    issuedAt,
  });
}

/** A request to a session-guarded route, which every role may make, answers `401` and
 *  leaves audit_log as it was. */
async function refusedWithNoRow(token: string): Promise<void> {
  const before = await auditRows();
  const res = await app.inject({ method: "GET", url: "/v1/incidents", cookies: { cgsess: token } });
  expect(res.statusCode).toBe(401);
  expect(await auditRows()).toBe(before);
}

describe("apiauth_ac_023 — a request with no session writes no row", () => {
  test("a token that names no entry", async () => {
    expect(await redis.exists(key("tok-023-none"))).toBe(0);
    await refusedWithNoRow("tok-023-none");
  });

  test("an entry issued 24 h and one second ago, which is then gone", async () => {
    await seeded("tok-023-past", new Date(Date.now() - 24 * HOUR - 1000).toISOString());
    await refusedWithNoRow("tok-023-past");
    expect(await redis.exists(key("tok-023-past"))).toBe(0);
  });

  test("an entry whose issued_at is not a time", async () => {
    await seeded("tok-023-garbled", "not a time");
    await refusedWithNoRow("tok-023-garbled");
  });

  test("an entry with no issued_at", async () => {
    await seeded("tok-023-unissued", new Date().toISOString());
    const { issued_at: _, ...rest } = JSON.parse(
      (await redis.get(key("tok-023-unissued"))) ?? "{}",
    );
    await redis.set(key("tok-023-unissued"), JSON.stringify(rest), "KEEPTTL");
    await refusedWithNoRow("tok-023-unissued");
  });

  test("an entry that is not a JSON object", async () => {
    const entries: [string, string][] = [
      ["tok-023-text", "not json"],
      ["tok-023-null", "null"],
    ];
    for (const [token, value] of entries) {
      await redis.set(key(token), value, "EX", 3600);
      await refusedWithNoRow(token);
    }
  });

  test("no row lands after the responses either", async () => {
    // A write the api did not wait for would land after its response, and so after the
    // count above; closing the api and its pool waits for every query it sent.
    closed = true;
    await app.close();
    await closeServices();
    expect(await auditRows()).toBe(initial);
  });
});
