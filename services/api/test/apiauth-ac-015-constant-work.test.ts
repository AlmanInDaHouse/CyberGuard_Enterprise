import { afterAll, beforeAll, expect, inject, test, vi } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { buildTestApp, insertUserDirect } from "./helpers/db.js";

// apiauth_ac_015 — SPEC-008 Amendment 2026-10-15 item 11, NFR-008-003: a login for an
// unknown address, and one for a disabled user, run the Argon2id verification, as a
// login with a wrong password does, so the work done does not tell whether an active
// account holds the address.

const calls = vi.hoisted(() => ({ verify: 0, hashes: [] as string[] }));
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

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  await insertUserDirect(config, { email: "active@apiauth015.test", role: "viewer" });
  await insertUserDirect(config, {
    email: "disabled@apiauth015.test",
    role: "viewer",
    status: "disabled",
  });
});
afterAll(async () => {
  await apph?.close();
});

test.each([
  ["a wrong password", "active@apiauth015.test", "198.51.100.151"],
  ["an unknown address", "nobody@apiauth015.test", "198.51.100.152"],
  ["a disabled user", "disabled@apiauth015.test", "198.51.100.153"],
])("%s runs one Argon2id verification and answers 401", async (_label, email, ip) => {
  const before = calls.verify;
  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: ip,
    payload: { email, password: "not-the-password", totp_code: "000000" },
  });
  expect(res.statusCode).toBe(401);
  expect(calls.verify - before).toBe(1);
  // Against an Argon2id hash, so the work is Argon2's, not a parse failure.
  expect(calls.hashes.at(-1)?.startsWith("$argon2id$")).toBe(true);
});

test("a login for an unknown address also runs the decrypt a found row costs", async () => {
  const services = await buildServices(config);
  const decrypts: string[] = [];
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
  const app = buildApp({ ...services, auth: createAuthService(counting, services.redis, config) });
  await app.ready();
  try {
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.51.100.154",
      payload: { email: "nobody-2@apiauth015.test", password: "x", totp_code: "000000" },
    });
    expect(res.statusCode).toBe(401);
    expect(decrypts).toHaveLength(1);
  } finally {
    await app.close();
    await services.close();
  }
});
