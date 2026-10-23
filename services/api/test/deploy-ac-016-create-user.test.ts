import { type ChildProcessWithoutNullStreams, spawn, spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import otplib from "otplib";
import pg from "pg";
import { beforeAll, expect, inject, test } from "vitest";
import { verifyPassword } from "../src/auth/password.js";
import type { Config } from "../src/config.js";

// deploy_ac_016 — SPEC-023 §Operational §8 (debt #77), §Data contracts §4: `create-user.js`
// refuses `--password` (exit 2); reads the password from a pipe; prints the provisioning
// URI on standard output and nothing secret on standard error; confirms the factor with a
// valid code, writing `user.totp.enroll`; leaves it unconfirmed after three invalid codes.
// It needs API_PG_URL, API_DB_ENC_PASSPHRASE and API_REDIS_URL alone.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const PASSWORD = "Valid-passw0rd-016";

let config: Config;

beforeAll(() => {
  config = inject("apiConfig");
});

/** The command's three variables, and no other API_* variable. */
function env(): NodeJS.ProcessEnv {
  const base = Object.fromEntries(
    Object.entries(process.env).filter(([k]) => !k.startsWith("API_")),
  );
  return {
    ...base,
    API_PG_URL: config.API_PG_URL,
    API_DB_ENC_PASSPHRASE: config.API_DB_ENC_PASSPHRASE,
    API_REDIS_URL: config.API_REDIS_URL,
  };
}

const ARGS = ["--import", "tsx", "src/cli/create-user.ts"];

async function query<R extends pg.QueryResultRow>(text: string, values: unknown[] = []) {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    return (await pool.query<R>(text, values)).rows;
  } finally {
    await pool.end();
  }
}

/** The command, run as a program, its standard input a pipe the test writes to. */
class Cli {
  readonly child: ChildProcessWithoutNullStreams;
  stdout = "";
  stderr = "";
  readonly exited: Promise<number | null>;

  constructor(argv: string[]) {
    this.child = spawn(process.execPath, [...ARGS, ...argv], { cwd: PACKAGE_DIR, env: env() });
    this.child.stdout.setEncoding("utf8").on("data", (d: string) => {
      this.stdout += d;
    });
    this.child.stderr.setEncoding("utf8").on("data", (d: string) => {
      this.stderr += d;
    });
    this.exited = new Promise((resolve) => this.child.on("exit", (code) => resolve(code)));
  }

  write(line: string): void {
    this.child.stdin.write(`${line}\n`);
  }

  /** The provisioning URI, once the command has printed it. */
  async uri(timeoutMs = 30_000): Promise<string> {
    const start = Date.now();
    while (!/otpauth:\/\/\S+\n/.test(this.stdout)) {
      if (Date.now() - start > timeoutMs) throw new Error(`no URI; stderr: ${this.stderr}`);
      await new Promise((r) => setTimeout(r, 50));
    }
    return (this.stdout.match(/otpauth:\/\/\S+/) as RegExpMatchArray)[0];
  }
}

const secretOf = (uri: string): string => new URL(uri).searchParams.get("secret") ?? "";

/** A six-digit code that no step within two of now accepts for `secret` (the command
 *  accepts one step either side). */
function invalidCode(secret: string, n: number): string {
  const wide = otplib.authenticator.clone({ window: 2 });
  for (let i = n; ; i++) {
    const code = String(100000 + i * 7919).slice(-6);
    if (!wide.check(code, secret)) return code;
  }
}

async function userRow(email: string) {
  return (
    await query<{ user_id: string; totp_enrolled: boolean }>(
      "SELECT user_id::text, totp_enrolled FROM users WHERE email = $1",
      [email],
    )
  )[0];
}

async function enrollRows(userId: string) {
  return query<{ outcome: string }>(
    "SELECT outcome FROM audit_log WHERE action = 'user.totp.enroll' AND user_id = $1 ORDER BY id",
    [userId],
  );
}

test.each([[["--password", PASSWORD]], [[`--password=${PASSWORD}`]]])(
  "--password is refused in either form, exit 2, and nothing is written",
  async (form) => {
    const r = spawnSync(process.execPath, [...ARGS, "--email", "pw@deploy016.test", ...form], {
      cwd: PACKAGE_DIR,
      env: env(),
      encoding: "utf8",
      input: `${PASSWORD}\n`,
    });
    expect(r.status).toBe(2);
    expect(r.stderr).toContain("--password");
    expect(r.stderr).not.toContain(PASSWORD);
    expect(await userRow("pw@deploy016.test")).toBeUndefined();
  },
);

