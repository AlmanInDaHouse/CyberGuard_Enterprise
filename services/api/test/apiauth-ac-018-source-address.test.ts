import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { parseTrustedProxies, sourceAddress } from "../src/auth/source-address.js";
import { type Config, loadConfig } from "../src/config.js";
import { buildServices } from "../src/services.js";

// apiauth_ac_018 — SPEC-008's second Amendment 2026-10-15, the source address behind a
// proxy (debt #79). From a peer `API_TRUSTED_PROXIES` lists, the api walks
// `X-Forwarded-For` from the right past the listed entries; the per-IP counter the login
// and the TOTP confirmation share, and the audit rows' `source_ip`, read that address.

const PEER = "192.0.2.10"; // the test's peer: `app.inject`'s remote address
const LISTED = `${PEER}, 192.0.2.32/28`; // the peer and a range, 192.0.2.32 to .47
const SECOND_HOP = "192.0.2.40"; // in the listed range

let config: Config;
let redis: Redis;
const apps: { app: FastifyInstance; close: () => Promise<void> }[] = [];
let listed: FastifyInstance;
let notListingThePeer: FastifyInstance;
let unset: FastifyInstance;

/** Every address the file counts under, cleared first: other files share the store. */
const ADDRESSES = [
  PEER,
  "203.0.113.7",
  "2001:db8::7",
  "203.0.113.9",
  "198.51.100.4",
  "203.0.113.20",
  "203.0.113.21",
  "203.0.113.30",
  "203.0.113.31",
  "203.0.113.60",
  "203.0.113.40",
  "203.0.113.41",
  "203.0.113.42",
];

async function appWith(list: string | undefined): Promise<FastifyInstance> {
  const services = await buildServices({ ...config, API_TRUSTED_PROXIES: list });
  const app = buildApp(services);
  await app.ready();
  apps.push({
    app,
    async close() {
      await app.close();
      await services.close();
    },
  });
  return app;
}

/** The per-IP counter for an address (the key `services/api/src/auth/ratelimit.ts` uses). */
async function counted(address: string): Promise<number> {
  return Number((await redis.get(`rl:ip:${address}`)) ?? 0);
}

/** The `auth.login.fail` rows recorded under an address. */
async function failRows(address: string): Promise<number> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query(
      "SELECT 1 FROM audit_log WHERE action = 'auth.login.fail' AND host(source_ip) = $1",
      [address],
    );
    return r.rowCount ?? 0;
  } finally {
    await pool.end();
  }
}

/** The denied `user.totp.enroll` rows recorded under an address. */
async function deniedConfirmations(address: string): Promise<number> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query(
      "SELECT 1 FROM audit_log WHERE action = 'user.totp.enroll' AND outcome = 'denied' AND host(source_ip) = $1",
      [address],
    );
    return r.rowCount ?? 0;
  } finally {
    await pool.end();
  }
}

let attempt = 0;
/** A failed login: an address no user has, so only the per-IP counter can refuse it. */
function failedLogin(app: FastifyInstance, forwardedFor?: string) {
  attempt += 1;
  return app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: PEER,
    headers: forwardedFor === undefined ? {} : { "x-forwarded-for": forwardedFor },
    payload: { email: `nobody-${attempt}@apiauth018.test`, password: "x", totp_code: "000000" },
  });
}

beforeAll(async () => {
  config = inject("apiConfig");
  redis = new Redis(config.API_REDIS_URL);
  await redis.del(...ADDRESSES.map((a) => `rl:ip:${a}`));
  listed = await appWith(LISTED);
  notListingThePeer = await appWith("192.0.2.99");
  unset = await appWith(undefined);
});
afterAll(async () => {
  for (const a of apps) await a.close();
  await redis?.quit();
});

