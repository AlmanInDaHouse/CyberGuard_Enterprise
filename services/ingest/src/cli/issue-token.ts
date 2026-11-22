// SPEC-004 FR-015 — enrollment-token issuance CLI (`task ingest:issue-token`), as SPEC-025
// §Operational §1 amends it: a single-use token for the org `default`, stored as its SHA-256
// under an identifier, whose expiry the database computes, 15 minutes after its `now()`. The
// opaque 32-byte base64url token is printed once to stdout, its identifier and expiry to
// stderr. Any `--org` but `default` is refused with exit 2. No audit row: `audit_log` is the
// api's (SPEC-026). No HTTP admin endpoint in SPEC-004 (§Ratification decision 2).

import pg from "pg";
import { ConfigError, loadConfigFor } from "../config.js";
import { watchPool } from "../store-events.js";
import { tokenSha256 } from "../token.js";

/** The MVP's one org (SPEC-025 §Operational §1). */
const MVP_ORG = "default";

/** The `--org` given, or `default`. */
function parseOrg(argv: string[]): string {
  const i = argv.indexOf("--org");
  if (i !== -1 && argv[i + 1] !== undefined) {
    return argv[i + 1] as string;
  }
  return MVP_ORG;
}

async function main(): Promise<void> {
  const org = parseOrg(process.argv.slice(2));
  if (org !== MVP_ORG) {
    process.stderr.write(
      `cg-ingest issue-token: refused: --org must be '${MVP_ORG}', the one org enrollment admits\n`,
    );
    process.exit(2);
  }
  // SPEC-023 §Data contracts §4: it validates INGEST_PG_URL alone.
  const config = loadConfigFor(["INGEST_PG_URL"]);
  const token = Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString(
    "base64url",
  );

  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  watchPool(pool);
  try {
    const issued = await pool.query<{ token_id: string; expires_at: Date }>(
      `INSERT INTO enrollment_tokens (token_id, token_sha256, org_id, scope, max_uses, expires_at)
       VALUES (gen_random_uuid(), $1, $2, 'enroll', 1, now() + interval '15 minutes')
       RETURNING token_id, expires_at`,
      [tokenSha256(token), org],
    );
    const row = issued.rows[0];
    // The token value goes to stdout exactly once; never logged (FR-014/NFR-005).
    process.stdout.write(`${token}\n`);
    process.stderr.write(
      `cg-ingest: token ${row?.token_id} issued for org '${org}', expires ${row?.expires_at.toISOString()}\n`,
    );
  } finally {
    await pool.end();
  }
}

main().catch((err: unknown) => {
  const line = err instanceof ConfigError ? err.message : String(err);
  process.stderr.write(`cg-ingest issue-token: fatal: ${line}\n`);
  process.exit(1);
});
