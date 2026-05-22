import type { GlobalSetupContext } from "vitest/node";
import type { Config } from "../src/config.js";
import { startBackends } from "./helpers/backends.js";

declare module "vitest" {
  export interface ProvidedContext {
    ingestConfig: Config;
  }
}

/**
 * Start the shared PG/CH/Redis backends once for the whole suite and
 * `provide` the Config to every test (consumed via `inject`). Returns the
 * teardown that stops the containers.
 */
export default async function setup({ provide }: GlobalSetupContext): Promise<() => Promise<void>> {
  const { config, stop } = await startBackends();
  provide("ingestConfig", config);
  return async () => {
    await stop();
  };
}
