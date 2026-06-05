import { type ClickHouseClient, createClient } from "@clickhouse/client";
import type { Config } from "../../src/config.js";

/**
 * SPEC-010 — materialise + seed the ClickHouse `cges_events` table in the api test
 * ClickHouse (the forensic event drill's read target).
 *
 * The api test harness applies ingest's REAL Postgres migrations in-workspace
 * (read-schema.ts), but `cges_events`'s DDL lives in ingest's PRIVATE
 * `bootstrapClickHouse` (services/ingest/src/db/migrate.ts:86-155), NOT a migration
 * file — so it cannot be reused the way the Postgres migrations are. This is
 * therefore a scoped, test-only DDL mirror of `cges_events` (the DDL is copied
 * VERBATIM from migrate.ts:134-153). Named architecture debt (SPEC-010 §Open
 * questions 1): replace with a shared ClickHouse-schema module (or an exported
 * `bootstrapClickHouse`) when a second api↔ingest ClickHouse table is shared, or on
 * the first un-caught DDL drift. A drift on a projected column is caught by
 * drill_ac_001 at the GREEN gate.
 */
function chClient(config: Config): ClickHouseClient {
  return createClient({
    url: config.API_CH_URL,
    username: config.API_CH_USER,
    password: config.API_CH_PASSWORD,
    database: config.API_CH_DB,
  });
}

/** Create the `cges_events` table (verbatim DDL, ingest migrate.ts:134-153). */
export async function bootstrapEventsTable(config: Config): Promise<void> {
  const ch = chClient(config);
  try {
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
  } finally {
    await ch.close();
  }
}

export interface SeedCgesEvent {
  agentId: string;
  eventId: string;
  /** ClickHouse DateTime64(9) literal, e.g. "2026-06-05 10:00:00.000000000". */
  time: string;
  activityId?: number;
  processPid?: number;
  processUid?: string;
  processName?: string;
  imageFileName?: string;
  processParentPid?: number | null;
  classUid?: number;
  orgId?: string;
}

/** Insert one `cges_events` row (mirrors services/ingest/test/helpers/db.ts:153-183). */
export async function insertCgesEvent(config: Config, ev: SeedCgesEvent): Promise<void> {
  const ch = chClient(config);
  try {
    await ch.insert({
      table: "cges_events",
      values: [
        {
          agent_id: ev.agentId,
          org_id: ev.orgId ?? "default",
          event_id: ev.eventId,
          class_uid: ev.classUid ?? 1007,
          activity_id: ev.activityId ?? 1,
          process_pid: ev.processPid ?? 4321,
          process_uid: ev.processUid ?? "",
          process_name: ev.processName ?? "powershell.exe",
          process_parent_pid: ev.processParentPid ?? null,
          image_file_name: ev.imageFileName ?? "C:\\Windows\\System32\\powershell.exe",
          time: ev.time,
        },
      ],
      format: "JSONEachRow",
    });
  } finally {
    await ch.close();
  }
}
