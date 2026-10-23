import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:https";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type ConnectionOptions, connect } from "node:tls";
import { afterAll, afterEach, beforeAll, expect, inject, test } from "vitest";
import type { ServerCa } from "../src/ca.js";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import {
  type ClientIdentity,
  caSigner,
  clientIdentity,
  serverCa,
  serverPair,
} from "./helpers/server-cert.js";
import { generateKeyPair, rawPubkeyB64url } from "./helpers/test-client.js";

// deploy_ac_015 — SPEC-023 §Operational §6, ingest's half: with INGEST_ENROLL_TLS on, its
// default, the enrollment listener answers over TLS 1.3 with the server certificate and
// refuses TLS 1.2, `/health` moving with it; each listener refuses a client that offers only
// TLS_AES_128_GCM_SHA256; an agent enrolls over it; a certificate in its last ten minutes is
// served on neither listener. With INGEST_ENROLL_TLS=false the listener keeps plain HTTP.

let config: Config;
let ca: ServerCa;
let client: ClientIdentity;
const open: IngestServer[] = [];
const dirs: string[] = [];

beforeAll(async () => {
  config = inject("ingestConfig");
  ca = await serverCa(config);
  client = await clientIdentity(ca);
});
afterEach(async () => {
  while (open.length > 0) await open.pop()?.close();
});
afterAll(() => {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
});

async function start(overrides: Partial<Config> = {}): Promise<IngestServer> {
  const dir = mkdtempSync(join(tmpdir(), "cg-deploy-015-"));
  dirs.push(dir);
  const server = await startIngest({
    ...config,
    INGEST_SERVER_CERT_PATH: join(dir, "server.pem"),
    INGEST_SERVER_KEY_PATH: join(dir, "server-key.pem"),
    INGEST_ENROLL_TLS: true,
    INGEST_DETECT_INTERVAL_MS: 0,
    ...overrides,
  });
  open.push(server);
  return server;
}

interface Hello {
  protocol: string | null;
  cipher: string;
  serial: string;
}

/** A TLS handshake with `url`'s listener, trusting the server's CA; rejects on failure. */
function hello(url: string, options: ConnectionOptions = {}): Promise<Hello> {
  const u = new URL(url);
  return new Promise((resolve, reject) => {
    const socket = connect({
      host: u.hostname,
      port: Number(u.port),
      ca: ca.caCertPem,
      ...options,
    });
    socket.once("secureConnect", () => {
      resolve({
        protocol: socket.getProtocol(),
        cipher: socket.getCipher().standardName,
        serial: socket.getPeerX509Certificate()?.serialNumber ?? "",
      });
      socket.end();
    });
    socket.once("error", (err) => {
      socket.destroy();
      reject(err);
    });
  });
}

/** An HTTPS request to the enrollment listener, trusting the server's CA. */
function https(
  url: string,
  method: "GET" | "POST",
  body?: unknown,
): Promise<{ status: number; body: string }> {
  return new Promise((resolve, reject) => {
    const data = body === undefined ? undefined : JSON.stringify(body);
    const req = request(
      url,
      {
        method,
        ca: ca.caCertPem,
        headers: data ? { "content-type": "application/json" } : {},
      },
      (res) => {
        let text = "";
        res.setEncoding("utf8");
        res.on("data", (d: string) => {
          text += d;
        });
        res.on("end", () => resolve({ status: res.statusCode ?? 0, body: text }));
      },
    );
    req.once("error", reject);
    if (data) req.write(data);
    req.end();
  });
}

test("the enrollment listener answers over TLS 1.3 with the server certificate, and /health with it", async () => {
  const server = await start();
  expect(server.enrollUrl).toMatch(/^https:\/\//);
  const h = await hello(server.enrollUrl);
  expect(h.protocol).toBe("TLSv1.3");
  expect(["TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"]).toContain(h.cipher);
  // The same certificate as the mTLS listener.
  const mtls = await hello(server.heartbeatUrl, { cert: client.certPem, key: client.keyPem });
  expect(h.serial).toBe(mtls.serial);
  const health = await https(`${server.enrollUrl}/health`, "GET");
  expect(health).toEqual({ status: 200, body: JSON.stringify({ status: "ok" }) });
});

test("the enrollment listener refuses TLS 1.2", async () => {
  const server = await start();
  // A TLS listener: the same client at TLS 1.3 completes the handshake.
  await expect(hello(server.enrollUrl)).resolves.toBeDefined();
  await expect(hello(server.enrollUrl, { maxVersion: "TLSv1.2" })).rejects.toThrow();
});

test("each listener refuses a client that offers only TLS_AES_128_GCM_SHA256", async () => {
  const server = await start();
  const only128 = { ciphers: "TLS_AES_128_GCM_SHA256", minVersion: "TLSv1.3" } as const;
  await expect(hello(server.enrollUrl, only128)).rejects.toThrow();
  await expect(
    hello(server.heartbeatUrl, { ...only128, cert: client.certPem, key: client.keyPem }),
  ).rejects.toThrow();
  // The same client with a suite the server takes completes the handshake.
  const taken = { ciphers: "TLS_AES_256_GCM_SHA384", minVersion: "TLSv1.3" } as const;
  await expect(hello(server.enrollUrl, taken)).resolves.toBeDefined();
});

test("an agent enrolls over the enrollment listener's TLS", async () => {
  const server = await start();
  const token = await issueToken(config);
  const res = await https(`${server.enrollUrl}/v1/agents/enroll`, "POST", {
    envelope_version: "0.1.0",
    enrollment_token: token,
    agent_pubkey: await rawPubkeyB64url(await generateKeyPair()),
    agent_hostname: "deploy-015",
    agent_platform: "windows",
    agent_version: "0.1.0",
  });
  expect(res.status, res.body).toBe(200);
});

test("a certificate in its last ten minutes is served on neither listener", async () => {
  // An operator-provided pair, which the server does not replace, five minutes from its end.
  const notAfter = new Date(Date.now() + 5 * 60 * 1000);
  const pair = await serverPair(caSigner(ca), new Date(Date.now() - 24 * 3600 * 1000), notAfter, {
    subject: "CN=localhost",
    san: [{ type: "dns", value: "localhost" }],
  });
  const dir = mkdtempSync(join(tmpdir(), "cg-deploy-015-exp-"));
  dirs.push(dir);
  writeFileSync(join(dir, "server.pem"), pair.certPem);
  writeFileSync(join(dir, "server-key.pem"), pair.keyPem, { mode: 0o600 });
  const server = await start({
    INGEST_SERVER_CERT_PATH: join(dir, "server.pem"),
    INGEST_SERVER_KEY_PATH: join(dir, "server-key.pem"),
  });
  // Each connection is closed before its handshake: a reset, not a TLS refusal.
  for (const attempt of [
    hello(server.enrollUrl),
    hello(server.heartbeatUrl, { cert: client.certPem, key: client.keyPem }),
  ]) {
    const err = await attempt.then(
      () => null,
      (e: unknown) => e as { code?: string; message?: string },
    );
    expect(err).not.toBeNull();
    expect(`${err?.code} ${err?.message}`).toMatch(/ECONNRESET|socket disconnected|socket hang up/);
  }
});

test("with INGEST_ENROLL_TLS=false the enrollment listener keeps plain HTTP", async () => {
  const server = await start({ INGEST_ENROLL_TLS: false });
  expect(server.enrollUrl).toMatch(/^http:\/\//);
  const res = await fetch(`${server.enrollUrl}/health`);
  expect(res.status).toBe(200);
});
