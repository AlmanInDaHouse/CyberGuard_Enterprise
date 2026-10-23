import { existsSync, realpathSync, statSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";

// SPEC-014 — the optional SMTP/notify block treats an empty string (an unset
// `${CG_*:-}` compose default) as ABSENT, so "notify unconfigured" is the clean
// default rather than a validation failure.
const blankToUndefined = (v: unknown): unknown => (v === "" ? undefined : v);
/** An optional, non-empty operator-set string (empty ⇒ absent). */
const optionalSecret = () => z.preprocess(blankToUndefined, z.string().min(1).optional());

/**
 * A value the start-up checks refuse (SPEC-023 §Operational §2). Its message is the
 * line the entry point prints after `cg-ingest: fatal: `; it names the variable and
 * never quotes the value, which may be a secret.
 */
export class ConfigError extends Error {
  constructor(
    readonly variable: string,
    readonly reason: string,
  ) {
    // One line: a reason that quotes a value could carry a line break.
    super(`invalid configuration: ${variable}: ${reason.replace(/[\r\n]+/g, " ")}`);
    this.name = "ConfigError";
  }
}

/** pino's levels (SPEC-023 §Data contracts §3, the log levels' check). */
export const LOG_LEVELS = ["fatal", "error", "warn", "info", "debug", "trace", "silent"] as const;

/** An integer from `min` to `max`, written in decimal digits alone, or unset. */
function optionalInteger(min: number, max: number) {
  return z
    .string()
    .optional()
    .transform((value, ctx) => {
      if (value === undefined) return undefined;
      const n = /^\d+$/.test(value) ? Number(value) : Number.NaN;
      if (!(n >= min && n <= max)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `must be an integer from ${min} to ${max}`,
        });
        return z.NEVER;
      }
      return n;
    });
}

/** An integer from `min` to `max`; `fallback` when unset. */
const integer = (min: number, max: number, fallback: number) =>
  optionalInteger(min, max).transform((n) => n ?? fallback);

/** A TCP port: an integer from 1 to 65535. */
const port = (fallback: number) => integer(1, 65_535, fallback);

/** setTimeout's longest delay; a longer one fires at once. */
const MAX_TIMER_MS = 2_147_483_647;

/**
 * The path a file has or would have: its real path when it exists, else its directory's
 * real path and its name, so a link to the directory is seen through; lower-cased where
 * the file system ignores case.
 */
function canonicalPath(p: string): string {
  let real: string;
  try {
    real = realpathSync(p);
  } catch {
    try {
      real = join(realpathSync(dirname(p)), basename(p));
    } catch {
      real = resolve(p);
    }
  }
  return process.platform === "win32" || process.platform === "darwin" ? real.toLowerCase() : real;
}

/** Whether two paths name one file: one canonical path, or one file on disk (a hard link). */
function sameFile(a: string, b: string): boolean {
  if (canonicalPath(a) === canonicalPath(b)) return true;
  try {
    const sa = statSync(a, { bigint: true });
    const sb = statSync(b, { bigint: true });
    return sa.dev === sb.dev && sa.ino === sb.ino;
  } catch {
    return false;
  }
}

/** The six SMTP/notify vars (SPEC-014 §Data contracts §3); all-or-nothing (the superRefine below). */
const SMTP_KEYS = [
  "INGEST_SMTP_HOST",
  "INGEST_SMTP_PORT",
  "INGEST_SMTP_USER",
  "INGEST_SMTP_PASS",
  "INGEST_SMTP_FROM",
  "INGEST_NOTIFY_RECIPIENT",
] as const;

// ADR-0012 Amendment 2026-06-07 — default rules dir for the production detection
// driver, resolved against a STABLE RUNTIME APP ROOT — NOT this module's source
// location, which differs between src and dist and would break in the container.
// Resolution order:
//   1. PACKAGED (prod): `<cwd>/rules/windows`. The container runs with cwd = the
//      app root (Dockerfile WORKDIR /app) and bundles rules/ there (COPY rules
//      ./rules), so this resolves to /app/rules/windows.
//   2. MONOREPO (dev / tests): the repo-root rules/windows, reached from this
//      module — only used when the packaged copy is absent (running from source).
// INGEST_DETECT_RULES_DIR overrides both (ports pattern). The driver also
// fail-louds at boot if the resolved dir yields zero rules (driver.ts).
function defaultDetectRulesDir(): string {
  const packaged = join(process.cwd(), "rules", "windows");
  if (existsSync(packaged)) return packaged;
  return join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "rules", "windows");
}

/**
 * SPEC-004 §Configuration, with SPEC-023 §Data contracts §3's checks. Validated at
 * startup; the service refuses to start on a missing/invalid variable.
 */
