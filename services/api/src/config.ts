import { z } from "zod";

/**
 * SPEC-008 §Configuration (auth-core). Validated at startup; the service refuses
 * to start on a missing/invalid variable (mirrors services/ingest/src/config.ts).
 * services/api is its OWN component (ADR-0014 §3) with its own config surface.
 */
const EnvSchema = z.object({
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
  API_PORT: z.coerce.number().int().positive().default(8081),
  API_RUN_MIGRATIONS: z
    .enum(["true", "false"])
    .default("false")
    .transform((v) => v === "true"),
  API_LOG_LEVEL: z.string().default("info"),
});

export type Config = z.infer<typeof EnvSchema>;

export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const parsed = EnvSchema.safeParse(env);
  if (!parsed.success) {
    const issues = parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ");
    throw new Error(`invalid api configuration: ${issues}`);
  }
  return parsed.data;
}
