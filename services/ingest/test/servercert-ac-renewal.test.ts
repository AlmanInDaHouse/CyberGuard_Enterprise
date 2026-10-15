import { X509Certificate as NodeX509 } from "node:crypto";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { connect as connectTls } from "node:tls";
import { afterEach, beforeAll, describe, expect, inject, test } from "vitest";
import type { ServerCa } from "../src/ca.js";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import {
  type ClientIdentity,
  DAY_MS,
  OTHER_HOST_PROFILE,
  type Pair,
  caSigner,
  clientIdentity,
  handshake,
  inspectPair,
  otherCa,
  pairWithDaysLeft,
  serverCa,
  strayKeyPem,
} from "./helpers/server-cert.js";

// SPEC-004 Amendment 2026-10-15 — the server certificate's renewal (debt #88).
// A self-issued pair (signed by the server's CA) is reissued at start and while
// running once fewer than 30 days remain, it has expired or its key does not
// match; an operator-provided pair is never overwritten and is logged near and
// past its expiry.

interface Line {
  level: "info" | "warn" | "error";
  obj: Record<string, unknown>;
  msg: string;
}

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
  await Promise.allSettled(open.splice(0).map((s) => s.close()));
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

interface Paths {
  certPath: string;
  keyPath: string;
}

function certDir(): Paths {
  const dir = mkdtempSync(join(tmpdir(), "cg-servercert-"));
  dirs.push(dir);
  return { certPath: join(dir, "server.pem"), keyPath: join(dir, "server-key.pem") };
}

function place(paths: Paths, pair: { certPem: string; keyPem: string }): void {
  writeFileSync(paths.certPath, pair.certPem);
  writeFileSync(paths.keyPath, pair.keyPem);
}

function read(paths: Paths): { certPem: string; keyPem: string } {
  return {
    certPem: readFileSync(paths.certPath, "utf8"),
    keyPem: readFileSync(paths.keyPath, "utf8"),
  };
}

async function start(
  paths: Paths,
  serverCert?: { now?: () => number; checkIntervalMs?: number },
): Promise<{ server: IngestServer; lines: Line[] }> {
  const lines: Line[] = [];
  const log = {
    info: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "info", obj, msg }),
    warn: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "warn", obj, msg }),
    error: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "error", obj, msg }),
  };
  const server = await startIngest(
    {
      ...config,
      INGEST_SERVER_CERT_PATH: paths.certPath,
      INGEST_SERVER_KEY_PATH: paths.keyPath,
    },
    { log, serverCert },
  );
  open.push(server);
  return { server, lines };
}

function certEvents(lines: Line[]): Line[] {
  return lines.filter((l) => String(l.obj.event ?? "").startsWith("server_cert_"));
}

/** The served certificate, read by a client that does not verify it. */
function servedSerial(server: IngestServer): Promise<{ serial: string }> {
  const url = new URL(server.heartbeatUrl);
  return new Promise((resolve, reject) => {
    const socket = connectTls({
      host: url.hostname,
      port: Number(url.port),
      cert: client.certPem,
      key: client.keyPem,
      rejectUnauthorized: false,
    });
    socket.once("secureConnect", () => {
      const serial = socket.getPeerX509Certificate()?.serialNumber ?? "";
      socket.end();
      resolve({ serial });
    });
    socket.once("error", reject);
  });
}

/**
 * The events logged after `mark`, from the first `[level, event]` on: a check already
 * running when a test moved the clock may log once more what the old clock decided.
 */
function settledAfter(lines: Line[], mark: number, level: Line["level"], event: string): Line[] {
  const after = certEvents(lines.slice(mark));
  const first = after.findIndex((l) => l.level === level && l.obj.event === event);
  expect(first, `no ${level} ${event} after the clock moved`).toBeGreaterThanOrEqual(0);
  const settled = after.slice(first);
  for (const line of settled) expect([line.level, line.obj.event]).toEqual([level, event]);
  return settled;
}

/** The error code of a connection the listener closes before its handshake; null if it completes. */
function closedBeforeHandshake(server: IngestServer): Promise<string | null> {
  return servedSerial(server).then(
    () => null,
    (err: NodeJS.ErrnoException) => err.code ?? err.message,
  );
}

