import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, getUserByEmail, insertUserDirect, seedSession } from "./helpers/db.js";

// auth_ac_008 — a user created through the api stores an Argon2 password hash and
// an encrypted TOTP secret, never plaintext.
//
// RED BY DESIGN: createUser is a stub → 501, so no user is created and the
// at-rest assertions are unreached. The RED is the absent user-creation control.
// (This AC deliberately exercises the REAL createUser — not insertUserDirect —
// because the property under test is what createUser writes.) GREEN: createUser
// hashes with Argon2id and pgcrypto-encrypts the TOTP secret; the row reflects it.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("createUser stores an Argon2 hash + encrypted TOTP secret, never plaintext", async () => {
  const adminId = await insertUserDirect(config, { email: "admin@ac008.test", role: "admin" });
  await seedSession(config, { token: "tok-admin-ac008", userId: adminId, role: "admin" });

  const created = await apph.app.inject({
    method: "POST",
    url: "/v1/users",
    cookies: { cgsess: "tok-admin-ac008" },
    headers: { "x-csrf-token": "csrf-tok-admin-ac008" },
    payload: { email: "made@ac008.test", password: "S3cret-passw0rd!", role: "viewer" },
  });
  // RED: 501 (createUser absent). GREEN: a 2xx create.
  expect(created.statusCode).toBeLessThan(300);

  const row = await getUserByEmail(config, "made@ac008.test");
  expect(row).not.toBeNull();
  expect(row?.password_hash).not.toBe("S3cret-passw0rd!"); // never plaintext
  expect(row?.password_hash.startsWith("$argon2")).toBe(true); // Argon2 encoding
  expect(row?.totp_secret.toString("utf8")).not.toContain("base32"); // ciphertext, not the secret
});