describe("from a listed peer", () => {
  test("a forwarded address is the one counted and recorded", async () => {
    const before = await failRows("203.0.113.7");
    const res = await failedLogin(listed, "203.0.113.7");
    expect(res.statusCode).toBe(401);
    expect(await counted("203.0.113.7")).toBe(1);
    expect(await failRows("203.0.113.7")).toBe(before + 1);
    expect(await counted(PEER)).toBe(0);
  });

  test("an IPv6 entry is read in its canonical form, a mapped one as its IPv4 address", async () => {
    const v6Before = await failRows("2001:db8::7");
    const v4Before = await failRows("203.0.113.9");
    expect((await failedLogin(listed, "2001:DB8:0:0:0:0:0:7")).statusCode).toBe(401);
    expect(await counted("2001:db8::7")).toBe(1);
    expect(await failRows("2001:db8::7")).toBe(v6Before + 1);
    expect((await failedLogin(listed, "::ffff:203.0.113.9")).statusCode).toBe(401);
    expect((await failedLogin(listed, "::ffff:cb00:7109")).statusCode).toBe(401);
    expect(await counted("203.0.113.9")).toBe(2);
    expect(await failRows("203.0.113.9")).toBe(v4Before + 2);
  });

  test("failures under two forwarded addresses are counted apart", async () => {
    for (let i = 0; i < 10; i++) {
      expect((await failedLogin(listed, "203.0.113.20")).statusCode, `attempt ${i}`).toBe(401);
    }
    expect((await failedLogin(listed, "203.0.113.20")).statusCode).toBe(429);
    expect((await failedLogin(listed, "203.0.113.21")).statusCode).toBe(401);
    expect(await counted("203.0.113.21")).toBe(1);
  });

  test("the walk passes listed entries and takes the first that is not", async () => {
    expect((await failedLogin(listed, `198.51.100.4, ${SECOND_HOP}`)).statusCode).toBe(401);
    expect(await counted("198.51.100.4")).toBe(1);
    expect(await counted(SECOND_HOP)).toBe(0);
  });

  test("the walk starts from the right: an entry left of the first unlisted one is not taken", async () => {
    await redis.del("rl:ip:203.0.113.40", "rl:ip:203.0.113.41", "rl:ip:203.0.113.42");
    expect(
      (await failedLogin(listed, `203.0.113.42, 203.0.113.40, ${SECOND_HOP}`)).statusCode,
    ).toBe(401);
    expect(await counted("203.0.113.40")).toBe(1);
    expect(await counted("203.0.113.42")).toBe(0);
    // An entry that is not an IP address left of the one taken does not matter.
    expect((await failedLogin(listed, "unknown, 203.0.113.41")).statusCode).toBe(401);
    expect(await counted("203.0.113.41")).toBe(1);
  });

  test("with every entry listed, the leftmost is taken", async () => {
    await redis.del(`rl:ip:${SECOND_HOP}`, "rl:ip:192.0.2.41");
    expect((await failedLogin(listed, `${SECOND_HOP}, 192.0.2.41`)).statusCode).toBe(401);
    expect(await counted(SECOND_HOP)).toBe(1);
    expect(await counted("192.0.2.41")).toBe(0);
  });

  test("no header, or an entry that is not an IP address, leaves the peer's address", async () => {
    await redis.del(`rl:ip:${PEER}`);
    expect((await failedLogin(listed)).statusCode).toBe(401);
    expect((await failedLogin(listed, "203.0.113.30, unknown")).statusCode).toBe(401);
    expect((await failedLogin(listed, "203.0.113.31:4711")).statusCode).toBe(401);
    expect((await failedLogin(listed, "203.0.113.31, ")).statusCode).toBe(401);
    expect((await failedLogin(listed, `unknown, ${SECOND_HOP}`)).statusCode).toBe(401);
    expect(await counted(PEER)).toBe(5);
    expect(await counted("203.0.113.30")).toBe(0);
    expect(await counted("203.0.113.31")).toBe(0);
  });

  test("a failed TOTP confirmation is counted under the forwarded address", async () => {
    await redis.del(`rl:ip:${PEER}`);
    const res = await listed.inject({
      method: "POST",
      url: "/v1/auth/totp/confirm",
      remoteAddress: PEER,
      headers: { "x-forwarded-for": "203.0.113.60" },
      payload: { userId: globalThis.crypto.randomUUID(), totp_code: "000000" },
    });
    expect(res.statusCode).toBe(401);
    expect(await counted("203.0.113.60")).toBe(1);
    expect(await counted(PEER)).toBe(0);
  });
});

