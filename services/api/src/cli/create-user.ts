// SPEC-008 §Operational §6 — first-admin bootstrap CLI (`task api:create-user`).
// Seeds a user directly in Postgres (Argon2 hash + pgcrypto-encrypted TOTP secret)
// and prints the TOTP provisioning URI once to stdout — never logged. This is the
// only way to create the FIRST admin; thereafter user creation is admin-only over
// the authed API. No unauthenticated HTTP bootstrap path exists.
import pg from "pg";
import { hashPassword } from "../auth/password.js";
import { generateSecret, provisioningUri } from "../auth/totp.js";
import { loadConfig } from "../config.js";

function arg(argv: string[], name: string): string | undefined {
  const i = argv.indexOf(name);
  return i !== -1 ? argv[i + 1] : undefined;
}

async function main(): Promise<void> {
  const config = loadConfig();
  const argv = process.argv.slice(2);
  const email = arg(argv, "--email");
  const password = arg(argv, "--password");
  const role = arg(argv, "--role") ?? "admin";
  if (!email || !password) {
    throw new Error(
      "usage: create-user --email <email> --password <password> [--role admin|analyst|viewer]",
    );
  }

  const userId = globalThis.crypto.randomUUID();
  const ph = await hashPassword(password);
  const secret = generateSecret();
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
       VALUES ($1, 'default', $2, $3, pgp_sym_encrypt($4, $5), false, $6)`,
      [userId, email, ph, secret, config.API_DB_ENC_PASSPHRASE, role],
    );
    // The provisioning URI goes to stdout exactly once; never logged.
    process.stdout.write(`${provisioningUri(email, secret)}\n`);
    process.stderr.write(
      `cg-api: created ${role} '${email}' (${userId}); add the URI above to an authenticator, then confirm enrollment on first login\n`,
    );
  } finally {
    await pool.end();
  }
}

main().catch((e: unknown) => {
  process.stderr.write(`cg-api create-user: ${String(e)}\n`);
  process.exit(1);
});
