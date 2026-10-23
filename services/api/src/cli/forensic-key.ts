// SPEC-023 §Data contracts §4 (ADR-0016 Amendment 2026-10-19 item 1) — prints the forensic
// public key in base64url, then its fingerprint, `sha256:<hex>`, which the operator
// records outside CyberGuard at installation. It reads the `forensic_key` row, never an
// export, and needs only API_PG_URL.
import pg from "pg";
import { ConfigError, loadConfigFor } from "../config.js";
import { fingerprintOf } from "../forensic/fingerprint.js";
import { isMain } from "../is-main.js";
import { watchPool } from "../store-events.js";

/** The key's two lines, or null when the api has not generated it yet. */
export async function forensicKeyLines(pgUrl: string): Promise<string | null> {
  const pool = new pg.Pool({ connectionString: pgUrl });
  watchPool(pool);
  try {
    const res = await pool.query<{ public_key: Buffer }>(
      "SELECT public_key FROM forensic_key WHERE id = 1",
    );
    const row = res.rows[0];
    if (!row) return null;
    const raw = new Uint8Array(row.public_key);
    return `${Buffer.from(raw).toString("base64url")}\n${await fingerprintOf(raw)}\n`;
  } finally {
    await pool.end();
  }
}

if (isMain(import.meta.url)) {
  Promise.resolve()
    .then(() => forensicKeyLines(loadConfigFor(["API_PG_URL"]).API_PG_URL))
    .then((lines) => {
      if (lines === null) {
        process.stderr.write(
          "cg-api forensic-key: no forensic key yet: the api generates it at its first start\n",
        );
        process.exit(1);
      }
      process.stdout.write(lines);
      process.exit(0);
    })
    .catch((e: unknown) => {
      const line = e instanceof ConfigError ? e.message : String(e);
      process.stderr.write(`cg-api forensic-key: fatal: ${line}\n`);
      process.exit(1);
    });
}
