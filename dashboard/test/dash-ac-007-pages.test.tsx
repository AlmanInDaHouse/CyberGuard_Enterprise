import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { RequestCookies } from "next/dist/server/web/spec-extension/cookies";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import IncidentDetailPage from "../app/incidents/[id]/page";
import IncidentsPage from "../app/incidents/page";

// dash_ac_007 and dash_ac_008, at the pages themselves — SPEC-009's second Amendment
// 2026-10-15, items 6 and 7: the incident pages read with `Cookie: cgsess=<token>` (the
// browser's `__Host-cgsess` under the api's name, SPEC-023 §Operational §7) and
// no other cookie, with no Cookie header without a session, and carry the sign-out
// button. Next's `cookies()` is the browser's cookies; `fetch` is the read API.

const jar = vi.hoisted(() => ({ header: "" }));
vi.mock("next/headers", () => ({
  cookies: async () => new RequestCookies(new Headers({ cookie: jar.header })),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn() }),
  redirect: (to: string) => {
    throw new Error(`redirect ${to}`);
  },
  notFound: () => {
    throw new Error("not found");
  },
}));

const INCIDENT = {
  incident_id: "11111111-1111-1111-1111-111111111111",
  agent_id: "22222222-2222-2222-2222-222222222222",
  status: "open",
  title: "t",
  severity_id: 4,
  cg_mitre: { tactics: [], techniques: [] },
  window_start: "2026-06-01T00:00:00.000Z",
  assigned_to: null,
  created_at: "2026-06-01T00:00:00.000Z",
  updated_at: "2026-06-01T00:00:00.000Z",
  alerts: [],
};

const fetchMock = vi.fn(async (_url: URL | string, _init?: RequestInit) => new Response(null));

function answer(status: number, body?: unknown) {
  fetchMock.mockResolvedValueOnce(
    new Response(body === undefined ? null : JSON.stringify(body), { status }),
  );
}

function cookieSent(): string | null {
  return new Headers(fetchMock.mock.calls[0]?.[1]?.headers).get("cookie");
}

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const PAGES = [
  ["the incidents list", () => IncidentsPage(), { items: [], next_cursor: null }],
  [
    "the incident detail",
    () => IncidentDetailPage({ params: Promise.resolve({ id: INCIDENT.incident_id }) }),
    INCIDENT,
  ],
] as const;

test.each(PAGES)("%s reads with the session alone and shows sign-out", async (_n, page, body) => {
  jar.header = "theme=dark; __Host-cgsess=tok-pages; __Host-cgcsrf=csrf-pages";
  answer(200, body);
  render(await page());
  expect(cookieSent()).toBe("cgsess=tok-pages");
  expect(screen.getByRole("button", { name: "Sign out" })).toBeInTheDocument();
});

test.each(PAGES)("%s reads a cgsess of another shape as no session", async (_n, page) => {
  jar.header = "__Host-cgsess=a%0Ab; __Host-cgcsrf=csrf-pages";
  answer(401);
  await expect(page()).rejects.toThrow("redirect /login");
  expect(cookieSent()).toBeNull();
});

test.each(PAGES)("%s sends no Cookie header without a session", async (_n, page) => {
  jar.header = "theme=dark; __Host-cgcsrf=csrf-pages";
  answer(401);
  await expect(page()).rejects.toThrow("redirect /login");
  expect(cookieSent()).toBeNull();
});
