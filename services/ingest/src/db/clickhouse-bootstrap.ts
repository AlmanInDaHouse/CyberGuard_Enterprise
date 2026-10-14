import type { ClickHouseClient } from "@clickhouse/client";

/**
 * The ClickHouse tables ingest writes and the statements that create them, in the
 * order they run: `heartbeats` (SPEC-004) and `cges_events` (SPEC-005, with the
 * ALTERs of SPEC-018, SPEC-019, SPEC-020 and ADR-0019 §9). Idempotent. Ingest's
 * bootstrap (`migrate.ts`) runs it, and so does the api test harness, which reads
 * `cges_events` (SPEC-010) and keeps no copy of this DDL (debt #6).
 */
export async function bootstrapClickHouseTables(ch: ClickHouseClient): Promise<void> {
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

  // ADR-0019 §9, SPEC-020 §Data contracts "Retention" — a logon row is deleted
  // once its arrival is 365 days old; no other class has a time-to-live. Set on
  // every bootstrap without materializing it, so a rerun rewrites no part; rows
  // of class 3002 did not exist before SPEC-020.
  await ch.command({
    query: `
      ALTER TABLE cges_events
      MODIFY TTL toDateTime(arrived_at) + toIntervalDay(365) DELETE WHERE class_uid = 3002
    `,
    clickhouse_settings: { materialize_ttl_after_modify: 0 },
  });
}
