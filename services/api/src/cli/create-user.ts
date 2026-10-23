// SPEC-008 §Operational §6 — first-admin bootstrap CLI (`task api:create-user`).
// Seeds a user directly in Postgres (Argon2 hash + pgcrypto-encrypted TOTP secret)
// and prints the TOTP provisioning URI once to stdout — never logged. This is the
// only way to create the FIRST admin; thereafter user creation is admin-only over
// the authed API. No unauthenticated HTTP bootstrap path exists.
//
// SPEC-008 Amendment 2026-10-15 item 13: the address, password and role go through
// the API's schemas and password policy, and the CLI writes the user.create audit
// row, with no acting principal.
//
// SPEC-023 §Operational §8 (debt #77): the password comes from standard input, never
// from the command line: from a terminal, twice and without echo; from a pipe, its
// first line, and the codes from the lines after it. The command then asks for a
// current code and confirms the factor, as POST /v1/auth/totp/confirm does.
import { realpathSync } from "node:fs";
import * as path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import type { Redis } from "ioredis";
import pg from "pg";
import { appendAudit } from "../auth/audit.js";
import { hashPassword } from "../auth/password.js";
import { CreateUserBody } from "../auth/schemas.js";
import { generateSecret, provisioningUri, verifyTotp } from "../auth/totp.js";
import { type Config, ConfigError, loadConfigFor } from "../config.js";
import { createRedisClient } from "../services.js";

const ORG = "default";
/** The invalid codes after which the factor is left unconfirmed. */
const CONFIRM_ATTEMPTS = 3;

/** The variables the command uses (SPEC-023 §Data contracts §4). */
export const CREATE_USER_VARIABLES = [
  "API_PG_URL",
  "API_DB_ENC_PASSPHRASE",
  "API_REDIS_URL",
] as const;
export type CreateUserConfig = Pick<Config, (typeof CREATE_USER_VARIABLES)[number]>;

/** A refusal the operator can act on; it never quotes the password. */
export class CliRefusal extends Error {}

function arg(argv: string[], name: string): string | undefined {
  const i = argv.indexOf(name);
  return i !== -1 ? argv[i + 1] : undefined;
}

/** What the command reads and writes: the password, the codes, and its two streams. */
export interface CliIo {
  /** The password; from a terminal, asked twice. */
  readPassword(): Promise<string>;
  /** The next code, or null when there is none to read. */
  readCode(): Promise<string | null>;
  out(text: string): void;
  err(text: string): void;
}

/** The address and role of the command line; `--password` is refused. */
export function parseArgs(argv: string[]): { email: string; role: string; confirm: boolean } {
  if (argv.includes("--password")) {
    throw new CliRefusal(
      "refused: --password (the password is read from standard input, never from the command line)",
    );
  }
  return {
    email: arg(argv, "--email") ?? "",
    role: arg(argv, "--role") ?? "admin",
    confirm: !argv.includes("--no-confirm"),
  };
}

/** The user the command created, and the secret it stored for the factor. */
export interface CreatedUser {
  userId: string;
  email: string;
  role: string;
  uri: string;
  secret: string;
}

/** Create one user, refusing an address, a password or a role SPEC-008 refuses. */
export async function createUserFromCli(
  config: CreateUserConfig,
  input: { email: string; password: string; role: string },
): Promise<CreatedUser> {
  const parsed = CreateUserBody.safeParse(input);
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
  return { userId, email, role, uri: provisioningUri(email, secret), secret };
}

/**
 * Confirm the factor with one code, as POST /v1/auth/totp/confirm does (SPEC-008 §6):
 * a valid code that was not used yet is marked used, the user's factor confirmed, and
 * a `user.totp.enroll` row written; an invalid one writes the row with outcome denied.
 */
export async function confirmFactor(
  pool: pg.Pool,
  redis: Redis,
  user: Pick<CreatedUser, "userId" | "secret">,
  code: string,
): Promise<boolean> {
  const valid =
    /^\d{6}$/.test(code) &&
    verifyTotp(user.secret, code) &&
    (await redis.set(`totp:replay:${user.userId}:${code}`, "1", "EX", 90, "NX")) !== null;
  if (valid) {
    await pool.query(
      "UPDATE users SET totp_enrolled = true, updated_at = now() WHERE user_id = $1",
      [user.userId],
    );
  }
  await appendAudit(pool, {
    userId: user.userId,
    orgId: ORG,
    action: "user.totp.enroll",
    outcome: valid ? "ok" : "denied",
  });
  return valid;
}

