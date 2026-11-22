import { afterAll, beforeAll, beforeEach, expect, inject, test, vi } from "vitest";
import * as cert from "../src/cert.js";
import type { Config } from "../src/config.js";
import { issueToken, issueTokenRow, tokenSha256 } from "./helpers/db.js";
import { type RenewHarness, startRenewHarness, withPool } from "./helpers/renew.js";
import { EnrollError, enroll } from "./helpers/test-client.js";

// SPEC-025 renew_ac_002 (checked before signing) [server]. An unknown token, an expired one,
// a withdrawn one, one of scope `other` and one of org `other` answer `401 token_rejected`;
// one with no use left `409 token_already_used`, expired or not; in each the CA signs
// nothing (a spy on the signing function counts 0) and no agent row is written. Two
// concurrent enrollments with one single-use token: one `200`, one `409` (§Operational §1).

vi.mock("../src/cert.js", async (importOriginal) => {
  const original = await importOriginal<typeof import("../src/cert.js")>();
  return { ...original, issueClientCert: vi.fn(original.issueClientCert) };
});
const signer = vi.mocked(cert.issueClientCert);

let config: Config;
let harness: RenewHarness;

beforeAll(async () => {
  config = inject("ingestConfig");
  harness = await startRenewHarness(config);
});

afterAll(async () => {
  await harness?.close();
});

beforeEach(() => {
  signer.mockClear();
});

async function agentCount(): Promise<number> {
  return withPool(config, async (pool) =>
    Number((await pool.query<{ n: string }>("SELECT count(*) AS n FROM agents")).rows[0]?.n),
  );
}

/** Enroll with `token`; the refusal's status and `error`. */
async function refusal(token: string): Promise<{ status: number; error: string | undefined }> {
  const err = await enroll(harness.enrollUrl, token).then(
    () => null,
    (e: unknown) => e,
  );
  expect(err, "the enrollment was accepted").toBeInstanceOf(EnrollError);
  const e = err as EnrollError;
  return { status: e.status, error: (JSON.parse(e.bodyText) as { error?: string }).error };
}

async function expectRefusedUnsigned(token: string, status: number, error: string): Promise<void> {
  const before = await agentCount();
  expect(await refusal(token)).toEqual({ status, error });
  expect(signer, "the CA signed for a refused token").toHaveBeenCalledTimes(0);
  expect(await agentCount()).toBe(before);
}

/** Spend every use of a token directly, as enrollments would have. */
async function useUp(token: string): Promise<void> {
  await withPool(config, (pool) =>
    pool.query(
      `UPDATE enrollment_tokens SET uses = max_uses, state = 'consumed', consumed_at = now()
        WHERE token_sha256 = $1`,
      [tokenSha256(token)],
    ),
  );
}

test("renew_ac_002: an unknown token answers 401 token_rejected and signs nothing", async () => {
  const unknown = Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString(
    "base64url",
  );
  await expectRefusedUnsigned(unknown, 401, "token_rejected");
});

test("renew_ac_002: an expired token answers 401 token_rejected and signs nothing", async () => {
  await expectRefusedUnsigned(
    await issueToken(config, { expiresInMs: -60_000 }),
    401,
    "token_rejected",
  );
});

test("renew_ac_002: a withdrawn token answers 401 token_rejected and signs nothing", async () => {
  const { token, tokenId } = await issueTokenRow(config);
  await withPool(config, (pool) =>
    pool.query("UPDATE enrollment_tokens SET withdrawn_at = now() WHERE token_id = $1", [tokenId]),
  );
  await expectRefusedUnsigned(token, 401, "token_rejected");
});

test("renew_ac_002: a token of scope other answers 401 token_rejected and signs nothing", async () => {
  await expectRefusedUnsigned(await issueToken(config, { scope: "other" }), 401, "token_rejected");
});

test("renew_ac_002: a token of org other answers 401 token_rejected and signs nothing", async () => {
  await expectRefusedUnsigned(await issueToken(config, { orgId: "other" }), 401, "token_rejected");
});

test("renew_ac_002: a token with no use left answers 409 token_already_used, expired or not", async () => {
  const live = await issueToken(config);
  await useUp(live);
  await expectRefusedUnsigned(live, 409, "token_already_used");

  const expired = await issueToken(config, { expiresInMs: -60_000 });
  await useUp(expired);
  await expectRefusedUnsigned(expired, 409, "token_already_used");
});

test("renew_ac_002: two concurrent enrollments with one single-use token: one 200, one 409", async () => {
  const token = await issueToken(config);
  const results = await Promise.allSettled([
    enroll(harness.enrollUrl, token),
    enroll(harness.enrollUrl, token),
  ]);
  const ok = results.filter((r) => r.status === "fulfilled");
  const refused = results.filter((r) => r.status === "rejected");
  expect(ok).toHaveLength(1);
  expect(refused).toHaveLength(1);
  expect((refused[0] as PromiseRejectedResult).reason).toMatchObject({ status: 409 });
  // One certificate was signed: the accepted one's.
  expect(signer).toHaveBeenCalledTimes(1);
});
