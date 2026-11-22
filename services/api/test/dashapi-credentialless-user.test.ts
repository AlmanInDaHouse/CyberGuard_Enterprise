import pg from "pg";
import { afterAll, beforeAll, expect, inject, test, vi } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { currentTotp, getAuditRows } from "./helpers/db.js";

// SPEC-026 §Data contracts §2 and §Operational §3 — a user with neither a password nor a TOTP
// secret, an invitation's or a reset's, cannot sign in; their login takes the fixed-hash
// verification an unknown address takes (SPEC-008 Amendment 2026-10-15 item 11), the decrypt a
// found row costs included, so that its work does not tell an invited address from an unknown
// one; and `POST /v1/auth/totp/confirm` refuses a user with no secret as it refuses an unknown
// one (item 12).

const calls = vi.hoisted(() => ({ verify: 0, hashes: [] as unknown[] }));
vi.mock("../src/auth/password.js", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../src/auth/password.js")>();
  return {
    ...mod,
    verifyPassword: async (stored: string, password: string) => {
      calls.verify += 1;
      calls.hashes.push(stored);
      return mod.verifyPassword(stored, password);
    },
  };
});

let config: Config;
let services: Awaited<ReturnType<typeof buildServices>>;
let app: ReturnType<typeof buildApp>;
const decrypts: string[] = [];
const USER = "01934abc-def0-7000-8c00-000000000001";
const EMAIL = "invited@dashapi-credentialless.test";

beforeAll(async () => {
  config = inject("apiConfig");
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
       VALUES ($1, 'default', $2, NULL, NULL, false, 'analyst')`,
      [USER, EMAIL],
    );
  } finally {
    await pool.end();
  }
  services = await buildServices(config);
  // Counts the decrypt of a ciphertext the service makes when it found no row to decrypt.
  const counting = new Proxy(services.pg, {
    get(target, prop, receiver) {
      if (prop === "query") {
        return (text: unknown, values?: unknown[]) => {
          if (typeof text === "string" && text.includes("pgp_sym_decrypt($1::bytea")) {
            decrypts.push(text);
          }
          return target.query(text as string, values);
        };
      }
      return Reflect.get(target, prop, receiver);
    },
  });
  app = buildApp({ ...services, auth: createAuthService(counting, services.redis, config) });
  await app.ready();
});
afterAll(async () => {
  await app?.close();
  await services?.close();
});

test("a login of a user with no credentials runs the unknown address's work and answers 401", async () => {
  const verifies = calls.verify;
  const decrypted = decrypts.length;
  const res = await app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: "198.51.100.201",
    payload: { email: EMAIL, password: "any password 1", totp_code: currentTotp() },
  });
  expect(res.statusCode).toBe(401);
  expect(res.json()).toEqual({ error: "unauthorized" });
  expect(calls.verify - verifies).toBe(1);
  // Against the fixed Argon2id hash, as an unknown address's login verifies.
  expect(String(calls.hashes.at(-1)).startsWith("$argon2id$")).toBe(true);
  expect(decrypts.length - decrypted).toBe(1);
  const rows = (await getAuditRows(config, { action: "auth.login.fail" })).filter(
    (r) => r.user_id === USER,
  );
  expect(rows).toEqual([{ action: "auth.login.fail", user_id: USER, outcome: "denied" }]);
});

test("the TOTP confirmation refuses a user with no secret as an unknown one", async () => {
  const unknown = globalThis.crypto.randomUUID();
  const answers = [];
  for (const [userId, remoteAddress] of [
    [USER, "198.51.100.202"],
    [unknown, "198.51.100.203"],
  ] as const) {
    const decrypted = decrypts.length;
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/totp/confirm",
      remoteAddress,
      payload: { userId, totp_code: "123456" },
    });
    // The same work, the decrypt a found row costs, for both.
    expect(decrypts.length - decrypted, userId).toBe(1);
    answers.push({ status: res.statusCode, body: res.json() });
    // Each a counted failure, and no code marked used.
    expect(Number(await services.redis.get(`rl:user:${userId}`)), userId).toBe(1);
    expect(await services.redis.exists(`totp:replay:${userId}:123456`), userId).toBe(0);
  }
  expect(answers).toEqual([
    { status: 401, body: { error: "unauthorized" } },
    { status: 401, body: { error: "unauthorized" } },
  ]);
  const rows = (
    await services.pg.query(
      `SELECT action, user_id, outcome FROM audit_log
        WHERE action = 'user.totp.enroll' AND source_ip IN ('198.51.100.202', '198.51.100.203')
        ORDER BY id`,
    )
  ).rows;
  // The user's row names them, as a disabled user's login row does; the unknown id's names none.
  expect(rows).toEqual([
    { action: "user.totp.enroll", user_id: USER, outcome: "denied" },
    { action: "user.totp.enroll", user_id: null, outcome: "denied" },
  ]);
  const r = await services.pg.query<{ totp_enrolled: boolean }>(
    "SELECT totp_enrolled FROM users WHERE user_id = $1",
    [USER],
  );
  expect(r.rows).toEqual([{ totp_enrolled: false }]);
});
