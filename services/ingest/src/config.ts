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