/** The served certificate's serial, once it differs from `old` (polled). */
async function servedOtherThan(server: IngestServer, old: string): Promise<string> {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const { serial } = await handshake(server.heartbeatUrl, ca.caCertPem, client);
    if (serial !== old) return serial;
    if (Date.now() > deadline) throw new Error(`still serving ${old} after 10 s`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

/** The pair on disk is a fresh self-issued one, 90 days from now, other than `old`. */
function expectFreshPair(paths: Paths, old: Pair | undefined): string {
  const disk = inspectPair(read(paths).certPem, read(paths).keyPem, ca.caCertPem);
  if (old) {
    expect(disk.serial).not.toBe(old.serial);
    // A new key, not the old one under a new certificate (item 2).
    expect(read(paths).keyPem).not.toBe(old.keyPem);
  }
  expect(disk.signedByCa).toBe(true);
  expect(disk.keyMatches).toBe(true);
  expect(Math.abs(disk.notAfterMs - (Date.now() + 90 * DAY_MS))).toBeLessThan(60_000);
  // An hour back, for an agent whose clock lags the server's (item 2).
  expect(Math.abs(disk.notBeforeMs - (Date.now() - 60 * 60 * 1000))).toBeLessThan(60_000);
  // The profile of item 2: subject, SAN and extended key usage serverAuth.
  const cert = new NodeX509(read(paths).certPem);
  expect(cert.subject).toBe("CN=localhost");
  expect(cert.subjectAltName).toBe("DNS:localhost, IP Address:127.0.0.1");
  expect(cert.keyUsage).toEqual(["1.3.6.1.5.5.7.3.1"]);
  return disk.serial;
}

describe("at start", () => {
  test("servercert_ac_001 — an expired self-issued pair is reissued", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), -1);
    place(paths, old);

    const { server, lines } = await start(paths);

    const serial = expectFreshPair(paths, old);
    const served = await handshake(server.heartbeatUrl, ca.caCertPem, client);
    expect(served).toEqual({ serial, protocol: "TLSv1.3" });
    expect(certEvents(lines)).toEqual([
      {
        level: "info",
        obj: expect.objectContaining({
          event: "server_cert_issued",
          reason: "expired",
          cert_path: paths.certPath,
          not_after: expect.any(String),
        }),
        msg: expect.any(String),
      },
    ]);
  });

  test("servercert_ac_002 — a self-issued pair with 10 days left is reissued", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 10);
    place(paths, old);

    const { server, lines } = await start(paths);

    const serial = expectFreshPair(paths, old);
    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(serial);
    expect(certEvents(lines).map((l) => [l.level, l.obj.event, l.obj.reason])).toEqual([
      ["info", "server_cert_issued", "expiring"],
    ]);
  });

  test("servercert_ac_003 — the pair the server issued is its own at the next start", async () => {
    // A round trip: the first start issues (reason absent) with the server's profile;
    // the next start classifies that pair as self-issued and keeps it.
    const paths = certDir();
    const first = await start(paths);
    expect(certEvents(first.lines).map((l) => [l.level, l.obj.event, l.obj.reason])).toEqual([
      ["info", "server_cert_issued", "absent"],
    ]);
    const issued = read(paths);
    const serial = expectFreshPair(paths, undefined);
    await first.server.close();
    open.splice(open.indexOf(first.server), 1);

    const { server, lines } = await start(paths);

    expect(read(paths)).toEqual(issued);
    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(serial);
    expect(certEvents(lines)).toEqual([]);
  });

  test("servercert_ac_003 — a self-issued pair with 60 days left is kept", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 60);
    place(paths, old);

    const { server, lines } = await start(paths);

    expect(read(paths)).toEqual({ certPem: old.certPem, keyPem: old.keyPem });
    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(old.serial);
    expect(certEvents(lines)).toEqual([]);
  });

  test("servercert_ac_004 — the operator's pair is never overwritten", async () => {
    const operator = await otherCa();
    for (const [daysLeft, level, event] of [
      [10, "warn", "server_cert_expiring"],
      [-1, "error", "server_cert_expired"],
    ] as const) {
      const paths = certDir();
      const pair = await pairWithDaysLeft(operator, daysLeft);
      place(paths, pair);

      const { server, lines } = await start(paths);

      expect(read(paths)).toEqual({ certPem: pair.certPem, keyPem: pair.keyPem });
      expect(certEvents(lines)).toEqual([
        {
          level,
          obj: expect.objectContaining({
            event,
            cert_path: paths.certPath,
            not_after: expect.any(String),
          }),
          msg: expect.any(String),
        },
      ]);
      expect((await fetch(`${server.enrollUrl}/health`)).status).toBe(200);
      if (daysLeft > 0) {
        expect((await handshake(server.heartbeatUrl, operator.certPem, client)).serial).toBe(
          pair.serial,
        );
      }
      await server.close();
      open.splice(open.indexOf(server), 1);
    }
  });

  test("servercert_ac_004 — an operator's file with its key before its certificate still serves", async () => {
    // The TLS context reads the file's first certificate and skips other PEM blocks;
    // the classification reads it the same way, so a file that served before serves.
    const operator = await otherCa();
    const paths = certDir();
    const pair = await pairWithDaysLeft(operator, 60);
    const combined = pair.keyPem + pair.certPem;
    place(paths, { certPem: combined, keyPem: pair.keyPem });

    const { server, lines } = await start(paths);

    expect(read(paths)).toEqual({ certPem: combined, keyPem: pair.keyPem });
    expect((await handshake(server.heartbeatUrl, operator.certPem, client)).serial).toBe(
      pair.serial,
    );
    expect(certEvents(lines)).toEqual([]);
  });

  test("servercert_ac_004 — a pair the server's CA signed for another host is kept", async () => {
    const paths = certDir();
    const pair = await pairWithDaysLeft(caSigner(ca), 10, OTHER_HOST_PROFILE);
    place(paths, pair);

    const { server, lines } = await start(paths);

    expect(read(paths)).toEqual({ certPem: pair.certPem, keyPem: pair.keyPem });
    expect((await servedSerial(server)).serial).toBe(pair.serial);
    expect(certEvents(lines).map((l) => [l.level, l.obj.event])).toEqual([
      ["warn", "server_cert_expiring"],
    ]);
    expect((await fetch(`${server.enrollUrl}/health`)).status).toBe(200);
  });

  test("servercert_ac_006 — a key that does not match its self-issued certificate", async () => {
    for (const keyPem of [await strayKeyPem(), "not a key\n"]) {
      const paths = certDir();
      const old = await pairWithDaysLeft(caSigner(ca), 60);
      place(paths, { certPem: old.certPem, keyPem });

      const { server, lines } = await start(paths);

      const serial = expectFreshPair(paths, old);
      expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(serial);
      expect(certEvents(lines).map((l) => [l.level, l.obj.event, l.obj.reason])).toEqual([
        ["info", "server_cert_issued", "key_mismatch"],
      ]);
      await server.close();
      open.splice(open.indexOf(server), 1);
    }
  });

  test("servercert_ac_007 — a reissue that cannot write keeps the current pair", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 10);
    place(paths, old);
    mkdirSync(`${paths.keyPath}.tmp`);

    const { server, lines } = await start(paths);

    expect(read(paths)).toEqual({ certPem: old.certPem, keyPem: old.keyPem });
    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(old.serial);
    expect(certEvents(lines).map((l) => [l.level, l.obj.event, l.obj.cert_path])).toEqual([
      ["error", "server_cert_reissue_failed", paths.certPath],
    ]);
    // The certificate kept is named by its end.
    expect(certEvents(lines)[0]?.obj.not_after).toBe(
      new Date(new NodeX509(old.certPem).validTo).toISOString(),
    );
  });

  test("servercert_ac_007 — with no pair to keep, a failed issue is fatal", async () => {
    const mismatched = await pairWithDaysLeft(caSigner(ca), 60);
    for (const before of [null, { certPem: mismatched.certPem, keyPem: await strayKeyPem() }]) {
      const paths = certDir();
      if (before) place(paths, before);
      mkdirSync(`${paths.keyPath}.tmp`);
      const lines: Line[] = [];
      const sink = (level: Line["level"]) => (obj: Record<string, unknown>, msg: string) =>
        lines.push({ level, obj, msg });

      await expect(
        startIngest(
          {
            ...config,
            INGEST_SERVER_CERT_PATH: paths.certPath,
            INGEST_SERVER_KEY_PATH: paths.keyPath,
          },
          { log: { info: sink("info"), warn: sink("warn"), error: sink("error") } },
        ),
      ).rejects.toThrow();

      const events = certEvents(lines);
      expect(events.map((l) => [l.level, l.obj.event, l.obj.cert_path])).toEqual([
        ["error", "server_cert_reissue_failed", paths.certPath],
      ]);
      expect(events[0]?.obj).not.toHaveProperty("not_after");
    }
  });

  test("servercert_ac_007 — a certificate file that cannot be parsed is fatal", async () => {
    const paths = certDir();
    place(paths, { certPem: "not a certificate\n", keyPem: await strayKeyPem() });
    const lines: Line[] = [];
    const sink = (level: Line["level"]) => (obj: Record<string, unknown>, msg: string) =>
      lines.push({ level, obj, msg });

    await expect(
      startIngest(
        {
          ...config,
          INGEST_SERVER_CERT_PATH: paths.certPath,
          INGEST_SERVER_KEY_PATH: paths.keyPath,
        },
        { log: { info: sink("info"), warn: sink("warn"), error: sink("error") } },
      ),
    ).rejects.toThrow();
    expect(read(paths).certPem).toBe("not a certificate\n");
    expect(certEvents(lines)).toEqual([]);
  });
});

