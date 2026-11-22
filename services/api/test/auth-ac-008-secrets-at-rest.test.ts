import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  buildTestApp,
  completeInvitationAsHolder,
  getUserByEmail,
  insertUserDirect,
  seedSession,
} from "./helpers/db.js";

// auth_ac_008 — a user created through the api stores an Argon2 password hash and
// an encrypted TOTP secret, never plaintext.
//
// SPEC-026 §Operational §3 (debt #98): the api creates the user with no password and no
// secret, and the user's completion of the invitation sets both. So the property is asserted
// on the row that completion leaves (dashapi_ac_004 asserts the invitation's pending secret).
// (This AC deliberately exercises the REAL routes — not insertUserDirect — because the
// property under test is what they write.)

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a user created by invitation stores an Argon2 hash + encrypted TOTP secret, never plaintext", async () => {
  const adminId = await insertUserDirect(config, { email: "admin@ac008.test", role: "admin" });
  await seedSession(config, { token: "tok-admin-ac008", userId: adminId, role: "admin" });

  const created = await apph.app.inject({
    method: "POST",
    url: "/v1/users",
    cookies: { cgsess: "tok-admin-ac008" },
    headers: { "x-csrf-token": "csrf-tok-admin-ac008" },
    payload: { email: "made@ac008.test", role: "viewer" },
  });
  expect(created.statusCode).toBe(201);
  const { invitation } = created.json() as { invitation: { token: string } };
  const secret = await completeInvitationAsHolder(apph.app, invitation.token, "S3cret-passw0rd!");

  const row = await getUserByEmail(config, "made@ac008.test");
  expect(row).not.toBeNull();
  expect(row?.password_hash).not.toBe("S3cret-passw0rd!"); // never plaintext
  expect(row?.password_hash.startsWith("$argon2id$")).toBe(true); // Argon2id encoding
  // Ciphertext, not the secret.
  expect(row?.totp_secret.toString("latin1")).not.toContain(secret);
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const decrypted = await pool.query<{ s: string }>(
      "SELECT pgp_sym_decrypt(totp_secret, $1)::text AS s FROM users WHERE email = 'made@ac008.test'",
      [config.API_DB_ENC_PASSPHRASE],
    );
    expect(decrypted.rows[0]?.s).toBe(secret);
  } finally {
    await pool.end();
  }
});