const EnvSchema = z
  .object({
    INGEST_PG_URL: z.string().url(),
    INGEST_CH_URL: z.string().url(),
    INGEST_CH_USER: z.string().default("default"),
    INGEST_CH_PASSWORD: z.string().default(""),
    INGEST_CH_DB: z.string().default("default"),
    INGEST_REDIS_URL: z.string().url(),
    INGEST_ENROLL_PORT: port(8080),
    INGEST_HEARTBEAT_PORT: port(8443),
    // Bind address for BOTH listeners. Default loopback = the host-process
    // topology (tests, `pnpm dev`); a container needs a non-loopback bind such
    // as 0.0.0.0. Independent of the cert SAN, which is verified against the
    // host the agent dials (SPEC-003 FR-005), not the bound interface.
    INGEST_BIND_HOST: z.string().min(1).default("127.0.0.1"),
    INGEST_SERVER_CERT_PATH: z.string().min(1).default("/certs/server.pem"),
    INGEST_SERVER_KEY_PATH: z.string().min(1).default("/certs/server-key.pem"),
    INGEST_CA_PASSPHRASE: z.string().min(1),
    INGEST_RUN_MIGRATIONS: z
      .enum(["true", "false"])
      .default("false")
      .transform((v) => v === "true"),
    INGEST_LOG_LEVEL: z.enum(LOG_LEVELS).default("info"),
    // ADR-0012 Amendment 2026-06-07 — production detection driver tunables.
    // Ports pattern: env-with-default, operator-overridable, NOT all-or-nothing
    // (unlike the SMTP block). INGEST_DETECT_INTERVAL_MS is the poll interval; a
    // value of 0 DISABLES the driver entirely (no ticks) — the operator
    // kill-switch, and what the detect-ac-001 marquee sets so its explicit
    // runDetectionCycle is the sole producer. INGEST_DETECT_RULES_DIR is the
    // Sigma rules directory the driver loads (fail-loud at boot if it is empty).
    // An empty interval is refused, not read as 0 (SPEC-023 §Data contracts §3).
    INGEST_DETECT_INTERVAL_MS: integer(0, MAX_TIMER_MS, 5000),
    INGEST_DETECT_RULES_DIR: z.string().min(1).default(defaultDetectRulesDir()),
    // SPEC-022 §Operational §1 — the SOAR playbooks directory. Optional with NO
    // default here, so the loader (src/soar/loader.ts) can tell set from unset: a
    // set directory that does not exist is logged at warn, an unset one resolves to
    // the bundled playbooks/. An empty value counts as unset.
    INGEST_SOAR_PLAYBOOKS_DIR: z.preprocess(blankToUndefined, z.string().min(1).optional()),
    // SPEC-014 / ADR-0017 — incident email notification (notify-only). This SMTP
    // block is OPTIONAL and all-or-nothing: when unset, notification is disabled
    // cleanly (the boot does NOT fail — detection still runs, only the email is skipped,
    // ADR-0017 §Consequences); when ANY member is set, ALL six are required (the
    // superRefine below). The values are an operator-set deployment contract
    // (ADR-0017 §Out of scope), never defaulted here.
    INGEST_SMTP_HOST: optionalSecret(),
    INGEST_SMTP_PORT: z.preprocess(blankToUndefined, optionalInteger(1, 65_535)),
    INGEST_SMTP_USER: optionalSecret(),
    INGEST_SMTP_PASS: optionalSecret(),
    INGEST_SMTP_FROM: optionalSecret(),
    INGEST_NOTIFY_RECIPIENT: optionalSecret(),
  })
  .superRefine((cfg, ctx) => {
    // Debt #105: a reissue writes the certificate and then the key, so one file for
    // both loses the key.
    if (sameFile(cfg.INGEST_SERVER_CERT_PATH, cfg.INGEST_SERVER_KEY_PATH)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["INGEST_SERVER_KEY_PATH"],
        message: "names the same file as INGEST_SERVER_CERT_PATH",
      });
    }
    const present = SMTP_KEYS.filter((k) => cfg[k] !== undefined);
    if (present.length > 0 && present.length < SMTP_KEYS.length) {
      for (const k of SMTP_KEYS) {
        if (cfg[k] === undefined) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: [k],
            message:
              "incident notification (SMTP) is all-or-nothing: set all six INGEST_SMTP_* / INGEST_NOTIFY_RECIPIENT vars, or none",
          });
        }
      }
    }
  });

export type Config = z.infer<typeof EnvSchema>;

/** The reason a check gives, in words that never quote the value. */
function reasonOf(issue: z.ZodIssue): string {
  if (issue.code === z.ZodIssueCode.invalid_type && issue.received === "undefined") {
    return "required";
  }
  if (issue.code === z.ZodIssueCode.invalid_string && issue.validation === "url") {
    return "must be a URL";
  }
  if (issue.code === z.ZodIssueCode.too_small && issue.type === "string") {
    return "must not be empty";
  }
  if (issue.code === z.ZodIssueCode.invalid_enum_value) {
    return `must be one of ${issue.options.join(", ")}`;
  }
  return issue.message;
}

/** The configuration `env` gives, or a ConfigError naming the first variable refused. */
export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const parsed = EnvSchema.safeParse(env);
  if (!parsed.success) {
    const issue = parsed.error.issues[0] as z.ZodIssue;
    throw new ConfigError(issue.path.join("."), reasonOf(issue));
  }
  return parsed.data;
}
