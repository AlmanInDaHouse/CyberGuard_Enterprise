import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect } from "./helpers/db.js";

// auth_ac_004 — login is rate-limited per account AND per source IP. SECURITY
// CONTROL (threat model § cg-api: "login rate-limiting with progressive
// back-off per account and per source IP").
//
// RED BY DESIGN: login throws NotImplemented → 501 for every attempt; no
// counters, no throttle. The RED is the absent rate-limit control. GREEN: after
// the threshold a further attempt is 429 — independently on the account axis and
// the source-IP axis.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("repeated failures for one account are throttled (429)", async () => {
  await insertUserDirect(config, { email: "rl@ac004.test", role: "viewer", totpEnrolled: true });
  let last = 0;
  for (let i = 0; i < 6; i += 1) {
    const r = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.51.100.41",
      payload: { email: "rl@ac004.test", password: "wrong", totp_code: "000000" },
    });
    last = r.statusCode;
  }
  // RED: 501 (no rate-limit). GREEN: 429 after the per-account threshold.
  expect(last).toBe(429);
});

test("repeated failures from one source IP are throttled across accounts (429)", async () => {
  let last = 0;
  for (let i = 0; i < 11; i += 1) {
    const r = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "203.0.113.7",
      payload: { email: `ip-${i}@ac004.test`, password: "wrong", totp_code: "000000" },
    });
    last = r.statusCode;
  }
  // RED: 501 (no rate-limit). GREEN: 429 after the per-IP threshold.
  expect(last).toBe(429);
});
