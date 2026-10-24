// @vitest-environment node
import { RequestCookies } from "next/dist/server/web/spec-extension/cookies";
import { afterEach, expect, test, vi } from "vitest";
import { type ApiClient, httpApiClient } from "../src/lib/api/client";
import { getIncidentDetail, getIncidents } from "../src/lib/api/incidents";
import { sessionCookieHeader } from "../src/lib/auth/cookies";

// dash_ac_007 — SPEC-009's second Amendment 2026-10-15, item 6 (debt #97): a page reads
// with the session alone, `Cookie: cgsess=<token>` (the api's name for the browser's
// `__Host-cgsess`, SPEC-023 §Operational §7), and with no Cookie header when the
// browser holds no session. The pages pass `sessionCookieHeader(await cookies())` to the
// read client; Next's `cookies()` store is built on `RequestCookies`.

afterEach(() => {
  vi.unstubAllGlobals();
});

function storeOf(cookieHeader: string) {
  return new RequestCookies(new Headers({ cookie: cookieHeader }));
}

function recording() {
  const sent: (string | undefined)[] = [];
  const client: ApiClient = {
    async get(_path, cookieHeader) {
      sent.push(cookieHeader);
      return { status: 401, body: undefined };
    },
  };
  return { client, sent };
}

test("a page read with cgsess and two other cookies sends the session alone", async () => {
  const store = storeOf("theme=dark; __Host-cgsess=tok-007; __Host-cgcsrf=csrf-007");
  const { client, sent } = recording();
  await getIncidents(client, sessionCookieHeader(store));
  await getIncidentDetail(client, sessionCookieHeader(store), "id");
  expect(sent).toEqual(["cgsess=tok-007", "cgsess=tok-007"]);
});

test("a cookie named cgsess, without the prefix, is no session", () => {
  expect(sessionCookieHeader(storeOf("cgsess=tok-007"))).toBeUndefined();
});

test("without cgsess, the page sends no Cookie header", async () => {
  const store = storeOf("theme=dark; __Host-cgcsrf=csrf-007");
  expect(sessionCookieHeader(store)).toBeUndefined();
  const fetchMock = vi.fn(
    async (_url: URL | string, _init?: RequestInit) => new Response(null, { status: 401 }),
  );
  vi.stubGlobal("fetch", fetchMock);
  await getIncidents(httpApiClient("http://api.test"), sessionCookieHeader(store));
  const init = fetchMock.mock.calls[0]?.[1];
  expect(new Headers(init?.headers).has("cookie")).toBe(false);
});

test("the read client sends exactly the header it is given", async () => {
  const fetchMock = vi.fn(
    async (_url: URL | string, _init?: RequestInit) => new Response(null, { status: 401 }),
  );
  vi.stubGlobal("fetch", fetchMock);
  await getIncidents(
    httpApiClient("http://api.test"),
    sessionCookieHeader(storeOf("a=1; __Host-cgsess=t; b=2")),
  );
  const init = fetchMock.mock.calls[0]?.[1];
  expect(new Headers(init?.headers).get("cookie")).toBe("cgsess=t");
});
