// What the bundle's tests share (SPEC-023 §Acceptance criteria): the bundle's directory, a
// copy of it as an operator unpacks it, its .env, a test CA and the dashboard's pair in
// provided mode, Docker and Compose run for a result, and TOTP codes for the first admin.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHmac, randomBytes } from "node:crypto";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { connect as tlsConnect } from "node:tls";

export const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
export const BUNDLE = join(REPO, "deploy", "docker");
export const TAG = process.env.CG_TEST_IMAGE_TAG || "test";

/** The temporary directories a test file made, removed when it exits. */
const made = [];
process.on("exit", () => {
  for (const dir of made) rmSync(dir, { recursive: true, force: true });
});
function tempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  made.push(dir);
  return dir;
}

/** POSIX root: setup.sh gives the TLS key to the proxy's user, which needs it. */
export const IS_ROOT = typeof process.getuid === "function" && process.getuid() === 0;

/** The secrets setup.sh generates (SPEC-023 §Data contracts §3). */
export const SECRETS = [
  "CG_PG_SUPERUSER_PASSWORD",
  "CG_PG_INGEST_OWNER_PASSWORD",
  "CG_PG_INGEST_PASSWORD",
  "CG_PG_API_OWNER_PASSWORD",
  "CG_PG_API_PASSWORD",
  "CG_CH_ADMIN_PASSWORD",
  "CG_CH_INGEST_PASSWORD",
  "CG_CH_API_PASSWORD",
  "CG_REDIS_INGEST_PASSWORD",
  "CG_REDIS_API_PASSWORD",
  "CG_INGEST_CA_PASSPHRASE",
  "CG_API_DB_ENC_PASSPHRASE",
  "CG_API_FORENSIC_PASSPHRASE",
  "CG_BACKUP_PASSPHRASE",
];

/** Runs a program; returns its status and output. */
export function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...opts });
  if (r.error) throw r.error;
  return r;
}

/** Runs a program and returns its trimmed standard output, failing on a non-zero exit. */
export function ok(cmd, args, opts = {}) {
  const r = run(cmd, args, opts);
  assert.equal(r.status, 0, `${cmd} ${args.join(" ")}\n${r.stdout}\n${r.stderr}`);
  return r.stdout.trim();
}

/**
 * A copy of the bundle as an operator has it: without the repository's own parts (test/,
 * proxy/), with the tests' override that names the images built from the commit.
 */
export function copyBundle() {
  const dir = tempDir("cg-bundle-");
  cpSync(BUNDLE, dir, {
    recursive: true,
    filter: (src) => {
      const rel = src.slice(BUNDLE.length).replaceAll("\\", "/");
      return !/^\/(test|proxy)(\/|$)/.test(rel) && !/^\/\.env$/.test(rel);
    },
  });
  return dir;
}

/** .env's variables, the last value of each. */
export function readEnv(dir) {
  const vars = new Map();
  for (const line of readFileSync(join(dir, ".env"), "utf8").split("\n")) {
    const m = /^([A-Z0-9_]+)=(.*)$/.exec(line);
    if (m) vars.set(m[1], m[2]);
  }
  return vars;
}

/** Sets variables in .env, in place. */
export function setEnv(dir, values) {
  const file = join(dir, ".env");
  let text = readFileSync(file, "utf8");
  for (const [name, value] of Object.entries(values)) {
    const re = new RegExp(`^${name}=.*$`, "m");
    text = re.test(text) ? text.replace(re, () => `${name}=${value}`) : `${text}${name}=${value}\n`;
  }
  writeFileSync(file, text);
}

