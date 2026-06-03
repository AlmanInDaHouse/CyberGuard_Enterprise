// @vitest-environment node
import { afterAll, beforeAll, expect, test } from "vitest";
import type { ApiClient } from "../src/lib/api/client";
import { getIncidents } from "../src/lib/api/incidents";

// dash_ac_001 — auth→read integration (SPEC-009 §AC dash_ac_001), the NEW terrain.
//
// The dashboard's server-side data-access layer is exercised against the REAL
// read-API IN-PROCESS: services/api's buildApp + app.inject, backed by real
// Postgres + Redis via testcontainers (the same backends services/api uses). No
// mocks, no Docker image of the api (the (A) toll avoided) — the pnpm workspace
// lets the dashboard build + drive the api in-process (the same cross-import the
// (b) retiral proved). services/api is loaded by DYNAMIC import so its NodeNext
// source is never typechecked as part of the dashboard build.
//
// HARNESS-FIRST RED: the data-access functions are stubs (throw NotImplementedError),
// so the three dash_ac_001 cases fail on ABSENT LOGIC. The harness itself is proven
// sound by the green "harness sanity" check below (the real read-API returns 200 for
// a valid session) — so the RED is unambiguously the missing data-access mapping,
// never a broken setup. GREEN: getIncidents maps 200→{ok:true,data}, 401→{ok:false}.

let stop: (() => Promise<void>) | undefined;
let close: (() => Promise<void>) | undefined;
let client: ApiClient;
let cookieValid: string;
const org = "dash-ac-001";

// Runtime-only loader: a NON-LITERAL specifier keeps tsc from typechecking
// services/api's NodeNext source as part of the dashboard's (bundler/isolatedModules)
// build — the api is only RUN in-process under vitest, never compiled here. Same
// principle the (b) retiral used (dynamic import to avoid cross-package typecheck).
const loadApi = (specifier: string) => import(specifier);

beforeAll(async () => {
  const { startBackends } = await loadApi("@cyberguard/api/test/helpers/backends.js");
  const { buildServices } = await loadApi("@cyberguard/api/src/services.js");
  const { buildApp } = await loadApi("@cyberguard/api/src/app.js");
  const { seedSession } = await loadApi("@cyberguard/api/test/helpers/db.js");
  const { ensureAgent, insertAlertRow, insertIncidentRow } = await loadApi(
    "@cyberguard/api/test/helpers/read-schema.js",
  );

  const backends = await startBackends();
  stop = backends.stop;
  const config = backends.config;

  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertAlertRow(config, { alertId, agentId, orgId: org });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  const token = "tok-dash-ac001";
  await seedSession(config, {
    token,
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });
  cookieValid = `cgsess=${token}`;

  const services = await buildServices(config);
  const app = buildApp(services);
  await app.ready();
  close = async () => {
    await app.close();
    await services.close();
  };

  client = {
    async get(path, cookieHeader) {
      const res = await app.inject({
        method: "GET",
        url: path,
        headers: cookieHeader ? { cookie: cookieHeader } : {},
      });
      return { status: res.statusCode, body: res.statusCode === 200 ? res.json() : undefined };
    },
  };
});

afterAll(async () => {
  await close?.();
  await stop?.();
});

// GREEN GUARD (truth over theatre): proves the in-process harness is sound — the
// REAL read-API returns 200 + items for a valid session. Stays green at the RED
// gate, so the three failures below are demonstrably logic-absent, not setup-broken.
test("harness sanity: the real read-API returns 200 for a valid session", async () => {
  const res = await client.get("/v1/incidents", cookieValid);
  expect(res.status).toBe(200);
  expect((res.body as { items: unknown[] }).items.length).toBeGreaterThanOrEqual(1);
});

test("dash_ac_001a: valid session → returns incident data from the read-API", async () => {
  const result = await getIncidents(client, cookieValid);
  expect(result.ok).toBe(true);
  if (result.ok) expect(result.data.items.length).toBeGreaterThanOrEqual(1);
});

test("dash_ac_001b: no session → unauthenticated (RSC redirects to login)", async () => {
  const result = await getIncidents(client, undefined);
  expect(result).toEqual({ ok: false, reason: "unauthenticated" });
});

test("dash_ac_001c: read-API 401 (invalid/revoked) → unauthenticated", async () => {
  const result = await getIncidents(client, "cgsess=not-a-real-token");
  expect(result).toEqual({ ok: false, reason: "unauthenticated" });
});
