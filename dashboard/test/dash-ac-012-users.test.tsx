import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { RequestCookies } from "next/dist/server/web/spec-extension/cookies";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import UsersPage from "../app/users/page";
import { type UserItem, UsersAdmin } from "../src/components/users-admin";

// SPEC-026 §Operational §9, phase 1: `/users`, for admins — the list with its pages, a new
// user with the invitation's link shown once, a new invitation, its withdrawal, a reset, the
// role and the status. dash_ac_012, its invitation half: a new invitation is shown on the page
// that issued it and nowhere else, and a reload does not show it (the token's half comes with
// the tokens' page, phase 3).

const router = vi.hoisted(() => ({ push: vi.fn(), refresh: vi.fn() }));
const jar = vi.hoisted(() => ({ header: "" }));
vi.mock("next/headers", () => ({
  cookies: async () => new RequestCookies(new Headers({ cookie: jar.header })),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => router,
  redirect: (to: string) => {
    throw new Error(`redirect ${to}`);
  },
  notFound: () => {
    throw new Error("not found");
  },
}));

const SELF = "33333333-3333-3333-3333-333333333333";
const NEW_ID = "44444444-4444-4444-4444-444444444444";
const INVITED = "55555555-5555-5555-5555-555555555555";
const ENROLLED = "66666666-6666-6666-6666-666666666666";
const TOKEN = "Zm9yLWRhc2gtYWMtMDEyLW9ubHk_dGVzdC10b2tlbg";

const USERS: UserItem[] = [
  {
    user_id: SELF,
    email: "admin@dash012.test",
    role: "admin",
    status: "active",
    totp_enrolled: true,
    invitation: null,
  },
  {
    user_id: INVITED,
    email: "invited@dash012.test",
    role: "analyst",
    status: "active",
    totp_enrolled: false,
    invitation: { purpose: "invite", expires_at: "2026-11-25T10:00:00.000Z", expired: false },
  },
  {
    user_id: ENROLLED,
    email: "enrolled@dash012.test",
    role: "viewer",
    status: "active",
    totp_enrolled: true,
    invitation: null,
  },
];

const fetchMock = vi.fn(async (_url: URL | string, _init?: RequestInit) => new Response(null));

function answer(status: number, body?: unknown) {
  fetchMock.mockResolvedValueOnce(
    new Response(body === undefined ? null : JSON.stringify(body), { status }),
  );
}

const setItem = vi.spyOn(Storage.prototype, "setItem");
const pushState = vi.spyOn(window.history, "pushState");
const replaceState = vi.spyOn(window.history, "replaceState");

/** The token is nowhere but the view: not in the address or the history, the browser's
 *  cookies or storage, a navigation of the router, or a request's URL. */
function tokenNowhereElse() {
  expect(window.location.href).not.toContain(TOKEN);
  expect(document.cookie).not.toContain(TOKEN);
  expect(setItem).not.toHaveBeenCalled();
  expect(JSON.stringify(Object.entries(sessionStorage))).not.toContain(TOKEN);
  expect(JSON.stringify(Object.entries(localStorage))).not.toContain(TOKEN);
  for (const spy of [pushState, replaceState, router.push]) {
    for (const args of spy.mock.calls) expect(JSON.stringify(args)).not.toContain(TOKEN);
  }
  for (const [url] of fetchMock.mock.calls) expect(String(url)).not.toContain(TOKEN);
}

