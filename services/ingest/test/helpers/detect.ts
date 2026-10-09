import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Config } from "../../src/config.js";
import type { DetectConfig } from "../../src/detect/types.js";

const here = dirname(fileURLToPath(import.meta.url));

/** Repo-root `rules/windows/` directory (SPEC-006 §Operational; ADR-0002 Rule 6). */
export const RULES_WINDOWS_DIR = join(here, "..", "..", "..", "..", "rules", "windows");

/**
 * Build the DetectConfig the detection slice consumes for one cycle. The settle
 * margin is 0 (SPEC-018 §Operational §2: tests may inject it): the synthetic
 * tests insert in series and each INSERT has returned, so it is visible, before
 * the cycle reads; no insert is in flight for the margin to wait out. A test that
 * needs the production margin builds its config with driver.ts buildDetectConfig.
 */
export function detectConfig(ingest: Config, orgId = "default"): DetectConfig {
  return {
    ingest,
    orgId,
    rulesDir: RULES_WINDOWS_DIR,
    settleMarginMs: 0,
  };
}
