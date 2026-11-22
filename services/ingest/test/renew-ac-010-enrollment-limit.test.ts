import type { FastifyInstance } from "fastify";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import { buildEnrollApp } from "../src/app.js";
import type { Config } from "../src/config.js";
import { type Services, buildServices } from "../src/services.js";
import { issueTokenRow } from "./helpers/db.js";
import { withPool } from "./helpers/renew.js";
import { generateKeyPair, rawPubkeyB64url } from "./helpers/test-client.js";

// SPEC-025 renew_ac_010 (the enrollment limit) [server]. Ten failed enrollments from one
// address, then an eleventh with a valid token: `429`, and the token keeps its use; another
// address enrolls (§Operational §5). The source address is the one the listener sees, given
// here through Fastify's injection.

let config: Config;
let services: Services;
let app: FastifyInstance;

beforeAll(async () => {
  config = inject("ingestConfig");
  services = await buildServices(config);
  app = buildEnrollApp(services);
  await app.ready();
});

afterAll(async () => {
  await app?.close();
  await services?.close();
});

async function enrollFrom(address: string, token: string) {
  return app.inject({
    method: "POST",
    url: "/v1/agents/enroll",
    remoteAddress: address,
    payload: {
      envelope_version: "0.1.0",
      enrollment_token: token,
      agent_pubkey: await rawPubkeyB64url(await generateKeyPair()),
      agent_hostname: "RENEW-AC-010",
      agent_platform: "linux",
      agent_version: "0.1.0",
    },
  });
}

function unknownToken(): string {
  return Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString("base64url");
}

test("renew_ac_010: ten failures from one address refuse its next enrollment with 429; another address enrolls", async () => {
  const blocked = "198.18.10.1";
  for (let i = 0; i < 10; i++) {
    const res = await enrollFrom(blocked, unknownToken());
    expect(res.statusCode, res.body).toBe(401);
  }
  const { token, tokenId } = await issueTokenRow(config);
  const limited = await enrollFrom(blocked, token);
  expect(limited.statusCode, limited.body).toBe(429);
  expect(JSON.parse(limited.body)).toEqual({ error: "rate_limited" });

  const uses = await withPool(config, (pool) =>
    pool.query<{ uses: number; state: string }>(
      "SELECT uses, state::text AS state FROM enrollment_tokens WHERE token_id = $1",
      [tokenId],
    ),
  );
  expect(uses.rows[0]).toEqual({ uses: 0, state: "issued" });

  const other = await enrollFrom("198.18.10.2", token);
  expect(other.statusCode, other.body).toBe(200);
});

test("renew_ac_010: nine failures do not limit; malformed requests and successes count for nothing", async () => {
  const address = "198.18.10.3";
  for (let i = 0; i < 9; i++) {
    expect((await enrollFrom(address, unknownToken())).statusCode).toBe(401);
  }
  for (let i = 0; i < 3; i++) {
    const malformed = await app.inject({
      method: "POST",
      url: "/v1/agents/enroll",
      remoteAddress: address,
      payload: { envelope_version: "0.1.0" },
    });
    expect(malformed.statusCode).toBe(400);
  }
  const ok = await enrollFrom(address, (await issueTokenRow(config)).token);
  expect(ok.statusCode, ok.body).toBe(200);
  // The tenth failure limits the address.
  expect((await enrollFrom(address, unknownToken())).statusCode).toBe(401);
  expect((await enrollFrom(address, (await issueTokenRow(config)).token)).statusCode).toBe(429);
});
