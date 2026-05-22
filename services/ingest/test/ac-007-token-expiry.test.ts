import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import { enroll } from "./helpers/test-client.js";

// AC-007 — enrolling with an already-expired token is rejected 401; no
// agents row.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("expired token is rejected 401", async () => {
  const expiredToken = await issueToken(config, { expiresInMs: -60_000 });
  await expect(enroll(server.enrollUrl, expiredToken)).rejects.toMatchObject({ status: 401 });
});
