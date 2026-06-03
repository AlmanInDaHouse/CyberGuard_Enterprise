import { GenericContainer, type StartedTestContainer, Wait } from "testcontainers";
import type { Config } from "../../src/config.js";
import { runMigrations } from "../../src/db/migrate.js";
import { applyReadSchema } from "./read-schema.js";

export interface Backends {
  config: Config;
  stop: () => Promise<void>;
}

/**
 * Start real Postgres + Redis via testcontainers (same image pins as
 * docker-compose.dev.yml), run the api-owned migrations, and return a Config
 * pointing at them. auth-core needs no ClickHouse and no agent — Postgres
 * (users/audit_log) + Redis (sessions/rate-limit) only (ADR-0003 §Decision).
 */
export async function startBackends(): Promise<Backends> {
  const started: StartedTestContainer[] = [];
  try {
    const pg = await new GenericContainer("pgvector/pgvector:pg16")
      .withEnvironment({
        POSTGRES_USER: "cyberguard",
        POSTGRES_PASSWORD: "cyberguard_dev",
        POSTGRES_DB: "cyberguard",
      })
      .withExposedPorts(5432)
      .withWaitStrategy(Wait.forLogMessage(/database system is ready to accept connections/, 2))
      .start();
    started.push(pg);

    const redis = await new GenericContainer("redis:7.4-alpine")
      .withCommand(["redis-server", "--requirepass", "cyberguard_dev"])
      .withExposedPorts(6379)
      .withWaitStrategy(Wait.forLogMessage(/Ready to accept connections/))
      .start();
    started.push(redis);

    const config: Config = {
      API_PG_URL: `postgres://cyberguard:cyberguard_dev@${pg.getHost()}:${pg.getMappedPort(5432)}/cyberguard`,
      API_REDIS_URL: `redis://:cyberguard_dev@${redis.getHost()}:${redis.getMappedPort(6379)}`,
      API_DB_ENC_PASSPHRASE: "test-api-passphrase",
      API_PORT: 0,
      API_RUN_MIGRATIONS: true,
      API_LOG_LEVEL: "warn",
    };

    await runMigrations(config);
    // SPEC-009 read-slice — materialise the ingest-owned read-target tables
    // (agents/alerts/incidents) the read-API reads, by applying ingest's REAL
    // Postgres migrations in-workspace (no ClickHouse; see read-schema.ts). The (b)
    // architecture debt — a test-only DDL mirror (option (c)) — is RETIRED here.
    await applyReadSchema(config);

    return {
      config,
      async stop() {
        await Promise.allSettled(started.map((c) => c.stop()));
      },
    };
  } catch (err) {
    await Promise.allSettled(started.map((c) => c.stop()));
    throw err;
  }
}
