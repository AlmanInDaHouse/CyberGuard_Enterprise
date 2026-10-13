import { expect, test } from "vitest";
import { loadConfig } from "../src/config.js";

// Debt #33 / SPEC-018 Amendment 2026-10-14: the detection driver's default
// interval is NFR-006-001's 5000 ms; a deployment that sets the variable keeps it.

const REQUIRED = {
  INGEST_PG_URL: "postgres://u:p@127.0.0.1:5432/db",
  INGEST_CH_URL: "http://127.0.0.1:8123",
  INGEST_REDIS_URL: "redis://127.0.0.1:6379",
  INGEST_CA_PASSPHRASE: "x",
};

test("INGEST_DETECT_INTERVAL_MS defaults to 5000 ms", () => {
  expect(loadConfig(REQUIRED).INGEST_DETECT_INTERVAL_MS).toBe(5000);
});

test("an INGEST_DETECT_INTERVAL_MS the deployment sets is kept, 0 included", () => {
  expect(
    loadConfig({ ...REQUIRED, INGEST_DETECT_INTERVAL_MS: "12000" }).INGEST_DETECT_INTERVAL_MS,
  ).toBe(12000);
  expect(
    loadConfig({ ...REQUIRED, INGEST_DETECT_INTERVAL_MS: "0" }).INGEST_DETECT_INTERVAL_MS,
  ).toBe(0);
});