describe("from a peer the list does not hold, or with no list", () => {
  for (const [name, app] of [
    ["a list without the peer", () => notListingThePeer],
    ["no list", () => unset],
  ] as const) {
    test(`${name}: the peer's address is counted and recorded`, async () => {
      const headers = [
        "203.0.113.7",
        "2001:DB8:0:0:0:0:0:7",
        "::ffff:203.0.113.9",
        `198.51.100.4, ${SECOND_HOP}`,
      ];
      const forwarded = ["203.0.113.7", "2001:db8::7", "203.0.113.9", "198.51.100.4", SECOND_HOP];
      await redis.del(`rl:ip:${PEER}`, ...forwarded.map((a) => `rl:ip:${a}`));
      const before = await failRows(PEER);
      const confirmsBefore = await deniedConfirmations(PEER);
      for (const header of headers) {
        expect((await failedLogin(app(), header)).statusCode, header).toBe(401);
      }
      const confirm = await app().inject({
        method: "POST",
        url: "/v1/auth/totp/confirm",
        remoteAddress: PEER,
        headers: { "x-forwarded-for": "203.0.113.7" },
        payload: { userId: globalThis.crypto.randomUUID(), totp_code: "000000" },
      });
      expect(confirm.statusCode).toBe(401);
      expect(await deniedConfirmations(PEER)).toBe(confirmsBefore + 1);
      expect(await counted(PEER)).toBe(headers.length + 1);
      for (const address of forwarded) expect(await counted(address), address).toBe(0);
      expect(await failRows(PEER)).toBe(before + headers.length);
    });
  }
});

describe("the list's matching", () => {
  test("a range is matched by its prefix, with host bits ignored", () => {
    const list = parseTrustedProxies("192.0.2.37/28");
    expect(sourceAddress("192.0.2.32", "203.0.113.1", list)).toBe("203.0.113.1");
    expect(sourceAddress("192.0.2.47", "203.0.113.1", list)).toBe("203.0.113.1");
    expect(sourceAddress("192.0.2.48", "203.0.113.1", list)).toBe("192.0.2.48");
  });

  test("a mapped entry matches its IPv4 peer, and a mapped peer its IPv4 entry", () => {
    expect(
      sourceAddress("192.0.2.5", "203.0.113.1", parseTrustedProxies("::ffff:192.0.2.0/120")),
    ).toBe("203.0.113.1");
    expect(sourceAddress("::ffff:192.0.2.5", "203.0.113.1", parseTrustedProxies("192.0.2.5"))).toBe(
      "203.0.113.1",
    );
  });

  test("the entries around spaces and empty entries are the ones listed", () => {
    const list = parseTrustedProxies(" 192.0.2.10 , , 2001:db8::/32 ,");
    expect(sourceAddress("192.0.2.10", "203.0.113.1", list)).toBe("203.0.113.1");
    expect(sourceAddress("2001:db8::1", "203.0.113.1", list)).toBe("203.0.113.1");
    expect(sourceAddress("192.0.2.11", "203.0.113.1", list)).toBe("192.0.2.11");
    expect(parseTrustedProxies(" , ")).toBeNull();
  });

  test("a request with no peer address keeps none, as with no list", () => {
    const missing = undefined as unknown as string;
    expect(
      sourceAddress(missing, "203.0.113.1", parseTrustedProxies("192.0.2.10")),
    ).toBeUndefined();
    expect(sourceAddress(missing, "203.0.113.1", null)).toBeUndefined();
  });
});

describe("the list's configuration", () => {
  function envWith(list: string): NodeJS.ProcessEnv {
    return {
      API_PG_URL: config.API_PG_URL,
      API_REDIS_URL: config.API_REDIS_URL,
      API_CH_URL: config.API_CH_URL,
      API_DB_ENC_PASSPHRASE: config.API_DB_ENC_PASSPHRASE,
      API_FORENSIC_PASSPHRASE: config.API_FORENSIC_PASSPHRASE,
      API_TRUSTED_PROXIES: list,
    };
  }

  test.each([
    ["a name", "loopback"],
    ["a name beside an address", "192.0.2.10, uniquelocal"],
    ["netmask notation", "10.0.0.0/255.0.0.0"],
    ["a zone index", "fe80::1%eth0"],
    ["an IPv4 /0", "0.0.0.0/0"],
    ["an IPv6 /0", "::/0"],
    ["the whole IPv4-mapped range", "::ffff:0:0/96"],
    ["an IPv6 range that holds the IPv4-mapped range", "::/80"],
    ["a prefix past the family's length", "192.0.2.0/33"],
    ["an IPv6 prefix past the family's length", "2001:db8::/129"],
    ["an address with a port", "192.0.2.10:8080"],
  ])("%s refuses the configuration", (_name, list) => {
    expect(() => loadConfig(envWith(list))).toThrow(/API_TRUSTED_PROXIES/);
  });

  test("spaces around an entry and empty entries are ignored", () => {
    const parsed = loadConfig(envWith(" 192.0.2.10 , , 2001:db8::/32 ,"));
    expect(parsed.API_TRUSTED_PROXIES).toBe(" 192.0.2.10 , , 2001:db8::/32 ,");
  });
});
