import { createClient } from "@clickhouse/client";
import pg from "pg";
import type { Config } from "../../src/config.js";

/**
 * Insert an enrollment_tokens row directly (the server's CLI does this in
 * production; tests insert directly to control expiry). Returns the opaque
 * token. `expiresInMs` negative ⇒ an already-expired token (AC-007).
 */
export async function issueToken(
  config: Config,
  opts: { expiresInMs?: number } = {},
): Promise<string> {
  const token = Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString(
    "base64url",
  );
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const expiresAt = new Date(Date.now() + (opts.expiresInMs ?? 15 * 60 * 1000));
    await pool.query(
      "INSERT INTO enrollment_tokens (token, org_id, scope, state, expires_at) VALUES ($1, 'default', 'enroll', 'issued', $2)",
      [token, expiresAt],
    );
  } finally {
    await pool.end();
  }
  return token;
}

export interface AgentRow {
  agent_id: string;
  pubkey: Buffer;
  enrolled_at: Date;
  last_seen: Date | null;
}

export async function getAgent(config: Config, agentId: string): Promise<AgentRow | null> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const r = await pool.query<AgentRow>(
      "SELECT agent_id, pubkey, enrolled_at, last_seen FROM agents WHERE agent_id = $1",
      [agentId],
    );
    return r.rows[0] ?? null;
  } finally {
    await pool.end();
  }
}

export interface HeartbeatRow {
  sequence_number: string;
  status: string;
  arrived_at: string;
}

export async function getHeartbeats(config: Config, agentId: string): Promise<HeartbeatRow[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query:
        "SELECT toString(sequence_number) AS sequence_number, status, toString(arrived_at) AS arrived_at FROM heartbeats WHERE agent_id = toUUID({id:String}) ORDER BY arrived_at",
      query_params: { id: agentId },
      format: "JSONEachRow",
    });
    return (await rs.json()) as HeartbeatRow[];
  } finally {
    await ch.close();
  }
}
