import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { DEFAULT_PASSWORD, buildTestApp, currentTotp, insertUserDirect } from "./helpers/db.js";

// apiauth_ac_016 — SPEC-008 Amendment 2026-10-15 item 12 (debt #81): the TOTP
// confirmation confirms only a user whose TOTP is not yet confirmed, is throttled like
// the login (per user id, and on the login's per-IP counter), writes a denied row for
// a failure, and marks the code it accepts used, so it cannot open a session.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

async function enrolled(userId: string): Promise<boolean | undefined> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<{ totp_enrolled: boolean }>(
      "SELECT totp_enrolled FROM users WHERE user_id = $1",
      [userId],
    );
    return r.rows[0]?.totp_enrolled;
  } finally {
    await pool.end();
  }
}

async function deniedRows(userId: string): Promise<number> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query(
      "SELECT 1 FROM audit_log WHERE action = 'user.totp.enroll' AND outcome = 'denied' AND user_id = $1",
      [userId],
    );
    return r.rowCount ?? 0;
  } finally {
    await pool.end();
  }
}

function confirm(userId: string, code: string, ip: string) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/auth/totp/confirm",
    remoteAddress: ip,
    payload: { userId, totp_code: code },
  });
}

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a user whose TOTP is confirmed cannot be confirmed again, and the attempt is recorded", async () => {
  const userId = await insertUserDirect(config, {
    email: "enrolled@apiauth016.test",
    role: "viewer",
    totpEnrolled: true,
  });
  const res = await confirm(userId, currentTotp(), "198.51.100.161");
  expect(res.statusCode).toBe(401);
  expect(await enrolled(userId)).toBe(true);
  expect(await deniedRows(userId)).toBe(1);
});

test("failures are throttled per user id, whatever the case of the id", async () => {
  const userId = await insertUserDirect(config, {
    email: "throttled@apiauth016.test",
    role: "viewer",
    totpEnrolled: false,
  });
  // Five failures from five addresses, half of them with the id in upper case.
  for (let i = 0; i < 5; i++) {
    const id = i % 2 === 0 ? userId : userId.toUpperCase();
    const res = await confirm(id, "000000", `198.51.100.${170 + i}`);
    expect(res.statusCode, `attempt ${i}`).toBe(401);
  }
  const sixth = await confirm(userId, currentTotp(), "198.51.100.179");
  expect(sixth.statusCode).toBe(429);
  expect(await enrolled(userId)).toBe(false);
});

test("failures count on the login's per-IP counter", async () => {
  const ip = "198.51.100.190";
  for (let i = 0; i < 10; i++) {
    const res = await confirm(globalThis.crypto.randomUUID(), "000000", ip);
    expect(res.statusCode, `attempt ${i}`).toBe(401);
  }
  const login = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: ip,
    payload: { email: "anyone@apiauth016.test", password: "x", totp_code: "000000" },
  });
  expect(login.statusCode).toBe(429);
});

test("failed logins from an address block the confirmation from it too", async () => {
  const ip = "198.51.100.195";
  for (let i = 0; i < 10; i++) {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: ip,
      payload: { email: `nobody-${i}@apiauth016.test`, password: "x", totp_code: "000000" },
    });
    expect(res.statusCode, `attempt ${i}`).toBe(401);
  }
  const userId = await insertUserDirect(config, {
    email: "blocked@apiauth016.test",
    role: "viewer",
    totpEnrolled: false,
  });
  const res = await confirm(userId, currentTotp(), ip);
  expect(res.statusCode).toBe(429);
  expect(await enrolled(userId)).toBe(false);
});

test("a failed confirmation for an id no user has writes a denied row with no user", async () => {
  const res = await confirm(globalThis.crypto.randomUUID(), "000000", "198.51.100.196");
  expect(res.statusCode).toBe(401);
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query(
      "SELECT user_id, outcome FROM audit_log WHERE action = 'user.totp.enroll' AND host(source_ip) = '198.51.100.196'",
    );
    expect(r.rows).toEqual([{ user_id: null, outcome: "denied" }]);
  } finally {
    await pool.end();
  }
});

test("a code the confirmation accepted cannot open a session", async () => {
  const userId = await insertUserDirect(config, {
    email: "fresh@apiauth016.test",
    role: "viewer",
    totpEnrolled: false,
  });
  const code = currentTotp();
  const ok = await confirm(userId, code, "198.51.100.200");
  expect(ok.statusCode).toBe(200);
  expect(await enrolled(userId)).toBe(true);
  const login = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: "198.51.100.201",
    payload: { email: "fresh@apiauth016.test", password: DEFAULT_PASSWORD, totp_code: code },
  });
  expect(login.statusCode).toBe(401);
  expect(login.cookies.find((c) => c.name === "cgsess")).toBeUndefined();
});
