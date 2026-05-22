import type { FastifyInstance } from "fastify";
import { afterAll, beforeAll, expect, test } from "vitest";
import { buildApp } from "../src/app.js";

let app: FastifyInstance;

beforeAll(async () => {
  app = buildApp();
  await app.ready();
});

afterAll(async () => {
  await app.close();
});

test("GET /health returns 200 ok", async () => {
  const res = await app.inject({ method: "GET", url: "/health" });
  expect(res.statusCode).toBe(200);
  expect(res.json()).toEqual({ status: "ok" });
});

test("enroll/heartbeat are 501 placeholders in the scaffold", async () => {
  const enroll = await app.inject({ method: "POST", url: "/v1/agents/enroll" });
  const heartbeat = await app.inject({ method: "POST", url: "/v1/agents/heartbeat" });
  expect(enroll.statusCode).toBe(501);
  expect(heartbeat.statusCode).toBe(501);
});