describe("while running", () => {
  test("servercert_ac_005 — reissued without a restart, TLS 1.3 only", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 40);
    place(paths, old);
    let ahead = 0;

    const { server, lines } = await start(paths, {
      now: () => Date.now() + ahead,
      checkIntervalMs: 100,
    });
    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(old.serial);
    expect(certEvents(lines)).toEqual([]);

    ahead = 11 * DAY_MS;
    const serial = await servedOtherThan(server, old.serial);

    expect(expectFreshPair(paths, old)).toBe(serial);
    // Refused for its protocol version, not closed: the swapped context keeps TLS 1.3 only.
    await expect(
      handshake(server.heartbeatUrl, ca.caCertPem, client, { maxVersion: "TLSv1.2" }),
    ).rejects.toThrow(/protocol version/i);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(certEvents(lines).map((l) => [l.level, l.obj.event, l.obj.reason])).toEqual([
      ["info", "server_cert_issued", "expiring"],
    ]);
  });

  test("servercert_ac_007 — a failed reissue keeps serving the current certificate", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 40);
    place(paths, old);
    let ahead = 0;

    const { server, lines } = await start(paths, {
      now: () => Date.now() + ahead,
      checkIntervalMs: 100,
    });
    mkdirSync(`${paths.keyPath}.tmp`);
    ahead = 11 * DAY_MS;
    await new Promise((resolve) => setTimeout(resolve, 500));

    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(old.serial);
    expect(read(paths)).toEqual({ certPem: old.certPem, keyPem: old.keyPem });
    const failed = certEvents(lines);
    expect(failed.length).toBeGreaterThan(0);
    for (const line of failed) {
      expect(line).toEqual({
        level: "error",
        obj: expect.objectContaining({
          event: "server_cert_reissue_failed",
          cert_path: paths.certPath,
          error: expect.any(String),
          not_after: expect.any(String),
        }),
        msg: expect.any(String),
      });
    }
  });
});

