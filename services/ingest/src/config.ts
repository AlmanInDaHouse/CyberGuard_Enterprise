import { X509Certificate } from "node:crypto";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { isIP } from "node:net";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import {
  RETENTION_DEFAULTS,
  RETENTION_MAX_DAYS,
  RETENTION_MIN_DAYS,
} from "./db/clickhouse-bootstrap.js";

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

/** A retention period in days, in SPEC-023 §Data contracts §3's bounds. */
const retentionDays = (fallback: number) =>
  integer(RETENTION_MIN_DAYS, RETENTION_MAX_DAYS, fallback);

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

/** The most names the server certificate carries (SPEC-023 §Operational §6). */
const MAX_SERVER_NAMES = 20;
/** A DNS name: labels of letters, digits and inner hyphens, 1 to 63 each, 253 in all. */
const DNS_LABEL = /^(?!-)[A-Za-z0-9-]{1,63}(?<!-)$/;

/** A label of digits alone: no top-level domain is one, so a name ending in one is a
 *  mistyped address (`10.0.0.256`, `1.2.3`), not a DNS name. */
const NUMERIC_LABEL = /^\d+$/;

/** Why a list of server names is refused, or null when it is a list of 1 to 20 DNS names
 *  and IPv4 or IPv6 addresses, an IPv6 address without a zone. The reasons name no entry. */
function serverNamesProblem(names: string[]): string | null {
  if (names.length === 0 || names.every((n) => n === "")) return "must name at least one host";
  if (names.length > MAX_SERVER_NAMES) return `must name at most ${MAX_SERVER_NAMES} hosts`;
  for (const name of names) {
    const ip = isIP(name);
    if (ip === 4) continue;
    if (ip === 6) {
      // A zone (`fe80::1%eth0`) is the host's own, and a certificate has no place for it.
      if (name.includes("%")) return "each entry must be a DNS name or an IP address";
      continue;
    }
    const labels = name.split(".");
    if (
      name.length > 253 ||
      !labels.every((label) => DNS_LABEL.test(label)) ||
      NUMERIC_LABEL.test(labels[labels.length - 1] ?? "")
    ) {
      return "each entry must be a DNS name or an IP address";
    }
  }
  return null;
}

/** An IPv6 address in the one form a URL prints it (lowercase, compressed, in hexadecimal),
 *  the form the certificate's SAN encodes faithfully; any other name as given. */
function canonicalServerName(name: string): string {
  return isIP(name) === 6 ? new URL(`http://[${name}]/`).hostname.slice(1, -1) : name;
}

/** A PEM certificate block. */
const PEM_CERTIFICATE = /-----BEGIN CERTIFICATE-----[\s\S]+?-----END CERTIFICATE-----/g;

/** Why a CA file is refused, or null when it is a readable file holding at least one PEM
 *  certificate Node loads (SPEC-023 §Data contracts §3): Node loads a CA file up to its
 *  first block that does not parse, so the first certificate block must; a corrupt one
 *  before a good one would leave the send trusting nothing. The reasons name no path. */
function caFileProblem(path: string): string | null {
  let text: string;
  try {
    text = readFileSync(path, "utf8");
  } catch {
    return "must name a readable file";
  }
  const first = text.match(PEM_CERTIFICATE)?.[0];
  if (first === undefined) return "must hold at least one PEM certificate";
  try {
    new X509Certificate(first);
  } catch {
    return "must hold at least one PEM certificate";
  }
  return null;
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
const Fields = z.object({
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
  // SPEC-023 §Operational §6 — the names agents reach the server by: the SANs of the
  // certificate the server issues itself, and its subject's CN, the first of them.
  INGEST_SERVER_NAMES: z
    .string()
    .default("localhost,127.0.0.1")
    .transform((value, ctx) => {
      const names = value.split(",").map((n) => n.trim());
      const reason = serverNamesProblem(names);
      if (reason) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, message: reason });
        return z.NEVER;
      }
      return names.map(canonicalServerName);
    }),
  // SPEC-023 §Operational §6 — the enrollment listener serves HTTPS with the server
  // certificate; `false` keeps plain HTTP, for the test suites that post to it in clear.
  INGEST_ENROLL_TLS: z
    .enum(["true", "false"])
    .default("true")
    .transform((v) => v === "true"),
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
  // SPEC-023 §Operational §9 — the days a row stays in ClickHouse after its arrival, per
  // class of cges_events and for heartbeats, 7 to 3650; the migration step sets them as the
  // tables' time-to-live.
  INGEST_RETENTION_PROCESS_DAYS: retentionDays(RETENTION_DEFAULTS.processDays),
  INGEST_RETENTION_NETWORK_DAYS: retentionDays(RETENTION_DEFAULTS.networkDays),
  INGEST_RETENTION_LOGON_DAYS: retentionDays(RETENTION_DEFAULTS.logonDays),
  INGEST_RETENTION_HEARTBEAT_DAYS: retentionDays(RETENTION_DEFAULTS.heartbeatDays),
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
  // SPEC-023 §Data contracts §3 (debt #78): the relay's private CA, whose certificates the
  // send trusts for the relay instead of Node's roots; the empty string counts as unset.
  INGEST_SMTP_CA_FILE: z.preprocess(
    blankToUndefined,
    z
      .string()
      .optional()
      .superRefine((path, ctx) => {
        if (path === undefined) return;
        const reason = caFileProblem(path);
        if (reason) ctx.addIssue({ code: z.ZodIssueCode.custom, message: reason });
      }),
  ),
});

const EnvSchema = Fields.superRefine((cfg, ctx) => {
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

/** The parsed value, or a ConfigError naming the first variable refused. */
function parsedOr<T>(parsed: z.SafeParseReturnType<unknown, T>): T {
  if (!parsed.success) {
    const issue = parsed.error.issues[0] as z.ZodIssue;
    throw new ConfigError(issue.path.join("."), reasonOf(issue));
  }
  return parsed.data;
}

/** The configuration `env` gives, or a ConfigError naming the first variable refused. */
export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  return parsedOr(EnvSchema.safeParse(env));
}

type Shape = typeof Fields.shape;

/**
 * The variables a command uses, checked as the service checks them, and no other
 * (SPEC-023 §Data contracts §4: each command validates only the variables it uses). The
 * checks across variables (the certificate and key naming one file, the notification's
 * all-or-none) are the whole configuration's, and do not run here.
 */
export function loadConfigFor<K extends keyof Shape>(
  keys: readonly K[],
  env: NodeJS.ProcessEnv = process.env,
): Pick<Config, K> {
  // The mask names keys of Fields alone, which zod's Exactly<> cannot see through a generic K.
  const mask = Object.fromEntries(keys.map((k) => [k, true])) as never;
  return parsedOr(Fields.pick(mask).safeParse(env)) as unknown as Pick<Config, K>;
}
