import type { AddressInfo } from "node:net";
import { buildEnrollApp, buildHeartbeatApp } from "./app.js";
import { ensureServerCert } from "./cert.js";
import type { Config } from "./config.js";
import { runMigrations } from "./db/migrate.js";
import { startProdDetectionDriver } from "./detect/driver.js";
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

/**
 * Start the SPEC-004 ingest server: a plain-HTTP enroll listener and an mTLS
 * heartbeat listener, backed by Postgres / ClickHouse / Redis, returning the
 * bound URLs and the server CA PEM. Both listeners bind INGEST_BIND_HOST
 * (default loopback; a container needs a non-loopback bind such as 0.0.0.0).
 * The bind is independent of the cert SAN: SAN verification is against the
 * host the agent dials (SPEC-003 FR-005), not the bound interface, and the
 * self-issued cert (cert.ts) covers only localhost / 127.0.0.1.
 */
export async function startIngest(config: Config): Promise<IngestServer> {
  if (config.INGEST_RUN_MIGRATIONS) {
    await runMigrations(config);
  }

  const services = await buildServices(config);

  try {
    const bindHost = config.INGEST_BIND_HOST;
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

    await enrollApp.listen({ host: bindHost, port: config.INGEST_ENROLL_PORT });
    await heartbeatApp.listen({ host: bindHost, port: config.INGEST_HEARTBEAT_PORT });

    const enrollPort = (enrollApp.server.address() as AddressInfo).port;
    const heartbeatPort = (heartbeatApp.server.address() as AddressInfo).port;

    enrollApp.log.info(
      { enroll_port: enrollPort, heartbeat_port: heartbeatPort },
      "server listening",
    );

    // ADR-0012 Amendment 2026-06-07 — start the production detection driver once
    // the listeners are bound (this point). It drives runDetectionCycle per org
    // on a self-rescheduling interval and carries services.notify (SPEC-014).
    // Cycle errors are logged best-effort; it is stopped in close() below BEFORE
    // services are torn down, so no cycle is mid-flight when the pool closes.
    const detectionDriver = startProdDetectionDriver(config, services, (event, fields) =>
      enrollApp.log.error(fields ?? {}, event),
    );
    enrollApp.log.info(
      { interval_ms: config.INGEST_DETECT_INTERVAL_MS },
      "detection driver started",
    );

    return {
      enrollUrl: `http://${bindHost}:${enrollPort}`,
      heartbeatUrl: `https://${bindHost}:${heartbeatPort}`,
      caCertPem: services.ca.caCertPem,
      async close() {
        // Stop the driver first: stop() cancels the pending tick and awaits any
        // in-flight pass (bounded), so services.close() below never races a cycle.
        await detectionDriver.stop();
        await Promise.allSettled([enrollApp.close(), heartbeatApp.close()]);
        await services.close();
      },
    };
  } catch (err) {
    await services.close();
    throw err;
  }
}