beforeEach(() => {
  fetchMock.mockReset();
  router.push.mockReset();
  router.refresh.mockReset();
  setItem.mockClear();
  vi.stubGlobal("fetch", fetchMock);
  window.history.replaceState(null, "", "/users");
  pushState.mockClear();
  replaceState.mockClear();
  jar.header = "__Host-cgsess=tok-012; __Host-cgcsrf=csrf-012";
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const admin = () =>
  render(
    <UsersAdmin page={{ items: USERS, next_cursor: null }} selfId={SELF} cursor={undefined} />,
  );

const row = (email: string) => {
  const cell = screen.getByText(email);
  const tr = cell.closest("tr");
  if (!tr) throw new Error(`no row for ${email}`);
  return within(tr);
};

const lastCall = () => {
  const [url, init] = fetchMock.mock.calls.at(-1) ?? [];
  return {
    url: String(url),
    body: init?.body === undefined ? undefined : JSON.parse(String(init.body)),
  };
};

const LINK = `${window.location.origin}/invitation#${TOKEN}`;

describe("dash_ac_012 — an invitation's link, shown once", () => {
  test("a new user's link is shown on the page that issued it, and a reload does not show it", async () => {
    answer(201, {
      user_id: NEW_ID,
      invitation: { token: TOKEN, expires_at: "2026-11-25T10:00:00.000Z" },
    });
    const view = admin();
    fireEvent.change(screen.getByLabelText("Email"), { target: { value: "new@dash012.test" } });
    fireEvent.change(screen.getByLabelText("Role"), { target: { value: "viewer" } });
    fireEvent.click(screen.getByRole("button", { name: "Create user" }));

    const link = await screen.findByLabelText("Invitation link");
    expect(link).toHaveValue(LINK);
    expect(screen.getByText(/shown once/)).toBeInTheDocument();
    expect(screen.getByText(/new@dash012\.test/)).toBeInTheDocument();
    expect(lastCall()).toEqual({
      url: "/api/users",
      body: { email: "new@dash012.test", role: "viewer" },
    });
    expect(router.refresh).toHaveBeenCalled();

    // Nowhere else.
    tokenNowhereElse();

    // The list the server renders again after the change keeps it.
    view.rerender(
      <UsersAdmin
        page={{ items: [...USERS], next_cursor: null }}
        selfId={SELF}
        cursor={undefined}
      />,
    );
    expect(screen.getByLabelText("Invitation link")).toHaveValue(LINK);

    // Done: it is gone.
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(screen.queryByLabelText("Invitation link")).toBeNull();
    expect(document.body.innerHTML).not.toContain(TOKEN);
  });

  test("a reload does not show it", async () => {
    answer(201, {
      user_id: NEW_ID,
      invitation: { token: TOKEN, expires_at: "2026-11-25T10:00:00.000Z" },
    });
    admin();
    fireEvent.change(screen.getByLabelText("Email"), { target: { value: "new@dash012.test" } });
    fireEvent.click(screen.getByRole("button", { name: "Create user" }));
    await screen.findByLabelText("Invitation link");
    cleanup();
    admin();
    expect(screen.queryByLabelText("Invitation link")).toBeNull();
    expect(document.body.innerHTML).not.toContain(TOKEN);
    tokenNowhereElse();
  });

  test("a new invitation's link and a reset's are shown once too", async () => {
    admin();
    answer(201, { token: TOKEN, expires_at: "2026-11-25T10:00:00.000Z" });
    fireEvent.click(row("invited@dash012.test").getByRole("button", { name: "New invitation" }));
    expect(await screen.findByLabelText("Invitation link")).toHaveValue(LINK);
    expect(lastCall()).toEqual({ url: `/api/users/${INVITED}/invitation`, body: {} });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));

    answer(201, { token: TOKEN, expires_at: "2026-11-25T10:00:00.000Z" });
    fireEvent.click(row("enrolled@dash012.test").getByRole("button", { name: "Reset" }));
    // A reset clears the user's credentials: it asks first.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    fireEvent.click(row("enrolled@dash012.test").getByRole("button", { name: "Confirm reset" }));
    expect(await screen.findByLabelText("Invitation link")).toHaveValue(LINK);
    expect(lastCall()).toEqual({ url: `/api/users/${ENROLLED}/reset`, body: {} });
    tokenNowhereElse();
  });
});

