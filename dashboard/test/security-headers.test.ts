// @vitest-environment node
import { NextRequest } from "next/server";
import { afterEach, expect, test, vi } from "vitest";
import { middleware } from "../middleware";

// SPEC-023 §Operational §7 (debt #104), the middleware alone: the CSP with a nonce of its
// own on each response, and on the request, where Next reads the nonce it puts on its
// scripts; `next dev` keeps no CSP; the three other headers always. deploy-dashboard.test.ts
// asserts the headers on the built server's responses.

afterEach(() => {
  vi.unstubAllEnvs();
});

const request = () => new NextRequest("http://soc.example/login");

function expectOtherHeaders(res: Response): void {
  expect(res.headers.get("x-content-type-options")).toBe("nosniff");
  expect(res.headers.get("referrer-policy")).toBe("same-origin");
  expect(res.headers.get("permissions-policy")).toBe("camera=(), microphone=(), geolocation=()");
}

test("each response carries a CSP with a nonce of its own, passed on the request too", () => {
  vi.stubEnv("NODE_ENV", "production");
  const first = middleware(request());
  const second = middleware(request());
  const csp = first.headers.get("content-security-policy") ?? "";
  const nonce = /'nonce-([A-Za-z0-9+/=]{24})'/.exec(csp)?.[1];
  expect(nonce).toBeDefined();
  expect(second.headers.get("content-security-policy")).not.toContain(`'nonce-${nonce}'`);
  // NextResponse.next({ request: { headers } }) carries the request's new headers so.
  expect(first.headers.get("x-middleware-request-content-security-policy")).toBe(csp);
  expectOtherHeaders(first);
});

test("under next dev there is no CSP, and the three other headers", () => {
  vi.stubEnv("NODE_ENV", "development");
  const res = middleware(request());
  expect(res.headers.get("content-security-policy")).toBeNull();
  expect(res.headers.get("x-middleware-request-content-security-policy")).toBeNull();
  expectOtherHeaders(res);
});