describe("while running, files changed", () => {
  test("servercert_ac_009 — an operator's pair written under the server is not overwritten", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 40);
    place(paths, old);
    let ahead = 0;

    const { server, lines } = await start(paths, {
      now: () => Date.now() + ahead,
      checkIntervalMs: 100,
    });
    const operatorPair = await pairWithDaysLeft(await otherCa(), 60);
    place(paths, operatorPair);
    ahead = 11 * DAY_MS;
    await new Promise((resolve) => setTimeout(resolve, 500));

    expect(read(paths)).toEqual({ certPem: operatorPair.certPem, keyPem: operatorPair.keyPem });
    expect((await handshake(server.heartbeatUrl, ca.caCertPem, client)).serial).toBe(old.serial);
    const events = certEvents(lines);
    expect(events.length).toBeGreaterThan(0);
    for (const line of events) {
      expect([line.level, line.obj.event, line.obj.cert_path]).toEqual([
        "warn",
        "server_cert_changed_on_disk",
        paths.certPath,
      ]);
    }

    // Past the served certificate's expiry: an error, and no more handshakes.
    const mark = lines.length;
    ahead = 41 * DAY_MS;
    await new Promise((resolve) => setTimeout(resolve, 500));
    for (const line of settledAfter(lines, mark, "error", "server_cert_expired")) {
      expect(line.obj.cert_path).toBe(paths.certPath);
    }
    expect(await closedBeforeHandshake(server)).toBe("ECONNRESET");
  });
});

describe("while running, a peer's renewal", () => {
  test("servercert_ac_010 — a fresh self-issued pair on disk is adopted", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 40);
    place(paths, old);
    let ahead = 0;

    const { server, lines } = await start(paths, {
      now: () => Date.now() + ahead,
      checkIntervalMs: 100,
    });
    const peer = await pairWithDaysLeft(caSigner(ca), 90);
    place(paths, peer);
    ahead = 11 * DAY_MS;

    expect(await servedOtherThan(server, old.serial)).toBe(peer.serial);
    expect(read(paths)).toEqual({ certPem: peer.certPem, keyPem: peer.keyPem });
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(certEvents(lines).map((l) => [l.level, l.obj.event, l.obj.cert_path])).toEqual([
      ["info", "server_cert_adopted", paths.certPath],
    ]);
  });

  test("servercert_ac_010 — self-issued files that cannot be adopted are reissued", async () => {
    const paths = certDir();
    const old = await pairWithDaysLeft(caSigner(ca), 40);
    place(paths, old);
    let ahead = 0;

    const { server, lines } = await start(paths, {
      now: () => Date.now() + ahead,
      checkIntervalMs: 100,
    });
    const peer = await pairWithDaysLeft(caSigner(ca), 90);
    place(paths, { certPem: peer.certPem, keyPem: await strayKeyPem() });
    ahead = 11 * DAY_MS;

    const serial = await servedOtherThan(server, old.serial);
    expect(serial).not.toBe(peer.serial);
    expect(expectFreshPair(paths, peer)).toBe(serial);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(certEvents(lines).map((l) => [l.level, l.obj.event, l.obj.reason])).toEqual([
      ["info", "server_cert_issued", "expiring"],
    ]);
  });
});

