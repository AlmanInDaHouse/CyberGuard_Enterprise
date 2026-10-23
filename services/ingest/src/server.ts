import type { AddressInfo, Socket } from "node:net";
import type { Server as TlsServer } from "node:tls";
import { pino } from "pino";
import { buildEnrollApp, buildHeartbeatApp } from "./app.js";
import {
  SERVER_CHECK_INTERVAL_MS,
  SERVER_EXPIRY_MARGIN_MS,
  type ServerIdentity,
  checkServerCert,
  ensureServerCert,
} from "./cert.js";
import type { Config } from "./config.js";
import { runMigrations } from "./db/migrate.js";
import { loadDetectionRules, startProdDetectionDriver } from "./detect/driver.js";
import { buildServices } from "./services.js";
import { loadPlaybooks } from "./soar/loader.js";
import type { LoadedPlaybooks } from "./soar/types.js";

/** ADR-0004 §Transport's cipher suites, the two both listeners take (SPEC-023 §Operational §6). */
export const TLS_CIPHER_SUITES = "TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256";

/** A running ingest server: the two listener URLs and the CA PEM. */
export interface IngestServer {
  /** Base URL of the enroll listener: `https://127.0.0.1:PORT`, or `http://` with
   *  INGEST_ENROLL_TLS=false. */
  enrollUrl: string;
  /** Base URL of the mTLS heartbeat listener (e.g. `https://127.0.0.1:PORT`). */
  heartbeatUrl: string;
  /** PEM of the server CA — the agent trust anchor and the test-client mTLS root. */
  caCertPem: string;
  /** Stop both listeners and release resources. */
  close(): Promise<void>;
}

/**
 * Start the SPEC-004 ingest server: an enroll listener, over TLS unless
 * INGEST_ENROLL_TLS=false (SPEC-023 §Operational §6), and an mTLS heartbeat
 * listener, backed by Postgres / ClickHouse / Redis, returning the bound URLs and
 * the server CA PEM. Both listeners bind INGEST_BIND_HOST (default loopback; a
 * container needs a non-loopback bind such as 0.0.0.0). The bind is independent of
 * the cert SAN: SAN verification is against the host the agent dials (SPEC-003
 * FR-005), not the bound interface, and the self-issued cert (cert.ts) covers the
 * names INGEST_SERVER_NAMES lists, localhost and 127.0.0.1 by default.
 */
/** The boot log surface (pino's `Logger` satisfies it); a test injects a sink. */
export interface BootLog {
  info(obj: Record<string, unknown>, msg: string): void;
  warn(obj: Record<string, unknown>, msg: string): void;
  error(obj: Record<string, unknown>, msg: string): void;
}

export interface StartOptions {
  /**
   * Where the boot logs before the apps exist, and where the server certificate's
   * checks log at start and while running; pino at INGEST_LOG_LEVEL by default.
   */
  log?: BootLog;
  /**
   * Test seams for the server certificate's checks (SPEC-004 Amendment 2026-10-15
   * item 10): the clock they and item 7 read, and the running check's interval (24 h).
   */
  serverCert?: { now?: () => number; checkIntervalMs?: number };
}

/** Run `check` every `intervalMs`, one at a time, until `stop()`, which awaits a run in flight. */
function every(intervalMs: number, check: () => Promise<void>): { stop(): Promise<void> } {
  let running: Promise<void> | null = null;
  const timer = setInterval(() => {
    if (running) return;
    running = check().finally(() => {
      running = null;
    });
  }, intervalMs);
  timer.unref();
  return {
    async stop() {
      clearInterval(timer);
      await running;
    },
  };
}

/** Log why SOAR is idle, at the level the loader chose (SPEC-022 §Operational §1). */
function logSoarIdle(log: BootLog, loaded: LoadedPlaybooks): void {
  if (!loaded.idle) return;
  const { reason, dir, level } = loaded.idle;
  log[level]({ event: "soar_idle", reason, dir }, "SOAR is idle");
}

