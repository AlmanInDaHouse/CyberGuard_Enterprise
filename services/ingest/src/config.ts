import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";

// SPEC-014 — the optional SMTP/notify block treats an empty string (an unset
// `${CG_*:-}` compose default) as ABSENT, so "notify unconfigured" is the clean
// default rather than a validation failure.
const blankToUndefined = (v: unknown): unknown => (v === "" ? undefined : v);
/** An optional, non-empty operator-set string (empty ⇒ absent). */
const optionalSecret = () => z.preprocess(blankToUndefined, z.string().min(1).optional());

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
 * SPEC-004 §Configuration. Validated at startup; the service refuses to
 * start on a missing/invalid variable.
 */
const EnvSchema = z
  .object({
    INGEST_PG_URL: z.string().url(),
    INGEST_CH_URL: z.string().url(),
    INGEST_CH_USER: z.string().default("default"),
    INGEST_CH_PASSWORD: z.string().default(""),
    INGEST_CH_DB: z.string().default("default"),
    INGEST_REDIS_URL: z.string().url(),
    INGEST_ENROLL_PORT: z.coerce.number().int().positive().default(8080),
    INGEST_HEARTBEAT_PORT: z.coerce.number().int().positive().default(8443),
    INGEST_SERVER_CERT_PATH: z.string().default("/certs/server.pem"),
    INGEST_SERVER_KEY_PATH: z.string().default("/certs/server-key.pem"),
    INGEST_CA_PASSPHRASE: z.string().min(1),
    INGEST_RUN_MIGRATIONS: z
      .enum(["true", "false"])
      .default("false")
      .transform((v) => v === "true"),
    INGEST_LOG_LEVEL: z.string().default("info"),
    // ADR-0012 Amendment 2026-06-07 — production detection driver tunables.
    // Ports pattern: env-with-default, operator-overridable, NOT all-or-nothing
    // (unlike the SMTP block). INGEST_DETECT_INTERVAL_MS is the poll interval; a
    // value of 0 DISABLES the driver entirely (no ticks) — the operator
    // kill-switch, and what the detect-ac-001 marquee sets so its explicit
    // runDetectionCycle is the sole producer. INGEST_DETECT_RULES_DIR is the
    // Sigma rules directory the driver loads (fail-loud at boot if it is empty).
    INGEST_DETECT_INTERVAL_MS: z.coerce.number().int().nonnegative().default(10000),
    INGEST_DETECT_RULES_DIR: z.string().min(1).default(defaultDetectRulesDir()),
    // SPEC-014 / ADR-0017 — incident email notification (notify-only). This SMTP
    // block is OPTIONAL and all-or-nothing: when unset, notification is disabled
    // cleanly (the boot does NOT fail — there is no prod detection driver yet,
    // ADR-0017 §Consequences); when ANY member is set, ALL six are required (the
    // superRefine below). The values are an operator-set deployment contract
    // (ADR-0017 §Out of scope), never defaulted here.
    INGEST_SMTP_HOST: optionalSecret(),
    INGEST_SMTP_PORT: z.preprocess(blankToUndefined, z.coerce.number().int().positive().optional()),
    INGEST_SMTP_USER: optionalSecret(),
    INGEST_SMTP_PASS: optionalSecret(),
    INGEST_SMTP_FROM: optionalSecret(),
    INGEST_NOTIFY_RECIPIENT: optionalSecret(),
  })
  .superRefine((cfg, ctx) => {
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

export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const parsed = EnvSchema.safeParse(env);
  if (!parsed.success) {
    const issues = parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ");
    throw new Error(`invalid ingest configuration: ${issues}`);
  }
  return parsed.data;
}
