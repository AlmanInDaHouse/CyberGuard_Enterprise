// @vitest-environment node
import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, expect, test } from "vitest";

// SPEC-023 §Operational §7 and deploy_ac_013's dashboard half, against the dashboard's
// start-up entry point (`next start` over the build, which the gate and ts-ci run first):
// every response carries the security headers, the CSP with a nonce new per response that
// the page's scripts carry; `GET /api/health` answers `200 { "status": "ok" }`; the
// dashboard refuses an empty `API_BASE_URL` and one that is not `http:` or `https:`, with
// one line, exit 1. deploy_ac_008 asserts the same through the proxy, in the bundle.

const DASHBOARD_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const NEXT_BIN = join(DASHBOARD_DIR, "node_modules", "next", "dist", "bin", "next");
const LINE = /^cg-dashboard: fatal: invalid configuration: API_BASE_URL: [^\n]+$/;

const CSP_WITHOUT_NONCE = [
  "default-src 'self'",
  "script-src 'self' 'nonce-NONCE' 'strict-dynamic'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data:",
  "connect-src 'self'",
  "frame-ancestors 'none'",
  "base-uri 'none'",
  "form-action 'self'",
  "object-src 'none'",
].join("; ");

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const port = (srv.address() as { port: number }).port;
      srv.close(() => resolve(port));
    });
  });
}

/** The environment of a start: this process's, without its API_BASE_URL. */
function baseEnv(): NodeJS.ProcessEnv {
  const env: Record<string, string> = {};
  for (const [name, value] of Object.entries(process.env)) {
    if (value !== undefined && name !== "API_BASE_URL") env[name] = value;
  }
  env.NODE_ENV = "production";
  env.NEXT_TELEMETRY_DISABLED = "1";
  return env as NodeJS.ProcessEnv;
}

let server: ChildProcess | undefined;
let base = "";

beforeAll(async () => {
  if (!existsSync(join(DASHBOARD_DIR, ".next", "BUILD_ID"))) {
    throw new Error("the dashboard is not built: run `pnpm run build` first");
  }
  const port = await freePort();
  base = `http://127.0.0.1:${port}`;
  server = spawn(process.execPath, [NEXT_BIN, "start", "-p", String(port), "-H", "127.0.0.1"], {
    cwd: DASHBOARD_DIR,
    env: { ...baseEnv(), API_BASE_URL: "http://127.0.0.1:9" },
    stdio: "ignore",
  });
  const start = Date.now();
  for (;;) {
    try {
      if ((await fetch(`${base}/api/health`)).status === 200) return;
    } catch {
      // not listening yet
    }
    if (server.exitCode !== null || Date.now() - start > 60_000) {
      throw new Error("the dashboard did not start");
    }
    await new Promise((r) => setTimeout(r, 200));
  }
});

afterAll(() => {
  server?.kill();
});

/** The nonce of a CSP's script-src. */
function nonceOf(csp: string | null): string {
  const m = /'nonce-([^']+)'/.exec(csp ?? "");
  if (!m?.[1]) throw new Error(`no nonce in ${csp}`);
  return m[1];
}

test("GET /api/health answers 200 { status: ok }", async () => {
  const res = await fetch(`${base}/api/health`);
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual({ status: "ok" });
});

test("a page carries the CSP with a nonce new per response, and its scripts carry it", async () => {
  const first = await fetch(`${base}/login`);
  const second = await fetch(`${base}/login`);
  expect(first.status).toBe(200);
  const csp = first.headers.get("content-security-policy");
  const nonce = nonceOf(csp);
  expect(csp).toBe(CSP_WITHOUT_NONCE.replace("NONCE", nonce));
  expect(nonceOf(second.headers.get("content-security-policy"))).not.toBe(nonce);
  const html = await first.text();
  const scripts = [...html.matchAll(/<script\b[^>]*>/g)].map((m) => m[0]);
  expect(scripts.length).toBeGreaterThan(0);
  for (const tag of scripts) expect(tag).toContain(`nonce="${nonce}"`);
});

test.each(["/login", "/api/health", "/no-such-page"])(
  "%s carries nosniff, the same-origin referrer policy and the permissions policy",
  async (path) => {
    const res = await fetch(`${base}${path}`, { redirect: "manual" });
    expect(res.headers.get("x-content-type-options")).toBe("nosniff");
    expect(res.headers.get("referrer-policy")).toBe("same-origin");
    expect(res.headers.get("permissions-policy")).toBe("camera=(), microphone=(), geolocation=()");
    expect(res.headers.get("content-security-policy")).toMatch(/^default-src 'self'; /);
  },
);

test.each([
  ["", "the empty string"],
  ["ftp://api.example.test", "an ftp: URL"],
  ["api:8081", "no URL"],
  ["127.0.0.1:8081", "a host and port without a scheme"],
])("API_BASE_URL=%j (%s) stops the dashboard with one line", async (value) => {
  const port = await freePort();
  const r = spawnSync(
    process.execPath,
    [NEXT_BIN, "start", "-p", String(port), "-H", "127.0.0.1"],
    {
      cwd: DASHBOARD_DIR,
      env: { ...baseEnv(), API_BASE_URL: value },
      encoding: "utf8",
      timeout: 60_000,
    },
  );
  expect(r.status, `${r.stdout}\n${r.stderr}`).toBe(1);
  const lines = r.stderr.trim().split(/\r?\n/);
  expect(lines, r.stderr).toHaveLength(1);
  expect(lines[0]).toMatch(LINE);
});
