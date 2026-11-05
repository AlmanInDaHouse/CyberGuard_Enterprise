// Regenerates `server_issued_client_cert.txt` (a public certificate in PEM,
// named .txt because the repository ignores *.pem): a client certificate issued by
// the ingest server's own `issueClientCert` (services/ingest/src/cert.ts),
// for a fixed agent id and the RFC 8032 test-vector 1 public key, so that
// the agent's DER reader runs over a certificate the server really issues
// (debt #146). No secret is involved: the CA is generated here and thrown
// away, and the agent's key is a published test vector.
//
// From services/ingest, after `pnpm install`:
//   ./node_modules/.bin/tsx ../../agent/cg-agent/tests/custody_fixtures/issue_client_cert.mts
import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { issueClientCert } from "../../../../services/ingest/src/cert.ts";

const require = createRequire(new URL("../../../../services/ingest/package.json", import.meta.url));
const x509 = require("@peculiar/x509") as typeof import("@peculiar/x509");

const AGENT_ID = "01934abc-def0-7000-89ab-000000000146";
const PUBLIC_KEY_HEX = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

const crypto = globalThis.crypto;
x509.cryptoProvider.set(crypto);
const keys = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
  "sign",
  "verify",
])) as CryptoKeyPair;
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
const out = new URL("./server_issued_client_cert.txt", import.meta.url);
writeFileSync(out, `${issued.certPem.replace(/\r\n/g, "\n").trimEnd()}\n`);
console.log(`wrote ${out.pathname}`);
