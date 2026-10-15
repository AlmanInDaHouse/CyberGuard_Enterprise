import "reflect-metadata";
import { X509Certificate as NodeX509, createPrivateKey, type webcrypto } from "node:crypto";
import { connect } from "node:tls";
import * as x509 from "@peculiar/x509";
import pg from "pg";
import { type ServerCa, ensureCa } from "../../src/ca.js";
import { issueClientCert } from "../../src/cert.js";
import type { Config } from "../../src/config.js";

// Server TLS identities for the servercert_ac suite (SPEC-004 Amendment
// 2026-10-15): pairs with a chosen validity, signed by the server's CA or by
// another one, and a TLS client that dials the heartbeat listener as an agent
// does, trusting one CA and checking validity with the system clock.

x509.cryptoProvider.set(globalThis.crypto);

const ED = { name: "Ed25519" } as const;
export const DAY_MS = 24 * 60 * 60 * 1000;

async function ed25519Keys(): Promise<webcrypto.CryptoKeyPair> {
  return (await globalThis.crypto.subtle.generateKey(ED, true, [
    "sign",
    "verify",
  ])) as webcrypto.CryptoKeyPair;
}

async function pkcs8Pem(key: webcrypto.CryptoKey): Promise<string> {
  const der = await globalThis.crypto.subtle.exportKey("pkcs8", key);
  const lines =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  return `-----BEGIN PRIVATE KEY-----\n${lines.join("\n")}\n-----END PRIVATE KEY-----\n`;
}

/** The server's CA, read from the test Postgres the server reads it from. */
export async function serverCa(config: Config): Promise<ServerCa> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL, max: 1 });
  try {
    return await ensureCa(pool, config.INGEST_CA_PASSPHRASE);
  } finally {
    await pool.end();
  }
}

/** What signs a server certificate: a CA's subject and private key. */
export interface Signer {
  subject: string;
  privateKey: webcrypto.CryptoKey;
}

export function caSigner(ca: ServerCa): Signer {
  return { subject: ca.caCert.subject, privateKey: ca.caPrivateKey };
}

/** Another CA, self-signed: an operator's PKI. */
export async function otherCa(): Promise<Signer & { certPem: string }> {
  const keys = await ed25519Keys();
  const cert = await x509.X509CertificateGenerator.createSelfSigned({
    serialNumber: "0a",
    name: "CN=Operator Test CA",
    notBefore: new Date(Date.now() - DAY_MS),
    notAfter: new Date(Date.now() + 365 * DAY_MS),
    signingAlgorithm: ED,
    keys,
    extensions: [
      new x509.BasicConstraintsExtension(true, undefined, true),
      new x509.KeyUsagesExtension(x509.KeyUsageFlags.keyCertSign, true),
    ],
  });
  return { subject: cert.subject, privateKey: keys.privateKey, certPem: cert.toString("pem") };
}

export interface Pair {
  certPem: string;
  keyPem: string;
  /** The serial as Node prints it, to compare with a handshake's. */
  serial: string;
}

/** The profile the server issues: `CN=localhost`, SAN localhost and 127.0.0.1. */
const SERVER_PROFILE = {
  subject: "CN=localhost",
  san: [
    { type: "dns", value: "localhost" },
    { type: "ip", value: "127.0.0.1" },
  ] as x509.JsonGeneralName[],
};

/** A server pair, by default with the server's profile, signed by `signer`. */
export async function serverPair(
  signer: Signer,
  notBefore: Date,
  notAfter: Date,
  profile = SERVER_PROFILE,
): Promise<Pair> {
  const keys = await ed25519Keys();
  const cert = await x509.X509CertificateGenerator.create({
    serialNumber: Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(16))).toString(
      "hex",
    ),
    subject: profile.subject,
    issuer: signer.subject,
    notBefore,
    notAfter,
    signingAlgorithm: ED,
    publicKey: keys.publicKey,
    signingKey: signer.privateKey,
    extensions: [
      new x509.KeyUsagesExtension(x509.KeyUsageFlags.digitalSignature, true),
      new x509.ExtendedKeyUsageExtension([x509.ExtendedKeyUsage.serverAuth], true),
      new x509.SubjectAlternativeNameExtension(profile.san),
    ],
  });
  const certPem = cert.toString("pem");
  return {
    certPem,
    keyPem: await pkcs8Pem(keys.privateKey),
    serial: new NodeX509(certPem).serialNumber,
  };
}

