import type { TestProject } from "vitest/node";
import type { Config } from "../src/config.js";
import { startBackends } from "./helpers/backends.js";
import { AGENT_ETW_SESSIONS, stopEtwSessions } from "./helpers/marquee-agent.js";

declare module "vitest" {
  export interface ProvidedContext {
    ingestConfig: Config;
  }
}

/**
 * Start the shared PG/CH/Redis backends once for the whole suite and
 * `provide` the Config to every test (consumed via `inject`). Returns the
 * teardown that stops the containers and, on Windows, any ETW session of the
 * agent's still running: vitest ends each test file's process when the file
 * ends, and a marquee agent that file left running ends with it, before its
 * exit handler stops its session (debt #63).
 */
export default async function setup(project: TestProject): Promise<() => Promise<void>> {
  const { config, stop } = await startBackends();
  project.provide("ingestConfig", config);
  return async () => {
    await stop();
    if (process.platform === "win32") {
      const left = stopEtwSessions(AGENT_ETW_SESSIONS);
      if (left.length > 0) {
        console.log(JSON.stringify({ event: "marquee_agent_etw_sessions_left", sessions: left }));
      }
    }
  };
}
