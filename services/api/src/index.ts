import { loadConfig } from "./config.js";
import { startApi } from "./server.js";

startApi(loadConfig()).catch((e: unknown) => {
  process.stderr.write(`cg-api: ${String(e)}\n`);
  process.exit(1);
});
