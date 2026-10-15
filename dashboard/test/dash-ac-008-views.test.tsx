import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import LoginPage from "../app/login/page";
import { LoginForm } from "../src/components/login-form";
import { SignOutButton } from "../src/components/sign-out-button";

// dash_ac_008 — SPEC-009's second Amendment 2026-10-15, item 7: the login view posts
// its three fields as JSON and navigates by the answer; the sign-out button posts to the
// logout and navigates, or stays, by the answer. The router is Next's `useRouter`.

const router = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => router }));

const fetchMock = vi.fn(async (_url: string, _init?: RequestInit) => new Response(null));

beforeEach(() => {
  fetchMock.mockReset();
  router.push.mockReset();
  router.refresh.mockReset();
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function answer(status: number) {
  fetchMock.mockResolvedValueOnce(new Response("{}", { status }));
}

function signIn() {
  fireEvent.change(screen.getByLabelText("Email"), { target: { value: "ana@soc.test" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "pw" } });
  fireEvent.change(screen.getByLabelText("Authenticator code"), { target: { value: "123456" } });
  fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
}

test("the login view posts the three fields as JSON and navigates to /incidents on 200", async () => {
  answer(200);
  render(<LoginForm />);
  signIn();
  await waitFor(() => expect(router.push).toHaveBeenCalledWith("/incidents"));
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const [url, init] = fetchMock.mock.calls[0] ?? [];
  expect(url).toBe("/api/auth/login");
  expect(init?.method).toBe("POST");
  expect(new Headers(init?.headers).get("content-type")).toBe("application/json");
  expect(JSON.parse(String(init?.body))).toEqual({
    email: "ana@soc.test",
    password: "pw",
    totp_code: "123456",
  });
});

test.each([
  [401, "Sign-in failed"],
  [400, "Check the fields"],
  [403, "The request was refused"],
  [429, "Too many attempts; try again later"],
  [502, "The service is unavailable"],
  [503, "The service is unavailable"],
])("on %i the login view says %s and keeps the email address", async (status, message) => {
  answer(status);
  render(<LoginForm />);
  signIn();
  expect(await screen.findByRole("alert")).toHaveTextContent(message);
  expect(screen.getByLabelText("Email")).toHaveValue("ana@soc.test");
  expect(router.push).not.toHaveBeenCalled();
});

test("the sign-out button posts to /api/auth/logout and navigates to /login on 200", async () => {
  answer(200);
  render(<SignOutButton />);
  fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
  await waitFor(() => expect(router.push).toHaveBeenCalledWith("/login"));
  expect(router.refresh).toHaveBeenCalled();
  const [url, init] = fetchMock.mock.calls[0] ?? [];
  expect(url).toBe("/api/auth/logout");
  expect(init?.method).toBe("POST");
});

test.each([502, 503])("on %i the sign-out navigates to /login with the notice", async (status) => {
  answer(status);
  render(<SignOutButton />);
  fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
  await waitFor(() => expect(router.push).toHaveBeenCalledWith("/login?notice=signed-out"));
  expect(router.refresh).toHaveBeenCalled();
  cleanup();
  render(await LoginPage({ searchParams: Promise.resolve({ notice: "signed-out" }) }));
  expect(screen.getByRole("status")).toHaveTextContent(
    "This browser is signed out; the server may keep the session until it expires.",
  );
});

test("on 403 the sign-out stays and says the request was refused", async () => {
  answer(403);
  render(<SignOutButton />);
  fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The request was refused");
  expect(router.push).not.toHaveBeenCalled();
  expect(router.refresh).not.toHaveBeenCalled();
});

test("the login view drops the router's cached views when it opens", () => {
  render(<LoginForm />);
  expect(router.refresh).toHaveBeenCalledTimes(1);
});

test("the login view shows no notice without ?notice=signed-out", async () => {
  render(await LoginPage({ searchParams: Promise.resolve({}) }));
  expect(screen.queryByRole("status")).toBeNull();
});
