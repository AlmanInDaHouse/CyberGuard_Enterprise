import "reflect-metadata";
import * as x509 from "@peculiar/x509";

// A self-signed certificate for 127.0.0.1, for the soar_ac suites' SMTP server that
// offers STARTTLS (SPEC-022 Amendment 2026-10-15). The send is given it as its
// trusted root; nodemailer sets no SNI name for an IP address, so the certificate
// names the address as an IP subject alternative name.

x509.cryptoProvider.set(globalThis.crypto);

export interface LoopbackCert {
  /** The private key, PKCS #8 PEM. */
  key: string;
  /** The certificate, PEM: the server's, and the client's trusted root. */
  cert: string;
}

const ALG = { name: "ECDSA", namedCurve: "P-256", hash: "SHA-256" } as const;

let made: Promise<LoopbackCert> | undefined;

/** One certificate per test process. */
export function loopbackCert(): Promise<LoopbackCert> {
  made ??= make();
  return made;
}

async function make(): Promise<LoopbackCert> {
  const keys = await globalThis.crypto.subtle.generateKey(ALG, true, ["sign", "verify"]);
  const cert = await x509.X509CertificateGenerator.createSelfSigned({
    serialNumber: "01",
    name: "CN=127.0.0.1",
    notBefore: new Date(Date.now() - 60_000),
    notAfter: new Date(Date.now() + 24 * 60 * 60 * 1000),
    signingAlgorithm: ALG,
    keys,
    extensions: [
      new x509.BasicConstraintsExtension(true, undefined, true),
      new x509.KeyUsagesExtension(
        x509.KeyUsageFlags.keyCertSign | x509.KeyUsageFlags.digitalSignature,
        true,
      ),
      new x509.ExtendedKeyUsageExtension([x509.ExtendedKeyUsage.serverAuth]),
      new x509.SubjectAlternativeNameExtension([{ type: "ip", value: "127.0.0.1" }]),
    ],
  });
  const der = await globalThis.crypto.subtle.exportKey("pkcs8", keys.privateKey);
  const lines =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  return {
    key: `-----BEGIN PRIVATE KEY-----\n${lines.join("\n")}\n-----END PRIVATE KEY-----\n`,
    cert: cert.toString("pem"),
  };
}
