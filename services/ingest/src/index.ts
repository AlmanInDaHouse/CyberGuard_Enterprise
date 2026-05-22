import "reflect-metadata";
import { buildApp } from "./app.js";
import { loadConfig } from "./config.js";

async function main(): Promise<void> {
  const config = loadConfig();
  const app = buildApp({ logger: { level: config.INGEST_LOG_LEVEL } });
  await app.listen({ host: "0.0.0.0", port: config.INGEST_ENROLL_PORT });
  // NOTE (B5): the mTLS heartbeat listener on INGEST_HEARTBEAT_PORT is
  // added with the SPEC-004 implementation; this scaffold serves
  // `/health` and the 501 placeholder routes on the enroll port.
}

main().catch((err: unknown) => {
  process.stderr.write(`cg-ingest: fatal: ${String(err)}\n`);
  process.exit(1);
});
