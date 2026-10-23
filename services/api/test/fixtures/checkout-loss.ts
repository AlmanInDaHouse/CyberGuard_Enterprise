// A program for deploy_ac_017's store-loss test: it checks a client out of the api's pool
// (createPgPool), says so on stdout, and waits on a query while the test cuts the
// connection. It exits 0 if it outlives the loss, the query having failed; an unheard
// `error` event of the checked-out client would end it with 1 first.
import { createPgPool } from "../../src/services.js";

/** The log a service gives watchPool, as JSON lines on stdout. */
const log = {
  warn: (obj: Record<string, unknown>, msg: string) =>
    process.stdout.write(`${JSON.stringify({ level: "warn", ...obj, msg })}\n`),
  info: (obj: Record<string, unknown>, msg: string) =>
    process.stdout.write(`${JSON.stringify({ level: "info", ...obj, msg })}\n`),
};

const pool = createPgPool(process.argv[2] as string, log);
const client = await pool.connect();
await client.query("SELECT 1");
process.stdout.write("checked-out\n");
// The pool's statement timeout (5 s) would end the query too: the error says which.
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