describe("an expired certificate", () => {
  test("servercert_ac_011 — is not served: connections close before the handshake", async () => {
    // An expired operator-provided pair, then an expired self-issued pair whose
    // reissue cannot write.
    const operator = await otherCa();
    const cases = [
      {
        pair: await pairWithDaysLeft(operator, -1),
        blockWrite: false,
        logged: "server_cert_expired",
      },
      {
        pair: await pairWithDaysLeft(operator, 5 / (24 * 60)),
        blockWrite: false,
        logged: "server_cert_expired",
      },
      {
        pair: await pairWithDaysLeft(caSigner(ca), -1),
        blockWrite: true,
        logged: "server_cert_reissue_failed",
      },
    ];
    for (const [i, { pair, blockWrite, logged }] of cases.entries()) {
      const paths = certDir();
      place(paths, pair);
      if (blockWrite) mkdirSync(`${paths.keyPath}.tmp`);

      const { server, lines } = await start(paths);

      expect((await fetch(`${server.enrollUrl}/health`)).status).toBe(200);
      expect(await closedBeforeHandshake(server)).toBe("ECONNRESET");
      expect(read(paths)).toEqual({ certPem: pair.certPem, keyPem: pair.keyPem });
      expect(certEvents(lines).map((l) => [l.level, l.obj.event])).toEqual([["error", logged]]);
      await server.close();
      open.splice(open.indexOf(server), 1);

      if (i === 0) {
        // The operator renews the pair and restarts: the listener serves again.
        const renewed = await pairWithDaysLeft(operator, 60);
        place(paths, renewed);
        const again = await start(paths);
        expect((await handshake(again.server.heartbeatUrl, operator.certPem, client)).serial).toBe(
          renewed.serial,
        );
      }
    }
  });

  test("servercert_ac_011 — a running server's operator pair reaching its expiry", async () => {
    const operator = await otherCa();
    const paths = certDir();
    const pair = await pairWithDaysLeft(operator, 60);
    place(paths, pair);
    let ahead = 0;

    const { server, lines } = await start(paths, {
      now: () => Date.now() + ahead,
      checkIntervalMs: 100,
    });
    expect((await handshake(server.heartbeatUrl, operator.certPem, client)).serial).toBe(
      pair.serial,
    );

    expect(certEvents(lines)).toEqual([]);
    ahead = 40 * DAY_MS;
    await new Promise((resolve) => setTimeout(resolve, 400));
    settledAfter(lines, 0, "warn", "server_cert_expiring");
    expect(certEvents(lines)[0]?.level).toBe("warn");
    expect((await handshake(server.heartbeatUrl, operator.certPem, client)).serial).toBe(
      pair.serial,
    );

    const mark = lines.length;
    ahead = 61 * DAY_MS;
    await new Promise((resolve) => setTimeout(resolve, 400));
    settledAfter(lines, mark, "error", "server_cert_expired");
    expect(await closedBeforeHandshake(server)).toBe("ECONNRESET");
    expect(read(paths)).toEqual({ certPem: pair.certPem, keyPem: pair.keyPem });
  });
});

test.skipIf(process.platform === "win32")(
  "servercert_ac_008 — the key file is written owner-only",
  async () => {
    const paths = certDir();
    const first = await start(paths);
    expect(statSync(paths.keyPath).mode & 0o777).toBe(0o600);
    await first.server.close();
    open.splice(open.indexOf(first.server), 1);

    // The pair placed with the mode a copy would have, then reissued.
    const old = await pairWithDaysLeft(caSigner(ca), 10);
    place(paths, old);
    chmodSync(paths.keyPath, 0o644);
    await start(paths);
    expectFreshPair(paths, old);
    expect(statSync(paths.keyPath).mode & 0o777).toBe(0o600);
  },
);
