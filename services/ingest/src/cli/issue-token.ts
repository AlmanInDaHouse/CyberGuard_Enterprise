// SPEC-004 FR-015 — enrollment-token issuance CLI (`task ingest:issue-token
// --org <id>`). Inserts an `enrollment_tokens` row in state `issued` with a
// 15-min TTL and prints the opaque 32-byte base64url token once to stdout.
// No HTTP admin endpoint in SPEC-004 (§Ratification decision 2).

import pg from "pg";
import { loadConfig } from "../config.js";

/** ADR-0004 enrollment-token TTL: 15 minutes. */
const TOKEN_TTL_MS = 15 * 60 * 1000;

function parseOrg(argv: string[]): string {
  const i = argv.indexOf("--org");
  if (i !== -1 && argv[i + 1]) {
    return argv[i + 1] as string;
  }
  return "default";
}

async function main(): Promise<void> {
  const config = loadConfig();
  const org = parseOrg(process.argv.slice(2));
  const token = Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString(
    "base64url",
  );
  const expiresAt = new Date(Date.now() + TOKEN_TTL_MS);

  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await pool.query(
      `INSERT INTO enrollment_tokens (token, org_id, scope, state, expires_at)
       VALUES ($1, $2, 'enroll', 'issued', $3)`,
      [token, org, expiresAt],
    );
    // The token value goes to stdout exactly once; never logged (FR-014/NFR-005).
    process.stdout.write(`${token}\n`);
    process.stderr.write(
      `cg-ingest: token issued for org '${org}', expires ${expiresAt.toISOString()}\n`,
    );
  } finally {
    await pool.end();
  }
}

main().catch((err: unknown) => {
  process.stderr.write(`cg-ingest issue-token: ${String(err)}\n`);
  process.exit(1);
});
