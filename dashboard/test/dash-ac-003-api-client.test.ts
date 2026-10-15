// @vitest-environment node
import { type IncomingMessage, type Server, type ServerResponse, createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, expect, test } from "vitest";
import { AUTH_CALL_BOUND_MS, httpAuthApi } from "../src/lib/auth/api";

// dash_ac_003 and dash_ac_004, the production client the handlers use — SPEC-009's
// second Amendment 2026-10-15, item 1: each call to the api is bounded to 30 seconds,
// and a call the api does not answer (a connection refused, or no answer within the
// bound) rejects, which the handlers answer as 503. The bound is a parameter here so
// that the test waits 200 ms, not 30 s.

let server: Server | undefined;

afterEach(async () => {
  server?.closeAllConnections();
  await new Promise((resolve) => server?.close(resolve) ?? resolve(undefined));
  server = undefined;
});

async function serve(
  handler: (req: IncomingMessage, res: ServerResponse) => void,
): Promise<string> {
  server = createServer(handler);
  await new Promise<void>((resolve) => server?.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${(server?.address() as AddressInfo).port}`;
}

test("the bound is 30 seconds", () => {
  expect(AUTH_CALL_BOUND_MS).toBe(30_000);
});

test("an api that does not answer within the bound rejects", async () => {
  const base = await serve(() => {});
  const started = Date.now();
  await expect(httpAuthApi(base, 200).post("/v1/auth/login", {}, "{}")).rejects.toThrow();
  expect(Date.now() - started).toBeLessThan(5_000);
});

test("an api that sends its headers and stalls its body rejects", async () => {
  const base = await serve((_req, res) => {
    res.writeHead(200, { "content-type": "application/json" });
    res.write('{"csrf_token":');
  });
  await expect(httpAuthApi(base, 200).post("/v1/auth/login", {}, "{}")).rejects.toThrow();
});

test("a refused connection rejects", async () => {
  const base = await serve(() => {});
  server?.close();
  server = undefined;
  await expect(httpAuthApi(base, 200).post("/v1/auth/logout", {})).rejects.toThrow();
});

test("a call sends its method, headers and body, and reads Set-Cookie and JSON", async () => {
  let seen: { method?: string; headers?: IncomingMessage["headers"]; body?: string } = {};
  const base = await serve((req, res) => {
    let body = "";
    req.on("data", (c) => {
      body += c;
    });
    req.on("end", () => {
      seen = { method: req.method, headers: req.headers, body };
      res.setHeader("set-cookie", ["cgsess=tok; Path=/; HttpOnly", "other=1"]);
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify({ csrf_token: "csrf" }));
    });
  });
  const answer = await httpAuthApi(base).post(
    "/v1/auth/login",
    { "content-type": "application/json", "x-forwarded-for": "203.0.113.5" },
    '{"a":1}',
  );
  expect(seen.method).toBe("POST");
  expect(seen.headers?.["x-forwarded-for"]).toBe("203.0.113.5");
  expect(seen.body).toBe('{"a":1}');
  expect(answer).toEqual({
    status: 200,
    setCookie: ["cgsess=tok; Path=/; HttpOnly", "other=1"],
    body: { csrf_token: "csrf" },
  });
});

test("a body that is not JSON reads as undefined, and a redirect is not followed", async () => {
  const base = await serve((_req, res) => {
    res.writeHead(302, { location: "/elsewhere" });
    res.end("moved");
  });
  const answer = await httpAuthApi(base).post("/v1/auth/logout", {});
  expect(answer).toEqual({ status: 302, setCookie: [], body: undefined });
});