export async function startIngest(
  config: Config,
  options: StartOptions = {},
): Promise<IngestServer> {
  // SPEC-022 §Operational §1 — the playbooks load first, before any client is built
  // or any migration runs: a refused playbook rejects here and leaves nothing open.
  // SPEC-023 §Operational §2 — the detection rules too, so a rule set that fails
  // leaves no listener open.
  const log = options.log ?? pino({ name: "ingest-boot", level: config.INGEST_LOG_LEVEL });
  const playbooks = loadPlaybooks(config.INGEST_SOAR_PLAYBOOKS_DIR);
  logSoarIdle(log, playbooks);
  const rules = loadDetectionRules(config);

  if (config.INGEST_RUN_MIGRATIONS) {
    await runMigrations(config);
  }

  const services = await buildServices(config, playbooks.playbooks);
  // The apps built so far, so a failure after a listen closes what was bound.
  const built: { close(): Promise<unknown> }[] = [];

  try {
    const bindHost = config.INGEST_BIND_HOST;
    // SPEC-004 Amendment 2026-10-15 — the pair is classified, and a self-issued one
    // reissued if it needs it, before the mTLS listener binds.
    const certPath = config.INGEST_SERVER_CERT_PATH;
    const keyPath = config.INGEST_SERVER_KEY_PATH;
    const certChecks = {
      now: options.serverCert?.now ?? Date.now,
      log,
      names: config.INGEST_SERVER_NAMES,
    };
    let serverIdentity: ServerIdentity = await ensureServerCert(
      services.ca,
      certPath,
      keyPath,
      certChecks,
    );
    // The secure context's options besides the pair; a reissue while running
    // replaces each listener's context with these and the new pair.
    const secureContext = {
      ca: services.ca.caCertPem,
      // TLS 1.3 only, with the two cipher suites ADR-0004 §Transport allows, the two the
      // agent's rustls offers (SPEC-023 §Operational §6): Node's default would add
      // TLS_AES_128_GCM_SHA256.
      minVersion: "TLSv1.3" as const,
      ciphers: TLS_CIPHER_SUITES,
    };
    // SPEC-023 §Operational §6 — the enrollment listener serves HTTPS with the server
    // certificate and asks for no client certificate; INGEST_ENROLL_TLS=false keeps it in
    // clear, for the test suites that post to it so.
    const enrollTls = config.INGEST_ENROLL_TLS;

    const logger = { level: config.INGEST_LOG_LEVEL };
    const enrollApp = buildEnrollApp(
      services,
      logger,
      enrollTls
        ? { ...secureContext, key: serverIdentity.keyPem, cert: serverIdentity.certPem }
        : undefined,
    );
    built.push(enrollApp);
    const heartbeatApp = buildHeartbeatApp(
      services,
      {
        ...secureContext,
        key: serverIdentity.keyPem,
        cert: serverIdentity.certPem,
        requestCert: true,
        rejectUnauthorized: true,
      },
      logger,
    );
    built.push(heartbeatApp);
    const heartbeatServer = heartbeatApp.server as unknown as TlsServer;
    // The listeners that serve the server certificate: both, enrollment over TLS.
    const tlsServers = enrollTls
      ? [heartbeatServer, enrollApp.server as unknown as TlsServer]
      : [heartbeatServer];
    // SPEC-004 Amendment 2026-10-15 item 7, on each TLS listener (SPEC-023 §Operational
    // §6) — an expired certificate is not served: a connection closed before its
    // handshake is one the agent retries, while an expired certificate makes it exit
    // (SPEC-003 exit 6). The margin covers an agent whose clock runs ahead of the server's.
    for (const tlsServer of tlsServers) {
      tlsServer.prependListener("connection", (socket: Socket) => {
        const left = serverIdentity.notAfter.getTime() - certChecks.now();
        if (left <= SERVER_EXPIRY_MARGIN_MS) socket.destroy();
      });
    }

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
    const detectionDriver = startProdDetectionDriver(
      config,
      services,
      (event, fields) => enrollApp.log.error(fields ?? {}, event),
      rules,
    );
    enrollApp.log.info(
      { interval_ms: config.INGEST_DETECT_INTERVAL_MS },
      "detection driver started",
    );

    // SPEC-004 Amendment 2026-10-15 item 4 — the running check of the certificate
    // the listener serves; after a reissue each new handshake gets the new pair.
    const serverCertCheck = every(
      options.serverCert?.checkIntervalMs ?? SERVER_CHECK_INTERVAL_MS,
      async () => {
        try {
          const next = await checkServerCert(
            services.ca,
            serverIdentity,
            certPath,
            keyPath,
            certChecks,
          );
          if (!next) return;
          // Each listener that serves the certificate takes the new pair: each new
          // handshake gets it (SPEC-023 §Operational §6).
          for (const tlsServer of tlsServers) {
            tlsServer.setSecureContext({
              ...secureContext,
              key: next.keyPem,
              cert: next.certPem,
            });
          }
          serverIdentity = next;
        } catch (err) {
          log.error(
            {
              event: "server_cert_reissue_failed",
              error: err instanceof Error ? err.message : String(err),
              cert_path: certPath,
              not_after: serverIdentity.notAfter.toISOString(),
            },
            "server certificate reissue failed",
          );
        }
      },
    );

    return {
      enrollUrl: `${enrollTls ? "https" : "http"}://${bindHost}:${enrollPort}`,
      heartbeatUrl: `https://${bindHost}:${heartbeatPort}`,
      caCertPem: services.ca.caCertPem,
      async close() {
        // Stop the driver first: stop() cancels the pending tick and awaits any
        // in-flight pass (bounded), so services.close() below never races a cycle.
        await detectionDriver.stop();
        await serverCertCheck.stop();
        await Promise.allSettled([enrollApp.close(), heartbeatApp.close()]);
        await services.close();
      },
    };
  } catch (err) {
    // Close whatever listener was bound before the failure (debt #19), then the services.
    await Promise.allSettled(built.map((app) => app.close()));
    await services.close();
    throw err;
  }
}
