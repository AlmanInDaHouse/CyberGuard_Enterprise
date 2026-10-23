import "reflect-metadata";
import { X509Certificate as NodeX509 } from "node:crypto";
import { chmodSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { connect } from "node:tls";
import * as x509 from "@peculiar/x509";
import { afterAll, afterEach, beforeAll, expect, inject, test } from "vitest";
import type { ServerCa } from "../src/ca.js";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { caSigner, pairWithDaysLeft, serverCa } from "./helpers/server-cert.js";

// deploy_ac_014 — SPEC-023 §Operational §6, ingest's suite: with
// INGEST_SERVER_NAMES=cg.example.test,10.0.0.5, the self-issued certificate's SANs are
// exactly those two and its subject `CN=cg.example.test, O=CyberGuard self-issued`; after a
// restart with another list, it is reissued with that list; an operator-provided pair is not
// rewritten; a key file of mode 0644 that ingest issued is 0600 after start; a reissued
// certificate is served on the enrollment listener without a restart.

const SELF_ISSUED_O = "O=CyberGuard self-issued";

let config: Config;
let ca: ServerCa;
const open: IngestServer[] = [];
const dirs: string[] = [];

beforeAll(async () => {
  config = inject("ingestConfig");
  ca = await serverCa(config);
});
afterEach(async () => {
  while (open.length > 0) await open.pop()?.close();
});
afterAll(() => {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
});

function certDir(): { cert: string; key: string } {
  const dir = mkdtempSync(join(tmpdir(), "cg-deploy-014-"));
  dirs.push(dir);
  return { cert: join(dir, "server.pem"), key: join(dir, "server-key.pem") };
}

async function start(
  paths: { cert: string; key: string },
  names: string[],
  serverCert?: { checkIntervalMs?: number; now?: () => number },
): Promise<IngestServer> {
  const server = await startIngest(
    {
      ...config,
      INGEST_SERVER_CERT_PATH: paths.cert,
      INGEST_SERVER_KEY_PATH: paths.key,
      INGEST_SERVER_NAMES: names,
      INGEST_ENROLL_TLS: true,
      INGEST_DETECT_INTERVAL_MS: 0,
    },
    serverCert ? { serverCert } : {},
  );
  open.push(server);
  return server;
}

function onDisk(path: string): NodeX509 {
  return new NodeX509(readFileSync(path, "utf8"));
}

const sans = (cert: NodeX509): string[] => (cert.subjectAltName ?? "").split(", ").sort();
const subjectParts = (cert: NodeX509): string[] => cert.subject.split("\n").sort();

/** The serial of the certificate a TLS listener presents, trusting the server's CA. */
function servedSerial(url: string): Promise<string> {
  const u = new URL(url);
  return new Promise((resolve, reject) => {
    const socket = connect({
      host: u.hostname,
      port: Number(u.port),
      ca: ca.caCertPem,
      servername: "localhost",
      // The certificate's names are under test, not the host the test dials.
      checkServerIdentity: () => undefined,
      minVersion: "TLSv1.3",
    });
    socket.once("secureConnect", () => {
      resolve(socket.getPeerX509Certificate()?.serialNumber ?? "");
      socket.end();
    });
    socket.once("error", (err) => {
      socket.destroy();
      reject(err);
    });
  });
}

test("the self-issued certificate carries the names as its SANs, and the first as its CN", async () => {
  const paths = certDir();
  await start(paths, ["cg.example.test", "10.0.0.5"]);
  const cert = onDisk(paths.cert);
  expect(sans(cert)).toEqual(["DNS:cg.example.test", "IP Address:10.0.0.5"]);
  expect(subjectParts(cert)).toEqual(["CN=cg.example.test", SELF_ISSUED_O].sort());
});

test("a CN takes the first name's first 64 characters", async () => {
  const paths = certDir();
  const long = `${"a".repeat(60)}.example.test`;
  await start(paths, [long]);
  expect(subjectParts(onDisk(paths.cert))).toEqual(
    [`CN=${long.slice(0, 64)}`, SELF_ISSUED_O].sort(),
  );
});

test("after a restart with another list, the certificate is reissued with that list", async () => {
  const paths = certDir();
  const first = await start(paths, ["cg.example.test", "10.0.0.5"]);
  const before = onDisk(paths.cert).serialNumber;
  await first.close();
  open.pop();
  await start(paths, ["agents.example.test"]);
  const cert = onDisk(paths.cert);
  expect(cert.serialNumber).not.toBe(before);
  expect(sans(cert)).toEqual(["DNS:agents.example.test"]);
});

test("a list in another order is reissued with its first name as the CN", async () => {
  const paths = certDir();
  const first = await start(paths, ["a.example.test", "b.example.test"]);
  await first.close();
  open.pop();
  await start(paths, ["b.example.test", "a.example.test"]);
  expect(subjectParts(onDisk(paths.cert))).toEqual(["CN=b.example.test", SELF_ISSUED_O].sort());
});

test("an IPv6 address is carried as given, and a restart with the same list keeps the pair", async () => {
  const paths = certDir();
  const names = ["cg.example.test", "::ffff:10.0.0.5", "2001:db8::1"];
  const first = await start(paths, names);
  const serial = onDisk(paths.cert).serialNumber;
  const ips = new x509.X509Certificate(readFileSync(paths.cert, "utf8"))
    .getExtension(x509.SubjectAlternativeNameExtension)
    ?.names.toJSON()
    .filter((n) => n.type === "ip")
    .map((n) => n.value);
  // ::ffff:10.0.0.5 is ::ffff:a00:5 in hexadecimal, not ::ffff:10.
  expect(ips).toEqual(["::ffff:a00:5", "2001:db8::1"]);
  await first.close();
  open.pop();
  await start(paths, names);
  expect(onDisk(paths.cert).serialNumber).toBe(serial);
});

test("a self-issued pair of the earlier profile is kept with the default names, and reissued with others", async () => {
  // SPEC-004's profile: CN=localhost, SANs localhost and 127.0.0.1, no O.
  const legacy = await pairWithDaysLeft(caSigner(ca), 80);
  const kept = certDir();
  writeFileSync(kept.cert, legacy.certPem);
  writeFileSync(kept.key, legacy.keyPem, { mode: 0o600 });
  await start(kept, ["localhost", "127.0.0.1"]);
  expect(onDisk(kept.cert).serialNumber).toBe(legacy.serial);

  const moved = certDir();
  writeFileSync(moved.cert, legacy.certPem);
  writeFileSync(moved.key, legacy.keyPem, { mode: 0o600 });
  await start(moved, ["cg.example.test"]);
  const cert = onDisk(moved.cert);
  expect(cert.serialNumber).not.toBe(legacy.serial);
  expect(sans(cert)).toEqual(["DNS:cg.example.test"]);
});

test("an operator-provided pair, one the CA signed included, is not rewritten whatever the names", async () => {
  const profile = {
    subject: "CN=ingest.example.test",
    san: [{ type: "dns", value: "ingest.example.test" }] as x509.JsonGeneralName[],
  };
  const operator = await pairWithDaysLeft(caSigner(ca), 80, profile);
  const paths = certDir();
  writeFileSync(paths.cert, operator.certPem);
  writeFileSync(paths.key, operator.keyPem, { mode: 0o600 });
  await start(paths, ["cg.example.test", "10.0.0.5"]);
  expect(readFileSync(paths.cert, "utf8")).toBe(operator.certPem);
  expect(readFileSync(paths.key, "utf8")).toBe(operator.keyPem);
});

test.skipIf(process.platform === "win32")(
  "a key file ingest issued, of mode 0644, is 0600 after start",
  async () => {
    const paths = certDir();
    const first = await start(paths, ["cg.example.test"]);
    const serial = onDisk(paths.cert).serialNumber;
    await first.close();
    open.pop();
    chmodSync(paths.key, 0o644);
    await start(paths, ["cg.example.test"]);
    expect(statSync(paths.key).mode & 0o777).toBe(0o600);
    // The mode is set on the pair kept, not by a reissue.
    expect(onDisk(paths.cert).serialNumber).toBe(serial);
  },
);

test("a certificate reissued while running is served on the enrollment listener without a restart", async () => {
  // A self-issued pair of today's profile with 20 days left: the running check reissues it.
  const profile = {
    subject: `CN=localhost, ${SELF_ISSUED_O}`,
    san: [
      { type: "dns", value: "localhost" },
      { type: "ip", value: "127.0.0.1" },
    ] as x509.JsonGeneralName[],
  };
  const expiring = await pairWithDaysLeft(caSigner(ca), 20, profile);
  const paths = certDir();
  writeFileSync(paths.cert, expiring.certPem);
  writeFileSync(paths.key, expiring.keyPem, { mode: 0o600 });
  let reissueAllowed = false;
  // The clock the checks read: not yet expiring at start, then 20 days left at the next check.
  const server = await start(paths, ["localhost", "127.0.0.1"], {
    checkIntervalMs: 200,
    now: () => (reissueAllowed ? Date.now() : Date.now() - 30 * 24 * 3600 * 1000),
  });
  expect(await servedSerial(server.enrollUrl)).toBe(expiring.serial);
  reissueAllowed = true;
  const start2 = Date.now();
  let served = expiring.serial;
  while (served === expiring.serial && Date.now() - start2 < 15_000) {
    await new Promise((r) => setTimeout(r, 200));
    served = await servedSerial(server.enrollUrl);
  }
  expect(served).not.toBe(expiring.serial);
  expect(served).toBe(onDisk(paths.cert).serialNumber);
});