/** A free TCP port on the loopback. */
export function freePort() {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

/**
 * A test CA and, signed by it, the dashboard's certificate for `hostname`, written as the
 * bundle's tls/dashboard.crt and tls/dashboard.key. Returns the CA's PEM.
 */
export function makeDashboardTls(dir, hostname, additionalNames = []) {
  const tls = join(dir, "tls");
  const work = tempDir("cg-tls-");
  const ec = ["-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256", "-nodes"];
  ok("openssl", ["req", "-x509", ...ec, "-keyout", join(work, "ca.key"), "-out", join(work, "ca.crt"),
    "-subj", "/CN=CyberGuard bundle test CA", "-days", "2",
    "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "keyUsage=critical,keyCertSign,cRLSign"]);
  ok("openssl", ["req", ...ec, "-keyout", join(tls, "dashboard.key"), "-out", join(work, "leaf.csr"),
    "-subj", `/CN=${hostname}`]);
  writeFileSync(join(work, "leaf.ext"), `subjectAltName=${[hostname, ...additionalNames].map((n) => `DNS:${n}`).join(",")}\nextendedKeyUsage=serverAuth\n`);
  ok("openssl", ["x509", "-req", "-in", join(work, "leaf.csr"), "-CA", join(work, "ca.crt"),
    "-CAkey", join(work, "ca.key"), "-CAcreateserial", "-days", "2", "-extfile", join(work, "leaf.ext"),
    "-out", join(tls, "dashboard.crt")]);
  return readFileSync(join(work, "ca.crt"), "utf8");
}

/** A server-side handshake decision, without HTTP. Client certificate verification
 * is deliberately disabled: an unknown SNI must fail at the server, not merely
 * because the client's certificate-name check rejected a completed handshake.
 * A timeout/unreachable listener fails the test rather than counting as refusal. */
export function tlsHandshake(port, servername) {
  return new Promise((resolve, reject) => {
    const socket = tlsConnect({ host: "127.0.0.1", port, servername, rejectUnauthorized: false });
    const timer = setTimeout(() => finish(undefined, new Error("TLS probe timed out")), 5000);
    let done = false;
    function finish(accepted, error) {
      if (done) return;
      done = true;
      clearTimeout(timer);
      socket.destroy();
      if (error) reject(error); else resolve(accepted);
    }
    socket.once("secureConnect", () => finish(true));
    socket.once("error", (error) => {
      if (error.code === "ECONNRESET" || /^ERR_SSL_.*ALERT/.test(error.code ?? "")) finish(false);
      else finish(undefined, error);
    });
  });
}

/** RFC 6238 TOTP (SHA-1, 30 s, six digits) of a base32 secret, at `atMs`. */
export function totp(secretBase32, atMs = Date.now()) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = "";
  for (const ch of secretBase32.replace(/=+$/, "").toUpperCase()) {
    const v = alphabet.indexOf(ch);
    if (v < 0) throw new Error("not base32");
    bits += v.toString(2).padStart(5, "0");
  }
  const key = Buffer.from((bits.match(/.{8}/g) ?? []).map((b) => Number.parseInt(b, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(atMs / 30_000)));
  const mac = createHmac("sha1", key).update(counter).digest();
  const offset = mac[mac.length - 1] & 0x0f;
  const n = (mac.readUInt32BE(offset) & 0x7fffffff) % 1_000_000;
  return String(n).padStart(6, "0");
}

/**
 * A code the server accepts now that no earlier sign-in or confirmation used: it accepts one
 * step either side, and refuses a code it saw within 90 s. Waits for the next step when the
 * three are used.
 */
export async function freshCode(secret, used) {
  for (;;) {
    const now = Date.now();
    for (const at of [now, now + 30_000, now - 30_000]) {
      const code = totp(secret, at);
      if (!used.has(code)) {
        used.add(code);
        return code;
      }
    }
    await new Promise((r) => setTimeout(r, 30_000 - (now % 30_000) + 500));
  }
}

/** A password SPEC-008 accepts. */
export function strongPassword() {
  return `Cg-${randomBytes(12).toString("base64url")}-9z`;
}

/** Runs a program with its standard input a pipe the caller writes to. */
export function interactive(cmd, args, opts = {}) {
  const child = spawn(cmd, args, opts);
  const io = { child, stdout: "", stderr: "" };
  child.stdout.setEncoding("utf8").on("data", (d) => {
    io.stdout += d;
  });
  child.stderr.setEncoding("utf8").on("data", (d) => {
    io.stderr += d;
  });
  io.exited = new Promise((resolve) => child.on("exit", (code) => resolve(code)));
  io.write = (line) => child.stdin.write(`${line}\n`);
  io.waitFor = async (re, timeoutMs = 60_000) => {
    const start = Date.now();
    while (!re.test(io.stdout)) {
      if (Date.now() - start > timeoutMs) throw new Error(`timed out waiting for ${re}: ${io.stderr}`);
      await new Promise((r) => setTimeout(r, 100));
    }
    return io.stdout.match(re);
  };
  return io;
}
