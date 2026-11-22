import "reflect-metadata";
import { X509Certificate, type webcrypto } from "node:crypto";
import * as x509 from "@peculiar/x509";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runPgMigrations } from "../src/db/migrate.js";
import { issueToken, tokenSha256 } from "./helpers/db.js";
import {
  DAY_MS,
  type RenewAnswer,
  type RenewHarness,
  ageAgentCert,
  buildRenewRequest,
  errorOf,
  postRenew,
  rawPubkey,
  signAgentCert,
  startRenewHarness,
  withPair,
  withPool,
} from "./helpers/renew.js";
import { enroll, generateKeyPair } from "./helpers/test-client.js";
import { createPgDatabase, dropPgDatabase, pgDatabaseUrl } from "./helpers/throwaway.js";

// SPEC-025 renew_ac_009 (the CA's end) [server]. With a CA whose `notAfter` is 60 days away,
// an enrollment's certificate ends at the CA's `notAfter`; a renewal presenting a certificate
// with 40 days left, which ends before the CA does, answers `200` with a certificate that ends
// at the CA's `notAfter`; a renewal under a certificate that already ends there answers
// `409 renewal_not_due` (§Operational §1 and §4). The last case is also run under a CA 30
// days from its end, where the 45-day window alone would admit it, so that the CA's end alone
// refuses it. An insert into `enrollment_tokens` with an `issued_at` a year ahead stores the
// database's time, and its `expires_at` past 7 days from it is refused (§Data contracts §1).

x509.cryptoProvider.set(globalThis.crypto);
const ED = { name: "Ed25519" } as const;

let suite: Config;

beforeAll(() => {
  suite = inject("ingestConfig");
});

/** A CA that ends `daysLeft` days from now, stored as ingest stores its own (ca.ts). */
async function storeShortCa(config: Config, daysLeft: number): Promise<Date> {
  const keys = (await globalThis.crypto.subtle.generateKey(ED, true, [
    "sign",
    "verify",
  ])) as webcrypto.CryptoKeyPair;
  // Whole seconds, as X.509 stores them.
  const notAfter = new Date(Math.floor((Date.now() + daysLeft * DAY_MS) / 1000) * 1000);
  const caCert = await x509.X509CertificateGenerator.createSelfSigned(
    {
      serialNumber: "01",
      name: "CN=CyberGuard Ingest CA",
      notBefore: new Date(Date.now() - 300 * DAY_MS),
      notAfter,
      signingAlgorithm: ED,
      keys,
      extensions: [
        new x509.BasicConstraintsExtension(true, undefined, true),
        new x509.KeyUsagesExtension(
          x509.KeyUsageFlags.keyCertSign | x509.KeyUsageFlags.cRLSign,
          true,
        ),
      ],
    },
    globalThis.crypto,
  );
  const der = await globalThis.crypto.subtle.exportKey("pkcs8", keys.privateKey);
  const b64 =
    Buffer.from(new Uint8Array(der))
      .toString("base64")
      .match(/.{1,64}/g) ?? [];
  const keyPem = `-----BEGIN PRIVATE KEY-----\n${b64.join("\n")}\n-----END PRIVATE KEY-----\n`;
  await withPool(config, (pool) =>
    pool.query(
      "INSERT INTO ca (id, cert_pem, private_key) VALUES (1, $1, pgp_sym_encrypt($2, $3))",
      [caCert.toString("pem"), keyPem, config.INGEST_CA_PASSPHRASE],
    ),
  );
  return notAfter;
}

/** A throwaway database at the latest schema, holding a CA that ends in `daysLeft` days. */
function shortCaSuite(database: string, daysLeft: number) {
  const ctx = {} as { config: Config; harness: RenewHarness; caEnd: Date };
  beforeAll(async () => {
    await createPgDatabase(suite.INGEST_PG_URL, database);
    ctx.config = { ...suite, INGEST_PG_URL: pgDatabaseUrl(suite.INGEST_PG_URL, database) };
    await runPgMigrations(ctx.config);
    ctx.caEnd = await storeShortCa(ctx.config, daysLeft);
    ctx.harness = await startRenewHarness(ctx.config);
  });
  afterAll(async () => {
    await ctx.harness?.close();
    await dropPgDatabase(suite.INGEST_PG_URL, database);
  });
  return ctx;
}

function notAfterOf(certPem: string): number {
  return new Date(new X509Certificate(certPem).validTo).getTime();
}