/** The whole command: 0 when it created the user, 2 when it refused, before writing. */
export async function runCreateUser(
  config: CreateUserConfig,
  argv: string[],
  io: CliIo,
): Promise<number> {
  const { email, role, confirm } = parseArgs(argv);
  const password = await io.readPassword();
  const created = await createUserFromCli(config, { email, password, role });
  // The provisioning URI goes to stdout exactly once; never logged.
  io.out(`${created.uri}\n`);
  io.err(`cg-api: created ${created.role} '${created.email}' (${created.userId})\n`);
  if (!confirm) {
    io.err(
      "cg-api: add the URI above to an authenticator, then confirm the factor with POST /v1/auth/totp/confirm\n",
    );
    return 0;
  }
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const redis = createRedisClient(config.API_REDIS_URL);
  try {
    for (let attempt = 1; attempt <= CONFIRM_ATTEMPTS; attempt++) {
      const code = await io.readCode();
      if (code === null) break;
      if (await confirmFactor(pool, redis, created, code.trim())) {
        io.err("cg-api: the TOTP factor is confirmed\n");
        return 0;
      }
      io.err(`cg-api: that code is not valid (${attempt} of ${CONFIRM_ATTEMPTS})\n`);
    }
  } finally {
    await pool.end();
    redis.disconnect();
  }
  io.err(
    `cg-api: the TOTP factor is not confirmed; confirm it later with POST /v1/auth/totp/confirm, giving the user id ${created.userId} and a current code\n`,
  );
  return 0;
}

/** One line from a terminal, read without echo, the prompt on stderr. */
function readHidden(prompt: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const stdin = process.stdin;
    process.stderr.write(prompt);
    stdin.setRawMode(true);
    stdin.resume();
    stdin.setEncoding("utf8");
    let value = "";
    const done = (result: string | Error) => {
      stdin.setRawMode(false);
      stdin.pause();
      stdin.off("data", onData);
      process.stderr.write("\n");
      if (result instanceof Error) reject(result);
      else resolve(result);
    };
    const onData = (chunk: string) => {
      for (const ch of chunk) {
        if (ch === "\r" || ch === "\n") return done(value);
        if (ch === "\u0003") return done(new CliRefusal("refused: interrupted"));
        if (ch === "\u007f" || ch === "\b") value = value.slice(0, -1);
        else value += ch;
      }
    };
    stdin.on("data", onData);
  });
}

/** The terminal's or the pipe's way of reading the password and the codes. */
function processIo(): CliIo {
  const out = (text: string) => process.stdout.write(text);
  const err = (text: string) => process.stderr.write(text);
  if (process.stdin.isTTY) {
    return {
      async readPassword() {
        const first = await readHidden("Password: ");
        const again = await readHidden("Password again: ");
        if (first !== again) throw new CliRefusal("refused: the two passwords differ");
        return first;
      },
      readCode: () => readHidden("Current TOTP code: "),
      out,
      err,
    };
  }
  // A pipe: the password on the first line, a code on each line after it.
  const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY })[
    Symbol.asyncIterator
  ]();
  const next = async (): Promise<string | null> => {
    const r = await lines.next();
    return r.done ? null : r.value;
  };
  return {
    readPassword: async () => (await next()) ?? "",
    readCode: next,
    out,
    err,
  };
}

const USAGE =
  "usage: create-user --email <email> [--role admin|analyst|viewer] [--no-confirm] (the password on standard input)";

// Run when invoked as a program, also through a symlinked bin; not when a test imports it.
if (
  process.argv[1] &&
  realpathSync(fileURLToPath(import.meta.url)) === realpathSync(path.resolve(process.argv[1]))
) {
  Promise.resolve()
    .then(() =>
      runCreateUser(loadConfigFor(CREATE_USER_VARIABLES), process.argv.slice(2), processIo()),
    )
    .then((code) => process.exit(code))
    .catch((e: unknown) => {
      if (e instanceof CliRefusal) {
        process.stderr.write(`cg-api create-user: ${e.message}\n${USAGE}\n`);
        process.exit(2);
      }
      const line = e instanceof ConfigError ? e.message : String(e);
      process.stderr.write(`cg-api create-user: fatal: ${line}\n`);
      process.exit(1);
    });
}
