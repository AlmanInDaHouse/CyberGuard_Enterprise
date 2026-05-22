import type { Config } from "./config.js";

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
 * Start the SPEC-004 ingest server: a plain-HTTP enroll listener and an
 * mTLS heartbeat listener, backed by Postgres / ClickHouse / Redis,
 * returning the bound URLs and the server CA PEM.
 *
 * **B4 scaffold: stub.** This is the TypeScript analog of a Rust `todo!()`.
 * Every acceptance test stands up real testcontainers and then calls this,
 * so the harness is RED by design from the first commit; only the server
 * logic is missing. The B5 commit implements it.
 */
export async function startIngest(_config: Config): Promise<IngestServer> {
  throw new Error("startIngest not implemented (SPEC-004 B5)");
}
