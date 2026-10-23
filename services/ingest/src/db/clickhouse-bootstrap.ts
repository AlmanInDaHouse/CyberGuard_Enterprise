import type { ClickHouseClient } from "@clickhouse/client";

/** The days a row stays after its arrival (SPEC-023 §Operational §9). */
export interface Retention {
  processDays: number;
  networkDays: number;
  logonDays: number;
  heartbeatDays: number;
}

/** The bounds of each period, in days (SPEC-023 §Data contracts §3). */
export const RETENTION_MIN_DAYS = 7;
export const RETENTION_MAX_DAYS = 3650;

/** The periods an operator who sets none gets (SPEC-023 §Data contracts §3). */
export const RETENTION_DEFAULTS: Readonly<Retention> = Object.freeze({
  processDays: 365,
  networkDays: 365,
  logonDays: 365,
  heartbeatDays: 30,
});

/**
 * The ClickHouse tables ingest writes and the statements that create them, in the
 * order they run: `heartbeats` (SPEC-004) and `cges_events` (SPEC-005, with the
 * ALTERs of SPEC-018, SPEC-019, SPEC-020 and ADR-0019 §9). Idempotent. Ingest's
 * bootstrap (`migrate.ts`) runs it, and so does the api test harness, which reads
 * `cges_events` (SPEC-010) and keeps no copy of this DDL (debt #6).
 */
export async function bootstrapClickHouseTables(
  ch: ClickHouseClient,
  retention: Retention,
): Promise<void> {
  checkRetention(retention);
  await ch.command({
    query: `
      CREATE TABLE IF NOT EXISTS heartbeats (
        agent_id        UUID,
        sequence_number UInt64,
        inner_sent_at   DateTime64(3, 'UTC'),
        outer_sent_at   DateTime64(3, 'UTC'),
        status          LowCardinality(String),
        uptime_seconds  UInt64,
        nonce           String,
        arrived_at      DateTime64(3, 'UTC') DEFAULT now64(3)
      ) ENGINE = MergeTree
      PARTITION BY toYYYYMMDD(arrived_at)
      ORDER BY (agent_id, arrived_at)
    `,
  });

  // SPEC-005 cges_events table. Engine + partition + order key per
  // ADR-0009 + ADR-0003: ReplacingMergeTree(arrived_at) for retry
  // dedup; partition by (org_id, daily) for per-org daily locality;
  // order by (org_id, time, event_id) with event_id last to enable
  // dedup at merge time without affecting query locality.
  //
  // Version column note: ClickHouse rejects UUID as a ReplacingMerge
  // Tree version column (must be integer / Date / DateTime / DateTime
  // 64). event_id (UUID) stays in the ORDER BY tuple as the dedup
  // key; arrived_at (DateTime64) is the version-column tiebreaker so
  // the latest-arrived row of a duplicate (org_id, time, event_id)
  // group wins at merge time — matches retry-side duplicate
  // collapse semantics per ADR-0009 Section References.
  //
  // Nullable columns express SPEC-005 conditional emission contracts:
  // - process_created_time: NULL on AC-004 cache-miss path (Terminate
  //   without prior Launch in cache).
  // - process_exit_code: NULL on AC-005 Launch path (absent in JSON,
  //   NULL in column).
  // - process_parent_pid: NULL when ParentProcessID unresolvable per
  //   AC-007 (rare; typically populated).
  await ch.command({
    query: `
      CREATE TABLE IF NOT EXISTS cges_events (
        agent_id              UUID,
        org_id                String DEFAULT 'default',
        event_id              UUID,
        class_uid             UInt32,
        activity_id           UInt8,
        process_pid           UInt32,
        process_uid           String,
        process_name          String,
        process_created_time  Nullable(DateTime64(9, 'UTC')),
        process_exit_code     Nullable(Int32),
        process_parent_pid    Nullable(UInt32),
        process_command_line  String DEFAULT '',
        subject_user_sid      String DEFAULT '',
        image_file_name       String DEFAULT '',
        time                  DateTime64(9, 'UTC'),
        arrived_at            DateTime64(3, 'UTC') DEFAULT now64(3)
      ) ENGINE = ReplacingMergeTree(arrived_at)
      PARTITION BY (org_id, toYYYYMMDD(time))
      ORDER BY (org_id, time, event_id)
    `,
  });

  // SPEC-018 §Operational §7 — the detection read-model reads forward by
  // arrival, and this minmax skip index on arrived_at lets that read prune.
  // An ALTER after the CREATE (which stays as it is) so a table that already
  // exists gets the index too. Parts written before the index are not
  // rewritten (no MATERIALIZE INDEX): they are read unpruned until a merge
  // rewrites them. Engine, partitioning and ordering are unchanged.
  await ch.command({
    query: `
      ALTER TABLE cges_events
      ADD INDEX IF NOT EXISTS ix_arrived_at arrived_at TYPE minmax GRANULARITY 1
    `,
  });

  // SPEC-019 §Data contracts, §Operational §7 — the six Network Activity (4001)
  // columns. An ALTER after the CREATE (which stays as it is), so a table that
  // already exists gets them too; rows written before read the defaults, which
  // are also what every Process Activity (1007) row carries.
  await ch.command({
    query: `
      ALTER TABLE cges_events
        ADD COLUMN IF NOT EXISTS src_ip        String DEFAULT '',
        ADD COLUMN IF NOT EXISTS src_port      UInt16 DEFAULT 0,
        ADD COLUMN IF NOT EXISTS dst_ip        String DEFAULT '',
        ADD COLUMN IF NOT EXISTS dst_port      UInt16 DEFAULT 0,
        ADD COLUMN IF NOT EXISTS net_protocol  String DEFAULT '',
        ADD COLUMN IF NOT EXISTS net_direction String DEFAULT ''
    `,
  });

  // SPEC-020 §Data contracts, §Operational §8 — the eleven Authentication (3002)
  // columns, by the same idempotent ALTER; rows of the other classes read the
  // defaults.
  await ch.command({
    query: `
      ALTER TABLE cges_events
        ADD COLUMN IF NOT EXISTS user_uid         String DEFAULT '',
        ADD COLUMN IF NOT EXISTS user_name        String DEFAULT '',
        ADD COLUMN IF NOT EXISTS user_domain      String DEFAULT '',
        ADD COLUMN IF NOT EXISTS logon_type_id    UInt8 DEFAULT 0,
        ADD COLUMN IF NOT EXISTS status_id        UInt8 DEFAULT 0,
        ADD COLUMN IF NOT EXISTS status_code      String DEFAULT '',
        ADD COLUMN IF NOT EXISTS status_detail    String DEFAULT '',
        ADD COLUMN IF NOT EXISTS auth_protocol    String DEFAULT '',
        ADD COLUMN IF NOT EXISTS auth_protocol_id UInt8 DEFAULT 0,
        ADD COLUMN IF NOT EXISTS src_hostname     String DEFAULT '',
        ADD COLUMN IF NOT EXISTS elevated_token   Nullable(Bool) DEFAULT NULL
    `,
  });

  await applyRetention(ch, retention);
}

