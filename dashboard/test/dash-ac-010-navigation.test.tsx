import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { RequestCookies } from "next/dist/server/web/spec-extension/cookies";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import AccountPage from "../app/account/page";
import IncidentDetailPage from "../app/incidents/[id]/page";
import IncidentsPage from "../app/incidents/page";
import { navEntries } from "../src/components/app-nav";

// dash_ac_010, its part in SPEC-026 §Operational §9's first phase: the navigation shared by
// every page, its entries by the role `GET /v1/auth/session` answers; no entry for the users'
// page but for an admin. The incident page's triage controls and downloads, and the agents',
// tokens' and audit view's entries, come with the phases that bring those pages.

const jar = vi.hoisted(() => ({ header: "" }));
vi.mock("next/headers", () => ({
  cookies: async () => new RequestCookies(new Headers({ cookie: jar.header })),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
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

/** The api's answers, in the order the page asks: the session, then the page's read. */
function answers(...bodies: [number, unknown?][]) {
  for (const [status, body] of bodies) {
    fetchMock.mockResolvedValueOnce(
      new Response(body === undefined ? null : JSON.stringify(body), { status }),
    );
  }
}

const session = (role: string) => ({
  user_id: "33333333-3333-3333-3333-333333333333",
  email: `${role}@dash010.test`,
  role,
});

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
  jar.header = "__Host-cgsess=tok-010; __Host-cgcsrf=csrf-010";
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

test("the entries by role: the users' page for an admin alone", () => {
  const labels = (role: string) => navEntries(role).map((e) => `${e.label} ${e.href}`);
  expect(labels("viewer")).toEqual(["Incidents /incidents", "Account /account"]);
  expect(labels("analyst")).toEqual(["Incidents /incidents", "Account /account"]);
  expect(labels("admin")).toEqual(["Incidents /incidents", "Users /users", "Account /account"]);
  expect(labels("Admin")).toEqual(["Incidents /incidents", "Account /account"]);
});

const PAGES = [
  ["the incidents list", () => IncidentsPage(), { items: [], next_cursor: null }],
  [
    "the incident detail",
    () => IncidentDetailPage({ params: Promise.resolve({ id: INCIDENT.incident_id }) }),
    INCIDENT,
  ],
  ["the account page", () => AccountPage(), undefined],
] as const;

test.each(PAGES)("%s reads the session first, with the session alone", async (_n, page, body) => {
  answers(
    [200, session("viewer")],
    ...(body === undefined ? [] : [[200, body] as [number, unknown]]),
  );
  render(await page());
  const [url] = fetchMock.mock.calls[0] ?? [];
  expect(new URL(String(url)).pathname).toBe("/v1/auth/session");
  for (const [, init] of fetchMock.mock.calls) {
    expect(new Headers(init?.headers).get("cookie")).toBe("cgsess=tok-010");
  }
});

test.each(PAGES)("%s shows the navigation by role", async (_n, page, body) => {
  for (const role of ["viewer", "analyst", "admin"]) {
    answers(
      [200, session(role)],
      ...(body === undefined ? [] : [[200, body] as [number, unknown]]),
    );
    render(await page());
    const nav = screen.getByRole("navigation", { name: "Main" });
    const links = within(nav)
      .getAllByRole("link")
      .map((a) => a.getAttribute("href"));
    expect(links).toEqual(
      role === "admin" ? ["/incidents", "/users", "/account"] : ["/incidents", "/account"],
    );
    expect(screen.getByText(`${role}@dash010.test`)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign out" })).toBeInTheDocument();
    cleanup();
  }
});

test.each(PAGES)("%s sends a session the api does not hold to the login", async (_n, page) => {
  answers([401]);
  await expect(page()).rejects.toThrow("redirect /login");
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

test.each(PAGES)("%s sends no Cookie header without a session", async (_n, page) => {
  jar.header = "__Host-cgcsrf=csrf-010";
  answers([401]);
  await expect(page()).rejects.toThrow("redirect /login");
  expect(new Headers(fetchMock.mock.calls[0]?.[1]?.headers).get("cookie")).toBeNull();
});
