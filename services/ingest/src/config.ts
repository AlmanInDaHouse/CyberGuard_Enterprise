import { z } from "zod";

/**
 * SPEC-004 §Configuration. Validated at startup; the service refuses to
 * start on a missing/invalid variable.
 */
const EnvSchema = z.object({
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
