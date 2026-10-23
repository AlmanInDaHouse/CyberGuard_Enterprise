import { spawnSync } from "node:child_process";
import { linkSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { type AddressInfo, type Server, createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import { type Config, loadConfig } from "../src/config.js";
import { startIngest } from "../src/server.js";

// deploy_ac_013 — SPEC-023 §Operational §2 and §Data contracts §3, ingest's half: the
// start-up entry point refuses a malformed value with one line that names the variable,
// `cg-ingest: fatal: invalid configuration: <VAR>: <reason>`, and exits 1; and a rule
// set that fails to load stops ingest with no listener having opened.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const LINE = /^cg-ingest: fatal: invalid configuration: ([A-Z_]+): \S.*$/;

let config: Config;
const dirs: string[] = [];

beforeAll(() => {
  config = inject("ingestConfig");
});
afterAll(() => {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
});

function tempDir(prefix: string): string {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  dirs.push(dir);
  return dir;
}

/** The environment of a start that would succeed: the suite's stores, two fixed ports. */
function validEnv(): Record<string, string> {
  return {
    INGEST_PG_URL: config.INGEST_PG_URL,
    INGEST_CH_URL: config.INGEST_CH_URL,
    INGEST_CH_USER: config.INGEST_CH_USER,
    INGEST_CH_PASSWORD: config.INGEST_CH_PASSWORD,
    INGEST_CH_DB: config.INGEST_CH_DB,
    INGEST_REDIS_URL: config.INGEST_REDIS_URL,
    INGEST_SERVER_CERT_PATH: config.INGEST_SERVER_CERT_PATH,
    INGEST_SERVER_KEY_PATH: config.INGEST_SERVER_KEY_PATH,
    INGEST_CA_PASSPHRASE: config.INGEST_CA_PASSPHRASE,
    INGEST_LOG_LEVEL: "warn",
    INGEST_DETECT_INTERVAL_MS: "10000",
    INGEST_DETECT_RULES_DIR: config.INGEST_DETECT_RULES_DIR,
    INGEST_SOAR_PLAYBOOKS_DIR: config.INGEST_SOAR_PLAYBOOKS_DIR ?? "",
  };
}

/** The process environment without the INGEST_* variables a developer's shell may set. */
function baseEnv(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("INGEST_")));
}

/** Run the start-up entry point (`src/index.ts`, as `pnpm dev` does) with `env`. */
function start(env: Record<string, string>) {
  const r = spawnSync(process.execPath, ["--import", "tsx", "src/index.ts"], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...baseEnv(), ...env },
    timeout: 60_000,
  });
  return { status: r.status, stderr: r.stderr, stdout: r.stdout };
}

/** The start ends with exit 1 and one line on stderr, naming `variable`. */
function expectRefused(env: Record<string, string>, variable: string): void {
  const r = start(env);
  expect(r.status, r.stderr).toBe(1);
  const lines = r.stderr.trim().split(/\r?\n/);
  expect(lines, r.stderr).toHaveLength(1);
  expect(lines[0]).toMatch(LINE);
  expect(LINE.exec(lines[0] ?? "")?.[1]).toBe(variable);
}

const SMTP = {
  INGEST_SMTP_HOST: "relay.example.test",
  INGEST_SMTP_PORT: "587",
  INGEST_SMTP_USER: "cg",
  INGEST_SMTP_PASS: "pass",
  INGEST_SMTP_FROM: "cg@example.test",
  INGEST_NOTIFY_RECIPIENT: "soc@example.test",
};

