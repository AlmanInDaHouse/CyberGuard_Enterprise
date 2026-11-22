import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { issueTokenRow, tokenSha256 } from "./helpers/db.js";
import { type RenewHarness, agentRow, startRenewHarness, withPool } from "./helpers/renew.js";
import { enroll } from "./helpers/test-client.js";

// SPEC-025 renew_ac_003 (several uses) [server]; SC-F4-001, a token for a rollout. A token
// of `max_uses` 3 enrolls three agents, each naming it; the fourth answers `409`; after
// `withdrawn_at` is set on another such token, its next enrollment answers `401`. The
// migration's checks refuse a single-use token of more than 15 minutes and any token of more
// than 7 days (§Data contracts §1).

const HOUR_MS = 60 * 60 * 1000;

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

test("renew_ac_003: a token of three uses enrolls three agents, each naming it; the fourth is 409", async () => {
  const { token, tokenId } = await issueTokenRow(config, { maxUses: 3, expiresInMs: HOUR_MS });
  for (let i = 0; i < 3; i++) {
    const identity = await enroll(harness.enrollUrl, token, { hostname: `ROLLOUT-${i}` });
    const row = await agentRow(config, identity.agentId);
    expect(row?.enrolled_by_token_id).toBe(tokenId);
  }
  await expect(enroll(harness.enrollUrl, token)).rejects.toMatchObject({ status: 409 });

  const stored = await withPool(config, (pool) =>
    pool.query<{ uses: number; state: string }>(
      "SELECT uses, state::text AS state FROM enrollment_tokens WHERE token_id = $1",
      [tokenId],
    ),
  );
  expect(stored.rows[0]).toEqual({ uses: 3, state: "consumed" });
});

test("renew_ac_003: a withdrawn token of several uses answers 401", async () => {
  const { token, tokenId } = await issueTokenRow(config, { maxUses: 3, expiresInMs: HOUR_MS });
  await enroll(harness.enrollUrl, token);
  await withPool(config, (pool) =>
    pool.query("UPDATE enrollment_tokens SET withdrawn_at = now() WHERE token_id = $1", [tokenId]),
  );
  await expect(enroll(harness.enrollUrl, token)).rejects.toMatchObject({ status: 401 });
});

/** The SQLSTATE an insert of a token of `maxUses` lasting `interval` fails with, or null. */
async function insertFails(maxUses: number, interval: string): Promise<string | null> {
  return withPool(config, (pool) =>
    pool
      .query(
        `INSERT INTO enrollment_tokens (token_id, token_sha256, org_id, scope, max_uses, expires_at)
         VALUES (gen_random_uuid(), $1, 'default', 'enroll', $2, now() + $3::interval)`,
        [tokenSha256(globalThis.crypto.randomUUID()), maxUses, interval],
      )
      .then(
        () => null,
        (e: unknown) => (e as { code?: string }).code ?? "unknown",
      ),
  );
}

test("renew_ac_003: the checks refuse a single-use token of more than 15 minutes, any of more than 7 days", async () => {
  expect(await insertFails(1, "15 minutes 1 second")).toBe("23514");
  expect(await insertFails(1, "14 minutes")).toBeNull();
  expect(await insertFails(3, "7 days 1 second")).toBe("23514");
  expect(await insertFails(1000, "7 days")).toBeNull();
  expect(await insertFails(1001, "1 hour")).toBe("23514");
});
