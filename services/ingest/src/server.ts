import type { AddressInfo } from "node:net";
import { buildEnrollApp, buildHeartbeatApp } from "./app.js";
import { ensureServerCert } from "./cert.js";
import type { Config } from "./config.js";
import { runMigrations } from "./db/migrate.js";
import { buildServices } from "./services.js";

/** A running ingest server: the two listener URLs and the CA PEM. */
export interface IngestServer {
  /** Base URL of the plain-HTTP enroll listener (e.g. `http://127.0.0.1:PORT`). */
  enrollUrl: string;
  /** Base URL of the mTLS heartbeat listener (e.g. `https://127.0.0.1:PORT`). */
  heartbeatUrl: string;
  /** PEM of the server CA — the agent trust anchor and the test-client mTLS root. */
  caCertPem: string;
  /** Stop both listeners and release resources. */
  close(): Promise<void>;
}

const BIND_HOST = "127.0.0.1";

/**
 * Start the SPEC-004 ingest server: a plain-HTTP enroll listener and an mTLS
 * heartbeat listener, backed by Postgres / ClickHouse / Redis, returning the
 * bound URLs and the server CA PEM. Binds to 127.0.0.1 so the agent's IP-based
 * TLS server-name verification matches the server cert's `IP:127.0.0.1` SAN.
 */
export async function startIngest(config: Config): Promise<IngestServer> {
  if (config.INGEST_RUN_MIGRATIONS) {
    await runMigrations(config);
  }

  const services = await buildServices(config);

  try {
    const serverIdentity = await ensureServerCert(
      services.ca,
      config.INGEST_SERVER_CERT_PATH,
      config.INGEST_SERVER_KEY_PATH,
    );

    const logger = { level: config.INGEST_LOG_LEVEL };
    const enrollApp = buildEnrollApp(services, logger);
    const heartbeatApp = buildHeartbeatApp(
      services,
      {
        key: serverIdentity.keyPem,
        cert: serverIdentity.certPem,
        ca: services.ca.caCertPem,
        requestCert: true,
        rejectUnauthorized: true,
        // TLS 1.3 only. Node's default 1.3 suites include the two the agent's
        // rustls offers (TLS_AES_256_GCM_SHA384 / TLS_CHACHA20_POLY1305_SHA256),
        // so negotiation succeeds (ADR-0004 / SPEC-003 FR-006).
        minVersion: "TLSv1.3",
      },
      logger,
    );

    await enrollApp.listen({ host: BIND_HOST, port: config.INGEST_ENROLL_PORT });
    await heartbeatApp.listen({ host: BIND_HOST, port: config.INGEST_HEARTBEAT_PORT });

    const enrollPort = (enrollApp.server.address() as AddressInfo).port;
    const heartbeatPort = (heartbeatApp.server.address() as AddressInfo).port;

    enrollApp.log.info(
      { enroll_port: enrollPort, heartbeat_port: heartbeatPort },
      "server listening",
    );

    return {
      enrollUrl: `http://${BIND_HOST}:${enrollPort}`,
      heartbeatUrl: `https://${BIND_HOST}:${heartbeatPort}`,
      caCertPem: services.ca.caCertPem,
      async close() {
        await Promise.allSettled([enrollApp.close(), heartbeatApp.close()]);
        await services.close();
      },
    };
  } catch (err) {
    await services.close();
    throw err;
  }
}
