import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";

// deploy_ac_013 — SPEC-023 §Operational §2 and §Data contracts §3, the api's half: the
// start-up entry point refuses a malformed value with one line that names the variable,
// `cg-api: fatal: invalid configuration: <VAR>: <reason>`, and exits 1; the error
// reaches that line, not an uncaught exception.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const LINE = /^cg-api: fatal: invalid configuration: ([A-Z_]+): \S.*$/;

let config: Config;

beforeAll(() => {
  config = inject("apiConfig");
});

/** The environment of a start that would succeed: the suite's stores. */
function validEnv(): Record<string, string> {
  return {
    API_PG_URL: config.API_PG_URL,
    API_REDIS_URL: config.API_REDIS_URL,
    API_CH_URL: config.API_CH_URL,
    API_CH_USER: config.API_CH_USER,
    API_CH_PASSWORD: config.API_CH_PASSWORD,
    API_CH_DB: config.API_CH_DB,
    API_DB_ENC_PASSPHRASE: config.API_DB_ENC_PASSPHRASE,
    API_FORENSIC_PASSPHRASE: config.API_FORENSIC_PASSPHRASE,
    API_LOG_LEVEL: "warn",
  };
}

/** The process environment without the API_* variables a developer's shell may set. */
function baseEnv(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("API_")));
}

/** Run the start-up entry point (`src/index.ts`, as `pnpm dev` does) and expect a refusal. */
function expectRefused(env: Record<string, string>, variable: string): void {
  const r = spawnSync(process.execPath, ["--import", "tsx", "src/index.ts"], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...baseEnv(), ...env },
    timeout: 60_000,
  });
  expect(r.status, r.stderr).toBe(1);
  const lines = r.stderr.trim().split(/\r?\n/);
  expect(lines, r.stderr).toHaveLength(1);
  expect(lines[0]).toMatch(LINE);
  expect(LINE.exec(lines[0] ?? "")?.[1]).toBe(variable);
}

// SPEC-023 Amendment 2026-10-23 item 1: decimal digits alone.
test.each([["0"], ["65536"], ["8081x"], [" 8081"], ["+8081"], ["8081.0"], ["1e3"], ["0x1F91"]])(
  "API_PORT=%j is refused",
  (value) => {
    expectRefused({ ...validEnv(), API_PORT: value }, "API_PORT");
  },
);

test.each([["verbose"], ["WARN"], [""]])("API_LOG_LEVEL=%j is refused", (value) => {
  expectRefused({ ...validEnv(), API_LOG_LEVEL: value }, "API_LOG_LEVEL");
});

test.each([["4"], ["481"], ["ten"], [""]])("API_SESSION_IDLE_MINUTES=%j is refused", (value) => {
  expectRefused({ ...validEnv(), API_SESSION_IDLE_MINUTES: value }, "API_SESSION_IDLE_MINUTES");
});

test.each([["0"], ["25"], ["1.5"]])("API_SESSION_MAX_HOURS=%j is refused", (value) => {
  expectRefused({ ...validEnv(), API_SESSION_MAX_HOURS: value }, "API_SESSION_MAX_HOURS");
});

test("an idle lifetime above the cap is refused", () => {
  expectRefused(
    { ...validEnv(), API_SESSION_IDLE_MINUTES: "61", API_SESSION_MAX_HOURS: "1" },
    "API_SESSION_IDLE_MINUTES",
  );
});

test("a missing required variable is refused, named", () => {
  const { API_FORENSIC_PASSPHRASE: _, ...env } = validEnv();
  expectRefused(env, "API_FORENSIC_PASSPHRASE");
});

test("a trusted-proxy list that does not parse is refused, named", () => {
  expectRefused({ ...validEnv(), API_TRUSTED_PROXIES: "not-an-address" }, "API_TRUSTED_PROXIES");
});
