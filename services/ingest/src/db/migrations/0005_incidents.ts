import { type Kysely, sql } from "kysely";

/**
 * SPEC-007 §Data contracts §3 — the `incidents` table (Postgres-only per ADR-0003
 * §Retention; incidents are mutable triage state, like alerts). Groups distinct
 * correlated alerts (ADR-0013 event-time windowing) keyed by `grouping_key` =
 * `<org>::<agent>::<canonical_tactics>::<window_bucket>` (SPEC-007 §Data contracts
 * §4). Extends the chain 0001 → 0002 → 0003 → 0004 under the same advisory lock.
 *
 * - `agent_id` FK → `agents` is production-faithful (Convention #12): every grouped
 *   alert belongs to an enrolled agent, so its incident does too. A synthetic test
 *   tripping this is fixed by enrolling the agent, never by weakening the FK.
 * - `UNIQUE (grouping_key)` makes create-or-update declarative (SPEC-007 §Operational
 *   §1/§4): `INSERT … ON CONFLICT (grouping_key) DO UPDATE` appends `alert_ids` +
 *   bumps `updated_at` WITHOUT clobbering human triage (`status`/`assigned_to`).
 * - `status` CHECK mirrors `incident.json`'s enum; `activity_id` default 1 (Created).
 *
 * Idempotent (CREATE TABLE IF NOT EXISTS). The grouping LOGIC that writes this table
 * lands at the SPEC-007 impl gate; this migration is structure only.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    CREATE TABLE IF NOT EXISTS incidents (
      incident_id   uuid        PRIMARY KEY,                 -- UUIDv7, slice-generated
      org_id        text        NOT NULL DEFAULT 'default',
      agent_id      uuid        NOT NULL REFERENCES agents (agent_id),
      category_uid  smallint    NOT NULL DEFAULT 10,
      class_uid     integer     NOT NULL DEFAULT 10002,      -- CGES Incident
      cg_kind       text        NOT NULL DEFAULT 'incident',
      activity_id   smallint    NOT NULL DEFAULT 1,          -- 1 = Created
      title         text        NOT NULL,
      status        text        NOT NULL DEFAULT 'open'
                      CHECK (status IN ('open', 'acknowledged', 'investigating', 'contained', 'resolved', 'false_positive')),
      cg_mitre      jsonb,                                   -- the canonical grouping tactic-set { tactics:[], techniques:[] }
      alert_ids     uuid[]      NOT NULL,                    -- >= 1; the grouped alerts (uniqueItems)
      assigned_to   text,                                    -- nullable human triage
      grouping_key  text        NOT NULL,                    -- org::agent::canonical_tactics::window_bucket
      window_start  timestamptz NOT NULL,                    -- event-time window the incident covers
      created_at    timestamptz NOT NULL DEFAULT now(),
      updated_at    timestamptz NOT NULL DEFAULT now(),
      CONSTRAINT incidents_grouping_key_unique  UNIQUE (grouping_key),
      CONSTRAINT incidents_alert_ids_nonempty   CHECK (cardinality(alert_ids) >= 1)
    )
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP TABLE IF EXISTS incidents`.execute(db);
}
