import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { enrollTestAgent, issueTokenRow } from "./helpers/db.js";

// SPEC-025 renew_ac_005 (the write-once columns) [server]; SC-F4-004's revocation. Setting
// `revoked_at` with a reason succeeds once, and stores the transaction's time, not a time
// given a day ahead; a second change of either, clearing them, and `revoked_at` without a
// reason fail; `duplicate_signal` once set cannot change; a token's `withdrawn_at` once set
// cannot change or be cleared, and its `expires_at` and `max_uses` cannot change (§Data
// contracts §1, the triggers). The owner role writes them here, as nothing else may until
// SPEC-026's route.

let config: Config;
let pool: pg.Pool;

beforeAll(async () => {
  config = inject("ingestConfig");
  pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  pool.on("error", () => {});
});

afterAll(async () => {
  await pool?.end();
});

/** The statement's SQLSTATE and message when it fails; null when it succeeds. */
async function failure(text: string, params: unknown[]): Promise<string | null> {
  return pool.query(text, params).then(
    () => null,
    (e: unknown) => `${(e as { code?: string }).code}: ${(e as Error).message}`,
  );
}

test("renew_ac_005: a revocation is set once, at the transaction's time, and never changed", async () => {
  const agentId = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agentId);

  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const tx = await client.query<{ now: Date }>("SELECT now() AS now");
    await client.query(
      `UPDATE agents SET revoked_at = now() + interval '1 day', revocation_reason = 'stolen laptop'
        WHERE agent_id = $1`,
      [agentId],
    );
    const stored = await client.query<{ revoked_at: Date; revocation_reason: string }>(
      "SELECT revoked_at, revocation_reason FROM agents WHERE agent_id = $1",
      [agentId],
    );
    await client.query("COMMIT");
    expect(stored.rows[0]?.revoked_at.getTime()).toBe(tx.rows[0]?.now.getTime());
    expect(stored.rows[0]?.revocation_reason).toBe("stolen laptop");
  } finally {
    client.release();
  }

  for (const change of [
    "UPDATE agents SET revocation_reason = 'another reason' WHERE agent_id = $1",
    "UPDATE agents SET revoked_at = now() - interval '1 hour' WHERE agent_id = $1",
    "UPDATE agents SET revoked_at = NULL, revocation_reason = NULL WHERE agent_id = $1",
  ]) {
    const err = await failure(change, [agentId]);
    expect(err, change).toMatch(/^P0001: revocation is write-once/);
  }
});

test("renew_ac_005: revoked_at without a reason, or a reason without it, fails", async () => {
  const agentId = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agentId);
  expect(
    await failure("UPDATE agents SET revoked_at = now() WHERE agent_id = $1", [agentId]),
  ).toMatch(/^23514/);
  expect(
    await failure("UPDATE agents SET revocation_reason = 'r' WHERE agent_id = $1", [agentId]),
  ).toMatch(/^23514/);
  expect(
    await failure(
      "UPDATE agents SET revoked_at = now(), revocation_reason = '' WHERE agent_id = $1",
      [agentId],
    ),
  ).toMatch(/^23514/);
});

test("renew_ac_005: duplicate_signal once set cannot change", async () => {
  const agentId = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agentId);
  await pool.query(
    `UPDATE agents SET duplicate_signal = 'superseded_certificate', duplicate_signal_at = now()
      WHERE agent_id = $1`,
    [agentId],
  );
  for (const change of [
    "UPDATE agents SET duplicate_signal = 'interleaved_process' WHERE agent_id = $1",
    "UPDATE agents SET duplicate_signal = NULL, duplicate_signal_at = NULL WHERE agent_id = $1",
    "UPDATE agents SET duplicate_signal_at = now() - interval '1 day' WHERE agent_id = $1",
  ]) {
    expect(await failure(change, [agentId]), change).toMatch(/^P0001/);
  }
  // Only the two signals are admitted.
  const other = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, other);
  expect(
    await failure(
      "UPDATE agents SET duplicate_signal = 'something else', duplicate_signal_at = now() WHERE agent_id = $1",
      [other],
    ),
  ).toMatch(/^23514/);
});

test("renew_ac_005: a token's uses only grow, and its state does not go back to issued", async () => {
  const { tokenId } = await issueTokenRow(config, { maxUses: 3, expiresInMs: 60 * 60 * 1000 });
  await pool.query("UPDATE enrollment_tokens SET uses = 2 WHERE token_id = $1", [tokenId]);
  expect(
    await failure("UPDATE enrollment_tokens SET uses = 1 WHERE token_id = $1", [tokenId]),
  ).toMatch(/^P0001: token uses only grow/);
  await pool.query(
    "UPDATE enrollment_tokens SET uses = 3, state = 'consumed' WHERE token_id = $1",
    [tokenId],
  );
  expect(
    await failure("UPDATE enrollment_tokens SET state = 'issued' WHERE token_id = $1", [tokenId]),
  ).toMatch(/^P0001: token uses only grow/);
});

test("renew_ac_005: a token's withdrawal is set once; its expiry and cap are fixed", async () => {
  const { tokenId } = await issueTokenRow(config, { maxUses: 3, expiresInMs: 60 * 60 * 1000 });
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const tx = await client.query<{ now: Date }>("SELECT now() AS now");
    await client.query(
      "UPDATE enrollment_tokens SET withdrawn_at = now() + interval '1 day' WHERE token_id = $1",
      [tokenId],
    );
    const stored = await client.query<{ withdrawn_at: Date }>(
      "SELECT withdrawn_at FROM enrollment_tokens WHERE token_id = $1",
      [tokenId],
    );
    await client.query("COMMIT");
    expect(stored.rows[0]?.withdrawn_at.getTime()).toBe(tx.rows[0]?.now.getTime());
  } finally {
    client.release();
  }
  for (const [change, message] of [
    [
      "UPDATE enrollment_tokens SET withdrawn_at = now() - interval '1 hour' WHERE token_id = $1",
      "withdrawal is write-once",
    ],
    [
      "UPDATE enrollment_tokens SET withdrawn_at = NULL WHERE token_id = $1",
      "withdrawal is write-once",
    ],
    [
      "UPDATE enrollment_tokens SET expires_at = expires_at + interval '1 minute' WHERE token_id = $1",
      "token terms are fixed",
    ],
    ["UPDATE enrollment_tokens SET max_uses = 4 WHERE token_id = $1", "token terms are fixed"],
  ] as const) {
    expect(await failure(change, [tokenId]), change).toMatch(new RegExp(`^P0001: ${message}`));
  }
});