describe("/users — the controls", () => {
  test("the list shows each user's role, status, factor and open invitation", () => {
    admin();
    const invited = row("invited@dash012.test");
    expect(invited.getByText(/invite, until/)).toBeInTheDocument();
    expect(invited.getByText("not confirmed")).toBeInTheDocument();
    expect(row("enrolled@dash012.test").getByText("confirmed")).toBeInTheDocument();
  });

  test("the controls each user's state admits", () => {
    admin();
    const self = row("admin@dash012.test");
    expect(self.queryByRole("button", { name: "Reset" })).toBeNull();
    expect(self.queryByRole("button", { name: "Disable" })).toBeNull();
    expect(self.queryByRole("button", { name: "New invitation" })).toBeNull();
    const invited = row("invited@dash012.test");
    expect(invited.getByRole("button", { name: "Withdraw invitation" })).toBeInTheDocument();
    expect(invited.getByRole("button", { name: "New invitation" })).toBeInTheDocument();
    const enrolled = row("enrolled@dash012.test");
    expect(enrolled.queryByRole("button", { name: "New invitation" })).toBeNull();
    expect(enrolled.queryByRole("button", { name: "Withdraw invitation" })).toBeNull();
    expect(enrolled.getByRole("button", { name: "Reset" })).toBeInTheDocument();
  });

  test("a role, a status and a withdrawal post to their handlers and reload the list", async () => {
    admin();
    const enrolled = row("enrolled@dash012.test");
    answer(200, { ok: true });
    fireEvent.change(enrolled.getByLabelText("Role of enrolled@dash012.test"), {
      target: { value: "analyst" },
    });
    fireEvent.click(enrolled.getByRole("button", { name: "Change role" }));
    await waitFor(() => expect(router.refresh).toHaveBeenCalledTimes(1));
    expect(lastCall()).toEqual({ url: `/api/users/${ENROLLED}/role`, body: { role: "analyst" } });

    answer(200, { ok: true });
    fireEvent.click(enrolled.getByRole("button", { name: "Disable" }));
    await waitFor(() => expect(router.refresh).toHaveBeenCalledTimes(2));
    expect(lastCall()).toEqual({
      url: `/api/users/${ENROLLED}/status`,
      body: { status: "disabled" },
    });

    answer(204);
    fireEvent.click(
      row("invited@dash012.test").getByRole("button", { name: "Withdraw invitation" }),
    );
    await waitFor(() => expect(router.refresh).toHaveBeenCalledTimes(3));
    expect(lastCall()).toEqual({ url: `/api/users/${INVITED}/invitation/withdraw`, body: {} });
  });

  test("a disabled user can be enabled, and has no invitation or reset control", () => {
    render(
      <UsersAdmin
        page={{
          items: [{ ...(USERS[2] as UserItem), status: "disabled" }],
          next_cursor: null,
        }}
        selfId={SELF}
        cursor={undefined}
      />,
    );
    const r = row("enrolled@dash012.test");
    expect(r.getByRole("button", { name: "Enable" })).toBeInTheDocument();
    expect(r.queryByRole("button", { name: "Reset" })).toBeNull();
    expect(r.queryByRole("button", { name: "New invitation" })).toBeNull();
  });

  test.each([
    [409, "user_has_credentials", "This user has credentials: a reset replaces them"],
    [409, "conflict", "The org needs another active admin first"],
    [403, "forbidden", "The request was refused"],
    [404, "not_found", "The user no longer exists"],
    [503, "unavailable", "The service is unavailable"],
  ])("a refusal (%i %s) says why and shows no link", async (status, error, message) => {
    admin();
    answer(status, { error });
    fireEvent.click(row("invited@dash012.test").getByRole("button", { name: "New invitation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
    expect(screen.queryByLabelText("Invitation link")).toBeNull();
  });

  test("a creation's refusals say why", async () => {
    admin();
    answer(409, { error: "conflict" });
    fireEvent.change(screen.getByLabelText("Email"), { target: { value: "dup@dash012.test" } });
    fireEvent.click(screen.getByRole("button", { name: "Create user" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A user with this email exists");
    answer(400, { error: "bad_request" });
    fireEvent.click(screen.getByRole("button", { name: "Create user" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("Check the email and the role"),
    );
  });

  test("a session the api no longer holds goes to the login", async () => {
    admin();
    answer(401, { error: "unauthorized" });
    fireEvent.click(row("enrolled@dash012.test").getByRole("button", { name: "Disable" }));
    await waitFor(() => expect(router.push).toHaveBeenCalledWith("/login"));
  });

  test("the pages: a next page by its cursor, and back to the first", () => {
    render(<UsersAdmin page={{ items: USERS, next_cursor: "c2" }} selfId={SELF} cursor="c1" />);
    expect(screen.getByRole("link", { name: "Next page" })).toHaveAttribute(
      "href",
      "/users?cursor=c2",
    );
    expect(screen.getByRole("link", { name: "First page" })).toHaveAttribute("href", "/users");
  });
});

describe("/users — the page", () => {
  const session = (role: string) => ({ user_id: SELF, email: `${role}@dash012.test`, role });

  test.each(["viewer", "analyst"])(
    "a %s's request is not found, and reads no user",
    async (role) => {
      answer(200, session(role));
      await expect(UsersPage({ searchParams: Promise.resolve({}) })).rejects.toThrow("not found");
      expect(fetchMock).toHaveBeenCalledTimes(1);
    },
  );

  test("an admin's reads the list with the session alone, by its cursor", async () => {
    answer(200, session("admin"));
    answer(200, { items: USERS, next_cursor: null });
    render(await UsersPage({ searchParams: Promise.resolve({ cursor: "a b/c" }) }));
    const [url, init] = fetchMock.mock.calls[1] ?? [];
    expect(String(url)).toBe("http://127.0.0.1:8081/v1/users?cursor=a%20b%2Fc");
    expect(new Headers(init?.headers).get("cookie")).toBe("cgsess=tok-012");
    expect(screen.getByText("invited@dash012.test")).toBeInTheDocument();
  });

  test("a cursor the api refuses goes back to the first page", async () => {
    answer(200, session("admin"));
    answer(400, { error: "bad_request" });
    await expect(
      UsersPage({ searchParams: Promise.resolve({ cursor: "forged" }) }),
    ).rejects.toThrow("redirect /users");
  });

  test("the api's 403 is not found", async () => {
    answer(200, session("admin"));
    answer(403, { error: "forbidden" });
    await expect(UsersPage({ searchParams: Promise.resolve({}) })).rejects.toThrow("not found");
  });
});
