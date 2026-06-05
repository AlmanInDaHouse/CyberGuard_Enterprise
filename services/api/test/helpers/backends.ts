import { GenericContainer, type StartedTestContainer, Wait } from "testcontainers";
import type { Config } from "../../src/config.js";
import { runMigrations } from "../../src/db/migrate.js";
import { bootstrapEventsTable } from "./events-schema.js";
import { applyReadSchema } from "./read-schema.js";

export interface Backends {
  config: Config;
  stop: () => Promise<void>;
}

/**
 * Start real Postgres + Redis + ClickHouse via testcontainers (same image pins as
 * docker-compose.dev.yml), run the api-owned migrations, materialise the ingest-owned
 * read-target tables, and return a Config pointing at them.
 *
 * ClickHouse joined the api harness for SPEC-010 (the forensic event drill): the api
 * acquired a read-only `cges_events` reader (ADR-0015). The auth-core + the three
 * SPEC-009 Postgres reads still touch only Postgres + Redis; ClickHouse is exercised
 * only by the drill ACs. The CH testcontainer is the same CI-able pattern
 * services/ingest already uses on Linux (services/ingest/test/helpers/backends.ts).
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

    const ch = await new GenericContainer("clickhouse/clickhouse-server:24.8")
      .withEnvironment({
        CLICKHOUSE_USER: "cyberguard",
        CLICKHOUSE_PASSWORD: "cyberguard_dev",
        CLICKHOUSE_DB: "cyberguard",
      })
      .withExposedPorts(8123)
      .withWaitStrategy(Wait.forHttp("/ping", 8123).forStatusCode(200))
      .start();
    started.push(ch);

    const config: Config = {
      API_PG_URL: `postgres://cyberguard:cyberguard_dev@${pg.getHost()}:${pg.getMappedPort(5432)}/cyberguard`,
      API_REDIS_URL: `redis://:cyberguard_dev@${redis.getHost()}:${redis.getMappedPort(6379)}`,
      API_CH_URL: `http://${ch.getHost()}:${ch.getMappedPort(8123)}`,
      API_CH_USER: "cyberguard",
      API_CH_PASSWORD: "cyberguard_dev",
      API_CH_DB: "cyberguard",
      API_DB_ENC_PASSPHRASE: "test-api-passphrase",
      API_PORT: 0,
      API_RUN_MIGRATIONS: true,
      API_LOG_LEVEL: "warn",
    };

    await runMigrations(config);
    // SPEC-009 read-slice — materialise the ingest-owned read-target Postgres tables
    // (agents/alerts/incidents) by applying ingest's REAL migrations in-workspace.
    await applyReadSchema(config);
    // SPEC-010 — materialise the ClickHouse cges_events read target the drill reads.
    // The api owns no ClickHouse migration and ingest's bootstrap is private
    // (services/ingest/src/db/migrate.ts:86-155), so this is a scoped test-only DDL
    // helper. Named debt (SPEC-010 §Open questions 1): replace with a shared CH-schema
    // module on the first un-caught DDL drift or a second shared CH table.
    await bootstrapEventsTable(config);

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