test.each([
  ["INGEST_ENROLL_PORT", "0"],
  ["INGEST_ENROLL_PORT", "65536"],
  ["INGEST_HEARTBEAT_PORT", "0"],
  ["INGEST_HEARTBEAT_PORT", "65536"],
  ["INGEST_ENROLL_PORT", "80a"],
  // SPEC-023 Amendment 2026-10-23 item 1: decimal digits alone.
  ["INGEST_ENROLL_PORT", " 8080"],
  ["INGEST_ENROLL_PORT", "+8080"],
  ["INGEST_ENROLL_PORT", "8080.0"],
  ["INGEST_HEARTBEAT_PORT", "1e3"],
  ["INGEST_HEARTBEAT_PORT", "0x1F90"],
])("%s=%s is refused", (variable, value) => {
  expectRefused({ ...validEnv(), [variable]: value }, variable);
});

test.each([["0"], ["65536"], [" 587"]])("INGEST_SMTP_PORT=%j is refused", (value) => {
  expectRefused({ ...validEnv(), ...SMTP, INGEST_SMTP_PORT: value }, "INGEST_SMTP_PORT");
});

test("an empty INGEST_SMTP_PORT beside the five others set is refused by the all-or-none rule", () => {
  expectRefused({ ...validEnv(), ...SMTP, INGEST_SMTP_PORT: "" }, "INGEST_SMTP_PORT");
  // Refused as unset, by the all-or-none rule, not as a malformed integer.
  const r = start({ ...validEnv(), ...SMTP, INGEST_SMTP_PORT: "" });
  expect(r.stderr).toContain("all-or-nothing");
});

test("leading zeros are digits (SPEC-023 Amendment 2026-10-23 item 1)", () => {
  const parsed = loadConfig({ ...validEnv(), INGEST_ENROLL_PORT: "08080" });
  expect(parsed.INGEST_ENROLL_PORT).toBe(8080);
});

test("the six notification variables empty count as unset (SPEC-023 Amendment 2026-10-23)", () => {
  const empty = Object.fromEntries(Object.keys(SMTP).map((k) => [k, ""]));
  const parsed = loadConfig({ ...validEnv(), ...empty });
  expect(parsed.INGEST_SMTP_PORT).toBeUndefined();
  expect(parsed.INGEST_SMTP_HOST).toBeUndefined();
});

test.each([["INGEST_SERVER_CERT_PATH"], ["INGEST_SERVER_KEY_PATH"]])(
  "an empty %s is refused",
  (variable) => {
    expectRefused({ ...validEnv(), [variable]: "" }, variable);
  },
);

test("a certificate and a key path that name one file are refused (debt #105)", () => {
  const dir = tempDir("cg-deploy-013-same-");
  expectRefused(
    {
      ...validEnv(),
      INGEST_SERVER_CERT_PATH: join(dir, "server.pem"),
      INGEST_SERVER_KEY_PATH: join(dir, "server.pem"),
    },
    "INGEST_SERVER_KEY_PATH",
  );
  // Another spelling of the same path is the same file.
  expectRefused(
    {
      ...validEnv(),
      INGEST_SERVER_CERT_PATH: join(dir, "server.pem"),
      INGEST_SERVER_KEY_PATH: `${dir}/./sub/../server.pem`,
    },
    "INGEST_SERVER_KEY_PATH",
  );
});

test("a key path through a link to the certificate's directory, files absent, is refused", () => {
  const dir = tempDir("cg-deploy-013-link-");
  const real = join(dir, "real");
  mkdirSync(real);
  const link = join(dir, "link");
  // A junction needs no privilege on Windows; elsewhere a directory symlink.
  symlinkSync(real, link, process.platform === "win32" ? "junction" : "dir");
  expectRefused(
    {
      ...validEnv(),
      INGEST_SERVER_CERT_PATH: join(real, "server.pem"),
      INGEST_SERVER_KEY_PATH: join(link, "server.pem"),
    },
    "INGEST_SERVER_KEY_PATH",
  );
});

test("an existing file reached by a hard link is the same file", () => {
  const dir = tempDir("cg-deploy-013-hard-");
  writeFileSync(join(dir, "server.pem"), "x");
  linkSync(join(dir, "server.pem"), join(dir, "other.pem"));
  expectRefused(
    {
      ...validEnv(),
      INGEST_SERVER_CERT_PATH: join(dir, "server.pem"),
      INGEST_SERVER_KEY_PATH: join(dir, "other.pem"),
    },
    "INGEST_SERVER_KEY_PATH",
  );
});

