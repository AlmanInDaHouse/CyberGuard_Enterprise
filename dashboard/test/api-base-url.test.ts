// @vitest-environment node
import { expect, test } from "vitest";
import { apiBaseUrl, apiBaseUrlProblem } from "../src/lib/api/client";

// Debt #67 (docs/handoff-session-38.md): with API_BASE_URL unset, the dashboard
// read ingest's enroll port (8080) instead of the read API. The default is the
// api's own listener: API_BIND_HOST and API_PORT as services/api defaults them.

// Runtime-only loader, as in dash-ac-001: services/api is never typechecked here.
const loadApi = (specifier: string) => import(specifier);

test("without API_BASE_URL the dashboard reads the api's default listener", async () => {
  const { loadConfig } = await loadApi("@cyberguard/api/src/config.js");
  const api = loadConfig({
    API_PG_URL: "postgres://u:p@127.0.0.1:5432/db",
    API_REDIS_URL: "redis://127.0.0.1:6379",
    API_CH_URL: "http://127.0.0.1:8123",
    API_DB_ENC_PASSPHRASE: "x",
    API_FORENSIC_PASSPHRASE: "y",
  });
  const url = new URL(apiBaseUrl({}));
  expect(url.protocol).toBe("http:");
  expect(url.hostname).toBe(api.API_BIND_HOST);
  expect(url.port).toBe(String(api.API_PORT));
});

// SPEC-023 §Data contracts §3: the start-up check's verdicts (deploy-dashboard.test.ts runs
// the refusals through the server's start).
test.each([
  [undefined, null],
  ["http://api:8081", null],
  ["https://api.example.test", null],
  ["", "must not be empty"],
  ["ftp://api.example.test", "must be an http: or https: URL"],
  ["api:8081", "must be an http: or https: URL"],
  ["not a url", "must be an http: or https: URL"],
])("API_BASE_URL=%j: %j", (value, reason) => {
  expect(apiBaseUrlProblem({ API_BASE_URL: value })).toBe(reason);
});

test("API_BASE_URL, when set, is used as given", () => {
  expect(apiBaseUrl({ API_BASE_URL: "https://api.example.test" })).toBe("https://api.example.test");
});