describe("a CA 60 days from its end", () => {
  const ctx = shortCaSuite("cyberguard_renew_ac_009_a", 60);

  test("renew_ac_009: an enrollment's certificate ends at the CA's notAfter", async () => {
    const identity = await enroll(ctx.harness.enrollUrl, await issueToken(ctx.config));
    expect(notAfterOf(identity.certPem)).toBe(ctx.caEnd.getTime());
  });

  test("renew_ac_009: a renewal under a certificate with 40 days left gets one ending at the CA's notAfter", async () => {
    const enrolled = await enroll(ctx.harness.enrollUrl, await issueToken(ctx.config));
    const identity = await ageAgentCert(ctx.config, ctx.harness.ca, enrolled, 40);
    const newKeys = await generateKeyPair();
    const res = await postRenew(ctx.harness.heartbeatUrl, {
      caCertPem: ctx.harness.caCertPem,
      identity,
      envelope: await buildRenewRequest(identity, newKeys),
    });
    expect(res.status, res.bodyText).toBe(200);
    const answer = JSON.parse(res.bodyText) as RenewAnswer;
    expect(notAfterOf(answer.client_certificate)).toBe(ctx.caEnd.getTime());
  });

  test("renew_ac_009: a renewal under a certificate that already ends there is 409 renewal_not_due", async () => {
    const identity = await enroll(ctx.harness.enrollUrl, await issueToken(ctx.config));
    expect(notAfterOf(identity.certPem)).toBe(ctx.caEnd.getTime());
    const res = await postRenew(ctx.harness.heartbeatUrl, {
      caCertPem: ctx.harness.caCertPem,
      identity,
      envelope: await buildRenewRequest(identity, await generateKeyPair()),
    });
    expect(res.status, res.bodyText).toBe(409);
    expect(errorOf(res)).toBe("renewal_not_due");
  });
});

describe("a CA 30 days from its end", () => {
  const ctx = shortCaSuite("cyberguard_renew_ac_009_b", 30);

  test("renew_ac_009: a due certificate that already ends at the CA's notAfter is 409 renewal_not_due", async () => {
    const enrolled = await enroll(ctx.harness.enrollUrl, await issueToken(ctx.config));
    // Due by the 45-day window: only the CA's end can refuse it.
    expect(notAfterOf(enrolled.certPem) - Date.now()).toBeLessThan(45 * DAY_MS);
    const res = await postRenew(ctx.harness.heartbeatUrl, {
      caCertPem: ctx.harness.caCertPem,
      identity: enrolled,
      envelope: await buildRenewRequest(enrolled, await generateKeyPair()),
    });
    expect(res.status, res.bodyText).toBe(409);
    expect(errorOf(res)).toBe("renewal_not_due");
  });

  test("renew_ac_009: a due certificate ending before the CA renews to the CA's end", async () => {
    const enrolled = await enroll(ctx.harness.enrollUrl, await issueToken(ctx.config));
    const shorter = await withPair(
      enrolled,
      await signAgentCert(ctx.harness.ca, enrolled.agentId, enrolled.keyPair, {
        notBefore: new Date(Date.now() - 60_000),
        notAfter: new Date(Date.now() + 10 * DAY_MS),
      }),
      enrolled.keyPair,
    );
    await withPool(ctx.config, async (pool) =>
      pool.query("UPDATE agents SET pubkey = $2, cert_pem = $3 WHERE agent_id = $1", [
        enrolled.agentId,
        await rawPubkey(enrolled.keyPair),
        shorter.certPem,
      ]),
    );
    const res = await postRenew(ctx.harness.heartbeatUrl, {
      caCertPem: ctx.harness.caCertPem,
      identity: shorter,
      envelope: await buildRenewRequest(shorter, await generateKeyPair()),
    });
    expect(res.status, res.bodyText).toBe(200);
    expect(notAfterOf((JSON.parse(res.bodyText) as RenewAnswer).client_certificate)).toBe(
      ctx.caEnd.getTime(),
    );
  });
});

describe("a CA past its end", () => {
  const ctx = shortCaSuite("cyberguard_renew_ac_009_c", -1);

  test("an enrollment signs nothing and writes no agent", async () => {
    const before = await withPool(ctx.config, async (pool) =>
      Number((await pool.query<{ n: string }>("SELECT count(*) AS n FROM agents")).rows[0]?.n),
    );
    await expect(enroll(ctx.harness.enrollUrl, await issueToken(ctx.config))).rejects.toMatchObject(
      { status: 500 },
    );
    const after = await withPool(ctx.config, async (pool) =>
      Number((await pool.query<{ n: string }>("SELECT count(*) AS n FROM agents")).rows[0]?.n),
    );
    expect(after).toBe(before);
  });
});

test("renew_ac_009: a token inserted with an issued_at a year ahead stores the database's time", async () => {
  const pool = new pg.Pool({ connectionString: suite.INGEST_PG_URL });
  pool.on("error", () => {});
  try {
    const stored = await pool.query<{ ahead: boolean; uses: number; state: string }>(
      `INSERT INTO enrollment_tokens
         (token_id, token_sha256, org_id, scope, max_uses, issued_at, expires_at, uses, state)
       VALUES (gen_random_uuid(), $1, 'default', 'enroll', 3, now() + interval '1 year',
               now() + interval '1 day', 2, 'consumed')
       RETURNING issued_at > now() AS ahead, uses, state::text AS state`,
      [tokenSha256(globalThis.crypto.randomUUID())],
    );
    expect(stored.rows[0]).toEqual({ ahead: false, uses: 0, state: "issued" });

    const refused = await pool
      .query(
        `INSERT INTO enrollment_tokens
           (token_id, token_sha256, org_id, scope, max_uses, issued_at, expires_at)
         VALUES (gen_random_uuid(), $1, 'default', 'enroll', 3, now() + interval '1 year',
                 now() + interval '1 year')`,
        [tokenSha256(globalThis.crypto.randomUUID())],
      )
      .then(
        () => null,
        (e: unknown) => (e as { code?: string }).code,
      );
    expect(refused).toBe("23514");
  } finally {
    await pool.end();
  }
});
