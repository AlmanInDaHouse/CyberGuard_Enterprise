import type { AddressInfo } from "node:net";
import { buildApp } from "./app.js";
import type { Config } from "./config.js";
import { runMigrations } from "./db/migrate.js";
import { buildServices } from "./services.js";

const BIND_HOST = "127.0.0.1";

/** A running api server: the base URL and a close handle. */
export interface ApiServer {
  url: string;
  close(): Promise<void>;
}

/**
 * Start the SPEC-008 api server: optionally apply the api-owned migrations
 * (API_RUN_MIGRATIONS), wire the services, and listen on a user-facing HTTP
 * port. Plain HTTP at the app layer — TLS termination is a deployment concern.
 */
export async function startApi(config: Config): Promise<ApiServer> {
  if (config.API_RUN_MIGRATIONS) {
    await runMigrations(config);
  }
  const services = await buildServices(config);
  try {
    const app = buildApp(services, { level: config.API_LOG_LEVEL });
    await app.listen({ host: BIND_HOST, port: config.API_PORT });
    const port = (app.server.address() as AddressInfo).port;
    app.log.info({ port }, "api listening");
    return {
      url: `http://${BIND_HOST}:${port}`,
      async close() {
        await app.close();
        await services.close();
      },
    };
  } catch (err) {
    await services.close();
    throw err;
  }
}
