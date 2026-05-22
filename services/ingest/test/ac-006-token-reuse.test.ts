import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { issueToken } from "./helpers/db.js";
import { enroll } from "./helpers/test-client.js";

// AC-006 — enrolling twice with the same token: first 200, second 409;
// exactly one agents row results.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("token reuse is rejected 409; first enrollment succeeds", async () => {
  const token = await issueToken(config);

  const first = await enroll(server.enrollUrl, token);
  expect(first.agentId).toMatch(/^[0-9a-f-]{36}$/);

  await expect(enroll(server.enrollUrl, token)).rejects.toMatchObject({ status: 409 });
});