/** The periods go into the DDL: whatever the caller, each is a whole number of days in
 *  bounds, checked before any statement runs. */
function checkRetention(retention: Retention): void {
  for (const name of ["processDays", "networkDays", "logonDays", "heartbeatDays"] as const) {
    const days = retention[name];
    if (!Number.isInteger(days) || days < RETENTION_MIN_DAYS || days > RETENTION_MAX_DAYS) {
      throw new Error(
        `retention: ${name} must be a whole number of days from ${RETENTION_MIN_DAYS} to ${RETENTION_MAX_DAYS}`,
      );
    }
  }
}

/** The time-to-live rules of each table, as `MODIFY TTL` takes them. */
function retentionTtl(r: Retention): { cges_events: string; heartbeats: string } {
  const at = "toDateTime(arrived_at)";
  const longest = Math.max(r.processDays, r.networkDays, r.logonDays);
  return {
    cges_events: [
      `${at} + toIntervalDay(${r.processDays}) DELETE WHERE class_uid = 1007`,
      `${at} + toIntervalDay(${r.networkDays}) DELETE WHERE class_uid = 4001`,
      `${at} + toIntervalDay(${r.logonDays}) DELETE WHERE class_uid = 3002`,
      `${at} + toIntervalDay(${longest}) DELETE WHERE class_uid NOT IN (1007, 4001, 3002)`,
    ].join(", "),
    heartbeats: `${at} + toIntervalDay(${r.heartbeatDays})`,
  };
}

/** The table's time-to-live as `SHOW CREATE TABLE` prints it, with DELETE, its default, left
 *  out; empty when it has none. */
async function currentTtl(ch: ClickHouseClient, table: string): Promise<string> {
  const rs = await ch.query({ query: `SHOW CREATE TABLE ${table}`, format: "TabSeparatedRaw" });
  const m = /\nTTL (.*)\n/.exec(await rs.text());
  return (m?.[1] ?? "").replace(/ DELETE/g, "");
}

/**
 * SPEC-023 §Operational §9 (ADR-0019 Amendment 2026-10-19) — the retention, per class of
 * cges_events (1007, 4001, 3002; a row of another class keeps the longest of the three) and
 * on heartbeats, measured from the row's arrival. A table whose time-to-live already says
 * these days is left as it is; one that says others gets the new rules, materialised
 * (`MATERIALIZE TTL`, ClickHouse's default after `MODIFY TTL`), so that rows already stored
 * are measured against the new period too. ClickHouse deletes at its merges, so a row may
 * outlive its period by the time a merge takes.
 */
async function applyRetention(ch: ClickHouseClient, retention: Retention): Promise<void> {
  const ttl = retentionTtl(retention);
  for (const table of ["cges_events", "heartbeats"] as const) {
    if ((await currentTtl(ch, table)) === ttl[table].replace(/ DELETE/g, "")) continue;
    await ch.command({
      query: `ALTER TABLE ${table} MODIFY TTL ${ttl[table]}`,
      clickhouse_settings: { materialize_ttl_after_modify: 1 },
    });
  }
}
