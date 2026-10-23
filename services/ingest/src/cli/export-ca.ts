// SPEC-023 §Data contracts §4 and §Operational §6 — prints the ingest CA's certificate as
// PEM, then a line `sha256:<hex>`, the SHA-256 of its DER in lowercase hexadecimal: the
// operator's way to the agents' trust anchor, and the fingerprint that lets the operator
// compare the file an agent holds with the server's CA. It reads the `ca` row and needs
// only INGEST_PG_URL.
import { createHash } from "node:crypto";
import { realpathSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { ConfigError, loadConfigFor } from "../config.js";

/** The DER a PEM certificate carries. */
function derOf(pem: string): Buffer {
  const body = pem
    .replace(/-----BEGIN CERTIFICATE-----/, "")
    .replace(/-----END CERTIFICATE-----[\s\S]*$/, "")
    .replace(/\s+/g, "");
  return Buffer.from(body, "base64");
}

/** The CA's two parts, or null when ingest has not generated the CA yet. */
export async function caExport(pgUrl: string): Promise<string | null> {
  const pool = new pg.Pool({ connectionString: pgUrl });
  try {
    const res = await pool.query<{ cert_pem: string }>("SELECT cert_pem FROM ca WHERE id = 1");
    const pem = res.rows[0]?.cert_pem;
    if (pem === undefined) return null;
    const hex = createHash("sha256").update(derOf(pem)).digest("hex");
    return `${pem.endsWith("\n") ? pem : `${pem}\n`}sha256:${hex}\n`;
  } finally {
    await pool.end();
  }
}

if (
  process.argv[1] &&
  realpathSync(fileURLToPath(import.meta.url)) === realpathSync(path.resolve(process.argv[1]))
) {
  Promise.resolve()
    .then(() => caExport(loadConfigFor(["INGEST_PG_URL"]).INGEST_PG_URL))
    .then((out) => {
      if (out === null) {
        process.stderr.write(
          "cg-ingest export-ca: no CA yet: ingest generates it at its first start\n",
        );
        process.exit(1);
      }
      process.stdout.write(out);
      process.exit(0);
    })
    .catch((e: unknown) => {
      const line = e instanceof ConfigError ? e.message : String(e);
      process.stderr.write(`cg-ingest export-ca: fatal: ${line}\n`);
      process.exit(1);
    });
}
