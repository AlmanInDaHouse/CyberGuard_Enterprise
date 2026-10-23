import { type ChildProcess, spawn } from "node:child_process";
import { type AddressInfo, type Socket, connect, createServer } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, inject, test } from "vitest";

// A defect SPEC-023's api check surfaced (deploy_ac_017), in ingest too: the shared pool's
// idle clients end when Postgres goes away, an `error` event of the pool that, unheard,
// ended the process. Once started, ingest keeps running and answering /health with its
// stores gone.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * A TCP forwarder on 127.0.0.1 to the host and port of `target`, a store's URL: `url` is
 * `target` through it, and `cut()` closes it and every connection through it.
 */
async function forwarder(target: string): Promise<{ url: string; cut(): Promise<void> }> {
  const to = new URL(target);
  const sockets = new Set<Socket>();
  const server = createServer((client) => {
    const upstream = connect(Number(to.port), to.hostname);
    for (const s of [client, upstream]) {
      sockets.add(s);
      s.on("error", () => {});
      s.on("close", () => {
        client.destroy();
        upstream.destroy();
      });
    }
    client.pipe(upstream).pipe(client);
  });
  await new Promise<void>((resolve) => server.listen({ host: "127.0.0.1", port: 0 }, resolve));
  const via = new URL(target);
  via.hostname = "127.0.0.1";
  via.port = String((server.address() as AddressInfo).port);
  return {
    url: via.toString().replace(/\/$/, ""),
    cut: () =>
      new Promise((resolve) => {
        server.close(() => resolve());
        for (const s of sockets) s.destroy();
      }),
  };
}

/** A port on 127.0.0.1 that was free a moment ago. */
function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => {
      const { port } = server.address() as AddressInfo;
      server.close(() => resolve(port));
    });
  });
}

async function health(port: number): Promise<number | null> {
  try {
    return (await fetch(`http://127.0.0.1:${port}/health`)).status;
  } catch {
    return null;
  }
}

test("once started, ingest keeps running and answers /health with its stores unreachable", async () => {
  const config = inject("ingestConfig");
  const pg = await forwarder(config.INGEST_PG_URL);
  const kv = await forwarder(config.INGEST_REDIS_URL);
  const enrollPort = await freePort();
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([k]) => !k.startsWith("INGEST_")),
  );
  const child: ChildProcess = spawn(process.execPath, ["--import", "tsx", "src/index.ts"], {
    cwd: PACKAGE_DIR,
    env: {
      ...env,
      INGEST_PG_URL: pg.url,
      INGEST_REDIS_URL: kv.url,
      INGEST_CH_URL: config.INGEST_CH_URL,
      INGEST_CH_USER: config.INGEST_CH_USER,
      INGEST_CH_PASSWORD: config.INGEST_CH_PASSWORD,
      INGEST_CH_DB: config.INGEST_CH_DB,
      INGEST_SERVER_CERT_PATH: config.INGEST_SERVER_CERT_PATH,
      INGEST_SERVER_KEY_PATH: config.INGEST_SERVER_KEY_PATH,
      INGEST_CA_PASSPHRASE: config.INGEST_CA_PASSPHRASE,
      INGEST_ENROLL_PORT: String(enrollPort),
      INGEST_HEARTBEAT_PORT: String(await freePort()),
      INGEST_LOG_LEVEL: "warn",
      INGEST_DETECT_INTERVAL_MS: "0",
      INGEST_SOAR_PLAYBOOKS_DIR: config.INGEST_SOAR_PLAYBOOKS_DIR ?? "",
    },
  });
  let stderr = "";
  child.stderr?.setEncoding("utf8").on("data", (d: string) => {
    stderr += d;
  });
  try {
    const start = Date.now();
    while ((await health(enrollPort)) !== 200) {
      if (child.exitCode !== null || Date.now() - start > 60_000) {
        throw new Error(`ingest did not start: ${stderr}`);
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    await Promise.all([pg.cut(), kv.cut()]);
    // Time for the pool to see its idle client end.
    await new Promise((r) => setTimeout(r, 1500));
    expect(child.exitCode, stderr).toBeNull();
    expect(await health(enrollPort)).toBe(200);
  } finally {
    child.kill();
  }
});
