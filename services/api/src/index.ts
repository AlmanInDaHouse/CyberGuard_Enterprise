import { ConfigError, loadConfig } from "./config.js";
import { startApi } from "./server.js";

// SPEC-023 §Operational §2: the configuration is read inside the promise, so a refused
// value reaches the one line that names its variable, not an uncaught exception.
Promise.resolve()
  .then(() => startApi(loadConfig()))
  .catch((e: unknown) => {
    const line = e instanceof ConfigError ? e.message : String(e);
    process.stderr.write(`cg-api: fatal: ${line}\n`);
    process.exit(1);
  });