test("--password is refused before the configuration is read: exit 2 with a variable missing", () => {
  const { API_REDIS_URL: _, ...partial } = env();
  const r = spawnSync(
    process.execPath,
    [...ARGS, "--email", "pw2@deploy016.test", "--password", "x"],
    {
      cwd: PACKAGE_DIR,
      env: partial,
      encoding: "utf8",
      input: "",
    },
  );
  expect(r.status).toBe(2);
});

test("a variable it uses, malformed, is refused with its name, exit 1", () => {
  const r = spawnSync(process.execPath, [...ARGS, "--email", "cfg@deploy016.test"], {
    cwd: PACKAGE_DIR,
    env: { ...env(), API_REDIS_URL: "not a url" },
    encoding: "utf8",
    input: `${PASSWORD}\n`,
  });
  expect(r.status).toBe(1);
  expect(r.stderr).toContain("invalid configuration: API_REDIS_URL: must be a URL");
});

test("a password the policy refuses is refused before anything is written, exit 2", async () => {
  const r = spawnSync(process.execPath, [...ARGS, "--email", "weak@deploy016.test"], {
    cwd: PACKAGE_DIR,
    env: env(),
    encoding: "utf8",
    input: "weak\n",
  });
  expect(r.status).toBe(2);
  expect(r.stderr).toContain("refused: password");
  expect(r.stderr).not.toContain("weak");
  expect(await userRow("weak@deploy016.test")).toBeUndefined();
});

test("reads the password from a pipe, prints the URI on stdout, and confirms the factor with a valid code", async () => {
  const cli = new Cli(["--email", "first@deploy016.test"]);
  cli.write(PASSWORD);
  const uri = await cli.uri();
  const secret = secretOf(uri);
  const code = otplib.authenticator.generate(secret);
  cli.write(code);
  expect(await cli.exited, cli.stderr).toBe(0);

  // The URI on stdout, once, and nothing else there.
  expect(cli.stdout).toBe(`${uri}\n`);
  // Nothing secret on stderr: not the password, the secret, the URI or the code.
  for (const secretText of [PASSWORD, secret, uri, code, config.API_DB_ENC_PASSPHRASE]) {
    expect(cli.stderr).not.toContain(secretText);
  }
  const user = await userRow("first@deploy016.test");
  expect(user?.totp_enrolled).toBe(true);
  // The password stored is the pipe's first line, as read.
  const [{ password_hash }] = (await query<{ password_hash: string }>(
    "SELECT password_hash FROM users WHERE email = $1",
    ["first@deploy016.test"],
  )) as [{ password_hash: string }];
  expect(await verifyPassword(password_hash, PASSWORD)).toBe(true);
  expect(await enrollRows(user?.user_id ?? "")).toEqual([{ outcome: "ok" }]);
  // The accepted code is used up, as the route's is.
  const redisKeys = await import("ioredis").then(async ({ Redis }) => {
    const redis = new Redis(config.API_REDIS_URL);
    try {
      return await redis.exists(`totp:replay:${user?.user_id}:${code}`);
    } finally {
      await redis.quit();
    }
  });
  expect(redisKeys).toBe(1);
});

test("three invalid codes leave the factor unconfirmed, and the command says how to confirm it", async () => {
  const cli = new Cli(["--email", "second@deploy016.test", "--role", "analyst"]);
  cli.write(PASSWORD);
  const uri = await cli.uri();
  const secret = secretOf(uri);
  const codes = [0, 1, 2].map((i) => invalidCode(secret, i));
  for (const code of codes) cli.write(code);
  expect(await cli.exited, cli.stderr).toBe(0);
  for (const secretText of [PASSWORD, secret, uri, ...codes]) {
    expect(cli.stderr).not.toContain(secretText);
  }
  const user = await userRow("second@deploy016.test");
  expect(user?.totp_enrolled).toBe(false);
  expect(cli.stderr).toContain("POST /v1/auth/totp/confirm");
  expect(cli.stderr).toContain(user?.user_id ?? "-");
  expect(await enrollRows(user?.user_id ?? "")).toEqual([
    { outcome: "denied" },
    { outcome: "denied" },
    { outcome: "denied" },
  ]);
});

test("--no-confirm skips the question", async () => {
  const cli = new Cli(["--email", "third@deploy016.test", "--no-confirm"]);
  cli.write(PASSWORD);
  await cli.uri();
  cli.child.stdin.end();
  expect(await cli.exited, cli.stderr).toBe(0);
  const user = await userRow("third@deploy016.test");
  expect(user?.totp_enrolled).toBe(false);
  expect(cli.stderr).toContain("POST /v1/auth/totp/confirm");
  expect(await enrollRows(user?.user_id ?? "")).toEqual([]);
});
