import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";

// /health smoke test on the real plain-HTTP enroll listener (FR-002).

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

test("GET /health returns 200 ok", async () => {
  const res = await fetch(`${server.enrollUrl}/health`);
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual({ status: "ok" });
});
