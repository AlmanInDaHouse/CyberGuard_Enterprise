import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { type ClickHouseClient, createClient } from "@clickhouse/client";
import type { Config } from "../../src/config.js";

/**
 * SPEC-010 — materialise + seed the ClickHouse `cges_events` table in the api test
 * ClickHouse (the forensic event drill's read target).
 *
 * The table is created by ingest's own ClickHouse bootstrap
 * (`services/ingest/src/db/clickhouse-bootstrap.ts`), the statements ingest runs at
 * its start, so the api tests read the table ingest writes and keep no copy of its
 * DDL (debt #6; SPEC-010 §Open questions 1). It is loaded by URL at run time, as
 * read-schema.ts loads ingest's Postgres migrations: the file lies outside this
 * package's `rootDir`.
 */
const INGEST_CLICKHOUSE_BOOTSTRAP = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../ingest/src/db/clickhouse-bootstrap.ts",
);

function chClient(config: Config): ClickHouseClient {
  return createClient({
    url: config.API_CH_URL,
    username: config.API_CH_USER,
    password: config.API_CH_PASSWORD,
    database: config.API_CH_DB,
  });
}

/** Create ingest's ClickHouse tables (`heartbeats` and `cges_events`) in the api test ClickHouse. */
export async function bootstrapIngestClickHouse(config: Config): Promise<void> {
  const { bootstrapClickHouseTables, RETENTION_DEFAULTS } = (await import(
    pathToFileURL(INGEST_CLICKHOUSE_BOOTSTRAP).href
  )) as {
    bootstrapClickHouseTables(ch: ClickHouseClient, retention: unknown): Promise<void>;
    RETENTION_DEFAULTS: unknown;
  };
  const ch = chClient(config);
  try {
    // The retention's defaults (SPEC-023 §Data contracts §3), ingest's own, as its
    // migration step sets them unless the operator says otherwise.
    await bootstrapClickHouseTables(ch, RETENTION_DEFAULTS);
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

/** Insert one `cges_events` row (as `insertCgesEvent` in services/ingest/test/helpers/db.ts does). */
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
