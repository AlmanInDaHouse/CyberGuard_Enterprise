import "reflect-metadata";
import type { webcrypto } from "node:crypto";
import { readFileSync } from "node:fs";
import * as x509 from "@peculiar/x509";
import { expect, test } from "vitest";
import { issueClientCert } from "../src/cert.js";

// SPEC-024 install_ac_005 (debt #146): the agent's DER reader runs over a
// certificate this server issued, kept as a fixture of the agent's tests. It
// is a snapshot, so this test issues one again and compares the two fields
// the reader takes, byte for byte: the subject and the subjectPublicKeyInfo.
// When it fails the server's encoding moved; regenerate the fixture
// (agent/cg-agent/tests/custody_fixtures/issue_client_cert.mts) and run the
// agent's x509_identity tests over it.

const FIXTURE = new URL(
  "../../../agent/cg-agent/tests/custody_fixtures/server_issued_client_cert.txt",
  import.meta.url,
);
// The generator's agent id and RFC 8032 test-vector 1 public key.
const AGENT_ID = "01934abc-def0-7000-89ab-000000000146";
const PUBLIC_KEY_HEX = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

/** The elements of a DER value, each with its tag and length. */
function elements(der: Uint8Array): Uint8Array[] {
  const found: Uint8Array[] = [];
  let at = 0;
  while (at < der.length) {
    const first = der[at + 1] ?? 0;
    const lengthBytes = first < 0x80 ? 0 : first & 0x7f;
    let length = first;
    if (lengthBytes > 0) {
      length = 0;
      for (const byte of der.subarray(at + 2, at + 2 + lengthBytes)) {
        length = length * 256 + byte;
      }
    }
    const end = at + 2 + lengthBytes + length;
    found.push(der.subarray(at, end));
    at = end;
  }
  return found;
}

/** The value of a DER element: what follows its tag and length. */
function value(element: Uint8Array): Uint8Array {
  const first = element[1] ?? 0;
  return element.subarray(2 + (first < 0x80 ? 0 : first & 0x7f));
}

/** A certificate's raw subject and subjectPublicKeyInfo. */
function readerFields(pem: string): { subject: Buffer; spki: Buffer } {
  const [certificate] = elements(new Uint8Array(new x509.X509Certificate(pem).rawData));
  const [tbs] = elements(value(certificate as Uint8Array));
  const fields = elements(value(tbs as Uint8Array));
  // version [0], serialNumber, signature, issuer, validity, subject, spki.
  const subject = fields[5] as Uint8Array;
  const spki = fields[6] as Uint8Array;
  return { subject: Buffer.from(subject), spki: Buffer.from(spki) };
}

test("the fixture of the agent's reader has the fields this server issues", async () => {
  const crypto = globalThis.crypto;
  x509.cryptoProvider.set(crypto);
  const keys = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as webcrypto.CryptoKeyPair;
  const caCert = await x509.X509CertificateGenerator.createSelfSigned(
    {
      serialNumber: "01",
      name: "CN=fixture CA",
      notBefore: new Date(Date.now() - 60_000),
      notAfter: new Date(Date.now() + 365 * 24 * 60 * 60 * 1000),
      keys,
      signingAlgorithm: { name: "Ed25519" },
      extensions: [new x509.BasicConstraintsExtension(true, undefined, true)],
    },
    crypto,
  );
  const issued = await issueClientCert(
    {
      caCertPem: caCert.toString("pem"),
      caCert,
      caPrivateKey: keys.privateKey,
      caPublicKey: keys.publicKey,
    },
    AGENT_ID,
    Uint8Array.from(Buffer.from(PUBLIC_KEY_HEX, "hex")),
  );
  const fresh = readerFields(issued.certPem);
  const fixture = readerFields(readFileSync(FIXTURE, "utf8"));
  expect(fresh.subject.toString("hex")).toBe(fixture.subject.toString("hex"));
  expect(fresh.spki.toString("hex")).toBe(fixture.spki.toString("hex"));
  expect(fresh.subject.toString("latin1")).toContain(AGENT_ID);
});
