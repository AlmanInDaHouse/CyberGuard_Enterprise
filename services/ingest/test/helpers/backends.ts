import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { GenericContainer, type StartedTestContainer, Wait } from "testcontainers";
import type { Config } from "../../src/config.js";
import { runMigrations } from "../../src/db/migrate.js";

export interface Backends {
  config: Config;
  stop: () => Promise<void>;
}

/**
 * Start real Postgres / ClickHouse / Redis via testcontainers (same image
 * pins as docker-compose.dev.yml), run the migrations, and return a Config
 * pointing at them. Ports 0 ⇒ the server binds ephemeral ports the test
 * reads back from the returned `IngestServer`.
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

    const certDir = mkdtempSync(join(tmpdir(), "cg-ingest-certs-"));
    const config: Config = {
      INGEST_PG_URL: `postgres://cyberguard:cyberguard_dev@${pg.getHost()}:${pg.getMappedPort(5432)}/cyberguard`,
      INGEST_CH_URL: `http://${ch.getHost()}:${ch.getMappedPort(8123)}`,
      INGEST_CH_USER: "cyberguard",
      INGEST_CH_PASSWORD: "cyberguard_dev",
      INGEST_CH_DB: "cyberguard",
      INGEST_REDIS_URL: `redis://:cyberguard_dev@${redis.getHost()}:${redis.getMappedPort(6379)}`,
      INGEST_ENROLL_PORT: 0,
      INGEST_HEARTBEAT_PORT: 0,
      INGEST_SERVER_CERT_PATH: join(certDir, "server.pem"),
      INGEST_SERVER_KEY_PATH: join(certDir, "server-key.pem"),
      INGEST_CA_PASSPHRASE: "test-ca-passphrase",
      INGEST_RUN_MIGRATIONS: true,
      INGEST_LOG_LEVEL: "warn",
    };

    await runMigrations(config);

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
