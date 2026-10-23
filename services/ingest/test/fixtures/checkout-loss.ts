// A program for the store-loss test: it checks a client out of a pool that ingest's
// services watch as theirs (watchPool), says so on stdout, and waits on a query while the
// test cuts the connection. It exits 0 if it outlives the loss, the query having failed;
// an unheard `error` event of the checked-out client would end it with 1 first.
import pg from "pg";
import { watchPool } from "../../src/store-events.js";

/** The log a service gives watchPool, as JSON lines on stdout. */
const log = {
  warn: (obj: Record<string, unknown>, msg: string) =>
    process.stdout.write(`${JSON.stringify({ level: "warn", ...obj, msg })}\n`),
  info: (obj: Record<string, unknown>, msg: string) =>
    process.stdout.write(`${JSON.stringify({ level: "info", ...obj, msg })}\n`),
};

const pool = new pg.Pool({ connectionString: process.argv[2] as string });
watchPool(pool, log);
const client = await pool.connect();
await client.query("SELECT 1");
process.stdout.write("checked-out\n");
const failed = await client.query("SELECT pg_sleep(30)").then(
  () => false,
  (err: Error) => {
    process.stdout.write(`${JSON.stringify({ query_error: err.message })}\n`);
    return true;
  },
);
await new Promise((r) => setTimeout(r, 500));
client.release(true);
await pool.end().catch(() => {});
process.stdout.write(failed ? "query failed\n" : "query answered\n");
process.exit(failed ? 0 : 3);
