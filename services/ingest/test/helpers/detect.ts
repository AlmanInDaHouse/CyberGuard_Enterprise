import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Config } from "../../src/config.js";
import { CORRELATION_WINDOW_SECONDS_DEFAULT, type DetectConfig } from "../../src/detect/types.js";

const here = dirname(fileURLToPath(import.meta.url));

/** Repo-root `rules/windows/` directory (SPEC-006 §Operational; ADR-0002 Rule 6). */
export const RULES_WINDOWS_DIR = join(here, "..", "..", "..", "..", "rules", "windows");

/** Build the DetectConfig the detection slice consumes for one cycle. */
export function detectConfig(ingest: Config, orgId = "default"): DetectConfig {
  return {
    ingest,
    orgId,
    rulesDir: RULES_WINDOWS_DIR,
    correlationWindowSeconds: CORRELATION_WINDOW_SECONDS_DEFAULT,
  };
}
