import { z } from "zod";
import { parseTrustedProxies } from "./auth/source-address.js";

/**
 * A value the start-up checks refuse (SPEC-023 §Operational §2). Its message is the
 * line the entry point prints after `cg-api: fatal: `; it names the variable and does
 * not quote a value that may be a secret: a refused API_TRUSTED_PROXIES entry, an address,
 * is the one value a reason names.
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

/** An integer from `min` to `max`, written in decimal digits alone; `fallback` when unset. */
function integer(min: number, max: number, fallback: number) {
  return z
    .string()
    .optional()
    .transform((value, ctx) => {
      if (value === undefined) return fallback;
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

/**
 * SPEC-008 §Configuration (auth-core), with SPEC-023 §Data contracts §3's checks.
 * Validated at startup; the service refuses to start on a missing/invalid variable
 * (mirrors services/ingest/src/config.ts). services/api is its OWN component
 * (ADR-0014 §3) with its own config surface.
 */
const EnvSchema = z
  .object({
    API_PG_URL: z.string().url(),
    API_REDIS_URL: z.string().url(),
    // Read-only ClickHouse reader for the forensic event drill (ADR-0015 / SPEC-010;
    // the only ClickHouse the human API touches — reads cges_events for
    // GET /v1/incidents/:id/events). 1:1 mirror of services/ingest/src/config.ts:
    // URL required; user/password/db defaulted. The client is lazy (createClient
    // connects on first query), so the three Postgres read routes do NOT depend on
    // ClickHouse availability (ADR-0015 §Compliance).
    API_CH_URL: z.string().url(),
    API_CH_USER: z.string().default("default"),
    API_CH_PASSWORD: z.string().default(""),
    API_CH_DB: z.string().default("default"),
    // pgcrypto passphrase for users.totp_secret at-rest encryption (SPEC-008
    // §Data contracts §1; the INGEST_CA_PASSPHRASE pattern).
    API_DB_ENC_PASSPHRASE: z.string().min(1),
    // pgcrypto passphrase for the DEDICATED forensic Ed25519 signing key at rest
    // (SPEC-012 §Data contracts §4; ADR-0016 §5). A DISTINCT secret from
    // API_DB_ENC_PASSPHRASE: the forensic key attests evidence integrity, a different
    // purpose than TOTP-secret encryption, and it MUST NOT be the ingest CA. Same
    // z.string().min(1) shape — a deployment-contract value the operator sets.
    API_FORENSIC_PASSPHRASE: z.string().min(1),
    API_PORT: integer(1, 65_535, 8081),
    // Bind address for the api listener. Default loopback = the host-process
    // topology (tests, `pnpm dev`); a container needs a non-loopback bind such
    // as 0.0.0.0. Plain HTTP; TLS termination is a deployment concern.
    API_BIND_HOST: z.string().min(1).default("127.0.0.1"),
    API_RUN_MIGRATIONS: z
      .enum(["true", "false"])
      .default("false")
      .transform((v) => v === "true"),
    API_LOG_LEVEL: z.enum(LOG_LEVELS).default("info"),
    // A session's idle lifetime and its cap (SPEC-023 §Data contracts §3), bounded by
    // SPEC-008's values, which are their defaults.
    API_SESSION_IDLE_MINUTES: integer(5, 480, 480),
    API_SESSION_MAX_HOURS: integer(1, 24, 24),
    // The hops whose X-Forwarded-For the api reads (SPEC-008's second Amendment
    // 2026-10-15, the source address behind a proxy): addresses and CIDR ranges,
    // comma-separated. Unset or empty, every request's source address is its peer's.
    API_TRUSTED_PROXIES: z
      .string()
      .optional()
      .superRefine((value, ctx) => {
        try {
          parseTrustedProxies(value);
        } catch (err) {
          ctx.addIssue({ code: z.ZodIssueCode.custom, message: (err as Error).message });
        }
      }),
  })
  .superRefine((cfg, ctx) => {
    if (cfg.API_SESSION_IDLE_MINUTES > cfg.API_SESSION_MAX_HOURS * 60) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["API_SESSION_IDLE_MINUTES"],
        message: "must not be above API_SESSION_MAX_HOURS",
      });
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