/** A pair with `daysLeft` of validity, issued 90 days before its end. */
export function pairWithDaysLeft(
  signer: Signer,
  daysLeft: number,
  profile = SERVER_PROFILE,
): Promise<Pair> {
  const notAfter = new Date(Date.now() + daysLeft * DAY_MS);
  return serverPair(signer, new Date(notAfter.getTime() - 90 * DAY_MS), notAfter, profile);
}

/** An operator's profile: another host's name. */
export const OTHER_HOST_PROFILE = {
  subject: "CN=ingest.example.test",
  san: [{ type: "dns", value: "ingest.example.test" }] as x509.JsonGeneralName[],
};

/** A private key that belongs to no certificate. */
export async function strayKeyPem(): Promise<string> {
  return pkcs8Pem((await ed25519Keys()).privateKey);
}

export interface ClientIdentity {
  certPem: string;
  keyPem: string;
}

/** A client certificate from the server's CA, as an enrolled agent holds one. */
export async function clientIdentity(ca: ServerCa): Promise<ClientIdentity> {
  const keys = await ed25519Keys();
  const raw = new Uint8Array(await globalThis.crypto.subtle.exportKey("raw", keys.publicKey));
  const issued = await issueClientCert(ca, "01934abc-def0-7000-89ab-00000000c001", raw);
  return { certPem: issued.certPem, keyPem: await pkcs8Pem(keys.privateKey) };
}

export interface Handshake {
  serial: string;
  protocol: string | null;
}

/**
 * Dial the heartbeat listener as an agent does: trust only `trustedCaPem`, check
 * validity with the system clock, present `client`. Resolves with the served
 * certificate once the handshake verifies; rejects with the TLS error otherwise.
 */
export function handshake(
  heartbeatUrl: string,
  trustedCaPem: string,
  client: ClientIdentity,
  options: { maxVersion?: "TLSv1.2" | "TLSv1.3" } = {},
): Promise<Handshake> {
  const url = new URL(heartbeatUrl);
  return new Promise((resolve, reject) => {
    const socket = connect({
      host: url.hostname,
      port: Number(url.port),
      ca: trustedCaPem,
      cert: client.certPem,
      key: client.keyPem,
      maxVersion: options.maxVersion ?? "TLSv1.3",
      rejectUnauthorized: true,
    });
    socket.once("secureConnect", () => {
      const result = {
        serial: socket.getPeerX509Certificate()?.serialNumber ?? "",
        protocol: socket.getProtocol(),
      };
      socket.end();
      resolve(result);
    });
    socket.once("error", (err) => {
      socket.destroy();
      reject(err);
    });
  });
}

export interface OnDisk {
  serial: string;
  notBeforeMs: number;
  notAfterMs: number;
  signedByCa: boolean;
  keyMatches: boolean;
}

/** What a pair on disk holds: its serial, its end, its issuer's signature, its key. */
export function inspectPair(certPem: string, keyPem: string, caCertPem: string): OnDisk {
  const cert = new NodeX509(certPem);
  let keyMatches: boolean;
  try {
    keyMatches = cert.checkPrivateKey(createPrivateKey(keyPem));
  } catch {
    keyMatches = false;
  }
  return {
    serial: cert.serialNumber,
    notBeforeMs: new Date(cert.validFrom).getTime(),
    notAfterMs: new Date(cert.validTo).getTime(),
    signedByCa: cert.verify(new NodeX509(caCertPem).publicKey),
    keyMatches,
  };
}
