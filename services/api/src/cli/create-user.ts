// SPEC-008 §Operational §6 — first-admin bootstrap CLI (`task api:create-user`).
// Seeds a user directly in Postgres (Argon2 hash + pgcrypto-encrypted TOTP secret)
// and prints the TOTP provisioning URI once to stdout — never logged. This is the
// only way to create the FIRST admin; thereafter user creation is admin-only over
// the authed API. No unauthenticated HTTP bootstrap path exists.
//
// SPEC-008 Amendment 2026-10-15 item 13: the address, password and role go through
// the API's schemas and password policy, and the CLI writes the user.create audit
// row, with no acting principal. Its password on the command line is debt #77.
import { realpathSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { appendAudit } from "../auth/audit.js";
import { hashPassword } from "../auth/password.js";
import { CreateUserBody } from "../auth/schemas.js";
import { generateSecret, provisioningUri } from "../auth/totp.js";
import { type Config, loadConfig } from "../config.js";

const ORG = "default";

function arg(argv: string[], name: string): string | undefined {
  const i = argv.indexOf(name);
  return i !== -1 ? argv[i + 1] : undefined;
}

/** A refusal the operator can act on; it never quotes the password. */
export class CliRefusal extends Error {}

/** Create one user from the CLI's arguments (`--email`, `--password`, `--role`). */
export async function createUserFromCli(
  config: Config,
  argv: string[],
): Promise<{ userId: string; email: string; role: string; uri: string }> {
  const parsed = CreateUserBody.safeParse({
    email: arg(argv, "--email") ?? "",
    password: arg(argv, "--password") ?? "",
    role: arg(argv, "--role") ?? "admin",
  });
  if (!parsed.success) {
    const fields = [...new Set(parsed.error.issues.map((i) => String(i.path[0])))].join(", ");
    throw new CliRefusal(
      `refused: ${fields} (an address; a password of 12 to 128 characters with a letter and a digit; a role of admin, analyst or viewer)`,
    );
  }
  const { email, password, role } = parsed.data;
  const userId = globalThis.crypto.randomUUID();
  const ph = await hashPassword(password);
  const secret = generateSecret();
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const db = await pool.connect();
  try {
    await db.query("BEGIN");
    await db.query(
      `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
       VALUES ($1, $2, $3, $4, pgp_sym_encrypt($5, $6), false, $7)`,
      [userId, ORG, email, ph, secret, config.API_DB_ENC_PASSPHRASE, role],
    );
    await appendAudit(db, {
      userId: null,
      orgId: ORG,
      action: "user.create",
      targetId: userId,
      outcome: "ok",
      detail: { role },
    });
    await db.query("COMMIT");
  } catch (err) {
    await db.query("ROLLBACK").catch(() => {});
    if ((err as { code?: unknown }).code === "23505") {
      throw new CliRefusal("refused: an account with that address already exists");
    }
    throw err;
  } finally {
    db.release();
    await pool.end();
  }
  return { userId, email, role, uri: provisioningUri(email, secret) };
}

async function main(): Promise<void> {
  const created = await createUserFromCli(loadConfig(), process.argv.slice(2));
  // The provisioning URI goes to stdout exactly once; never logged.
  process.stdout.write(`${created.uri}\n`);
  process.stderr.write(
    `cg-api: created ${created.role} '${created.email}' (${created.userId}); add the URI above to an authenticator, then confirm enrollment on first login\n`,
  );
}

// Run when invoked as a program, also through a symlinked bin; not when a test imports it.
if (
  process.argv[1] &&
  realpathSync(fileURLToPath(import.meta.url)) === realpathSync(path.resolve(process.argv[1]))
) {
  main().catch((e: unknown) => {
    const usage =
      "usage: create-user --email <email> --password <password> [--role admin|analyst|viewer]";
    process.stderr.write(
      `cg-api create-user: ${e instanceof CliRefusal ? `${e.message}\n${usage}` : String(e)}\n`,
    );
    process.exit(e instanceof CliRefusal ? 2 : 1);
  });
}
