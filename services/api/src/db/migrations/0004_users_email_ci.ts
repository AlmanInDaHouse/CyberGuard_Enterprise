import { type Kysely, sql } from "kysely";

/**
 * SPEC-008 Amendment 2026-10-15 item 3 — case-insensitive email uniqueness per org
 * (§Data contracts §1). Every stored address is rewritten to its trimmed, lower-cased
 * form, computed with JavaScript's `trim` and `toLowerCase` as the api computes it at
 * every entry, and a unique index on `(org_id, lower(email))` follows.
 *
 * If two addresses of one org share that form, by JavaScript's functions or by
 * Postgres's `lower()`, the migration fails before it changes anything: the migrator
 * runs it in a transaction. Its message names the org and the number of such
 * addresses, never an address; the operator resolves them and runs it again.
 */
function refuse(org: string, count: number): never {
  throw new Error(
    `migration 0004_users_email_ci: org "${org}" holds ${count} addresses that are equal once trimmed and lower-cased; resolve them, then run the migrations again`,
  );
}

export async function up(db: Kysely<unknown>): Promise<void> {
  const rows = await sql<{ user_id: string; org_id: string; email: string }>`
    SELECT user_id, org_id, email FROM users
  `.execute(db);

  const byOrg = new Map<string, Map<string, number>>();
  for (const row of rows.rows) {
    const forms = byOrg.get(row.org_id) ?? new Map<string, number>();
    const form = row.email.trim().toLowerCase();
    forms.set(form, (forms.get(form) ?? 0) + 1);
    byOrg.set(row.org_id, forms);
  }
  for (const [org, forms] of byOrg) {
    const clashing = [...forms.values()].filter((n) => n > 1).reduce((a, n) => a + n, 0);
    if (clashing > 0) refuse(org, clashing);
  }

  for (const row of rows.rows) {
    const form = row.email.trim().toLowerCase();
    if (form !== row.email) {
      await sql`UPDATE users SET email = ${form} WHERE user_id = ${row.user_id}`.execute(db);
    }
  }

  // Postgres's lower() can differ from JavaScript's on some non-ASCII text.
  const pgClash = await sql<{ org_id: string; n: string }>`
    SELECT org_id, sum(c)::text AS n
    FROM (SELECT org_id, count(*) AS c FROM users GROUP BY org_id, lower(email) HAVING count(*) > 1) d
    GROUP BY org_id
  `.execute(db);
  const first = pgClash.rows[0];
  if (first) refuse(first.org_id, Number(first.n));

  await sql`
    CREATE UNIQUE INDEX IF NOT EXISTS users_email_org_lower_unique ON users (org_id, lower(email))
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP INDEX IF EXISTS users_email_org_lower_unique`.execute(db);
}
