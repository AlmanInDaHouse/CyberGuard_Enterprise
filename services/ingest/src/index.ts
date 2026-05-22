import "reflect-metadata";
import { loadConfig } from "./config.js";
import { type IngestServer, startIngest } from "./server.js";

async function main(): Promise<void> {
  const config = loadConfig();
  const server = await startIngest(config);

  // NFR-003 graceful shutdown: stop accepting connections and drain.
  const shutdown = (signal: string) => {
    void (async () => {
      process.stderr.write(`cg-ingest: ${signal} received, shutting down\n`);
      try {
        await server.close();
        process.exit(0);
      } catch (err) {
        process.stderr.write(`cg-ingest: shutdown error: ${String(err)}\n`);
        process.exit(1);
      }
    })();
  };
  for (const sig of ["SIGINT", "SIGTERM"] as const) {
    process.on(sig, () => shutdown(sig));
  }
}

main().catch((err: unknown) => {
  process.stderr.write(`cg-ingest: fatal: ${String(err)}\n`);
  process.exit(1);
});

export type { IngestServer };