test.skipIf(process.platform !== "win32")(
  "on Windows, paths that differ in case alone name one file",
  () => {
    const dir = tempDir("cg-deploy-013-case-");
    expectRefused(
      {
        ...validEnv(),
        INGEST_SERVER_CERT_PATH: join(dir, "server.pem"),
        INGEST_SERVER_KEY_PATH: join(dir, "SERVER.pem"),
      },
      "INGEST_SERVER_KEY_PATH",
    );
  },
);

test.each([["verbose"], ["INFO"], [""]])("INGEST_LOG_LEVEL=%j is refused", (value) => {
  expectRefused({ ...validEnv(), INGEST_LOG_LEVEL: value }, "INGEST_LOG_LEVEL");
});

// Item 2 of the amendment: at most 2147483647, the longest delay Node's timers keep.
test.each([[""], ["-1"], ["5s"], ["5e3"], ["2147483648"]])(
  "INGEST_DETECT_INTERVAL_MS=%j is refused",
  (value) => {
    expectRefused({ ...validEnv(), INGEST_DETECT_INTERVAL_MS: value }, "INGEST_DETECT_INTERVAL_MS");
  },
);

test("a missing required variable is refused, named", () => {
  const { INGEST_CA_PASSPHRASE: _, ...env } = validEnv();
  expectRefused(env, "INGEST_CA_PASSPHRASE");
});

/** Bind 127.0.0.1 on a free port and keep it. */
function hold(): Promise<Server> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
  });
}

const portOf = (server: Server): number => (server.address() as AddressInfo).port;
const release = (server: Server): Promise<void> =>
  new Promise((resolve) => server.close(() => resolve()));

test("a rule set that fails to load stops ingest before a listener opens", async () => {
  // Both ports are held: a listener that tried to open would fail with EADDRINUSE
  // before the rules were read.
  const enroll = await hold();
  const heartbeat = await hold();
  try {
    const r = start({
      ...validEnv(),
      INGEST_ENROLL_PORT: String(portOf(enroll)),
      INGEST_HEARTBEAT_PORT: String(portOf(heartbeat)),
      INGEST_DETECT_RULES_DIR: tempDir("cg-deploy-013-no-rules-"),
    });
    expect(r.status, r.stderr).toBe(1);
    const lines = r.stderr.trim().split(/\r?\n/);
    expect(lines, r.stderr).toHaveLength(1);
    expect(lines[0]).toMatch(/^cg-ingest: fatal: .*refusing to start a detection-dark driver/);
    expect(r.stderr).not.toContain("EADDRINUSE");
  } finally {
    await release(enroll);
    await release(heartbeat);
  }
});

test("in process too, the rule set is read before a listener opens", async () => {
  const enroll = await hold();
  const heartbeat = await hold();
  try {
    await expect(
      startIngest({
        ...config,
        INGEST_ENROLL_PORT: portOf(enroll),
        INGEST_HEARTBEAT_PORT: portOf(heartbeat),
        INGEST_DETECT_INTERVAL_MS: 10_000,
        INGEST_DETECT_RULES_DIR: tempDir("cg-deploy-013-no-rules-"),
      }),
    ).rejects.toThrow("refusing to start a detection-dark driver");
  } finally {
    await release(enroll);
    await release(heartbeat);
  }
});

test("with the kill switch, an empty rules directory is not read and the listeners open", async () => {
  const server = await startIngest({
    ...config,
    INGEST_DETECT_INTERVAL_MS: 0,
    INGEST_DETECT_RULES_DIR: tempDir("cg-deploy-013-no-rules-"),
  });
  try {
    expect((await fetch(`${server.enrollUrl}/health`)).status).toBe(200);
  } finally {
    await server.close();
  }
});
