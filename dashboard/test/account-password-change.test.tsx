import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { PasswordChangeForm } from "../src/components/password-change-form";

// SPEC-026 §Operational §9, phase 1: `/account`, the password change, which then clears both
// cookies (the handler's, dash_ac_009) and returns to `/login`.

const router = vi.hoisted(() => ({ push: vi.fn(), refresh: vi.fn() }));
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

function answer(status: number, body: unknown = {}) {
  fetchMock.mockResolvedValueOnce(new Response(JSON.stringify(body), { status }));
}

function submit() {
  fireEvent.change(screen.getByLabelText("Current password"), { target: { value: "old pw 1" } });
  fireEvent.change(screen.getByLabelText("New password"), { target: { value: "new password 12" } });
  fireEvent.change(screen.getByLabelText("Repeat the new password"), {
    target: { value: "new password 12" },
  });
  fireEvent.change(screen.getByLabelText("Authenticator code"), { target: { value: "123456" } });
  fireEvent.click(screen.getByRole("button", { name: "Change password" }));
}

test("posts the three fields as JSON, then goes to /login", async () => {
  answer(200, { ok: true });
  render(<PasswordChangeForm />);
  submit();
  await waitFor(() => expect(router.push).toHaveBeenCalledWith("/login"));
  expect(router.refresh).toHaveBeenCalled();
  const [url, init] = fetchMock.mock.calls[0] ?? [];
  expect(url).toBe("/api/account/password");
  expect(init?.method).toBe("POST");
  expect(new Headers(init?.headers).get("content-type")).toBe("application/json");
  expect(JSON.parse(String(init?.body))).toEqual({
    currentPassword: "old pw 1",
    newPassword: "new password 12",
    totp_code: "123456",
  });
});

test("a repeated password that differs is refused before any request", async () => {
  render(<PasswordChangeForm />);
  fireEvent.change(screen.getByLabelText("Current password"), { target: { value: "old pw 1" } });
  fireEvent.change(screen.getByLabelText("New password"), { target: { value: "new password 12" } });
  fireEvent.change(screen.getByLabelText("Repeat the new password"), {
    target: { value: "new password 13" },
  });
  fireEvent.change(screen.getByLabelText("Authenticator code"), { target: { value: "123456" } });
  fireEvent.click(screen.getByRole("button", { name: "Change password" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The new passwords differ");
  expect(fetchMock).not.toHaveBeenCalled();
});

test.each([
  [400, "Check the fields: a new password has 12 to 128 characters, a letter and a digit"],
  [401, "The current password or the code was not accepted"],
  [403, "The request was refused"],
  [429, "Too many attempts; try again later"],
  [503, "The service is unavailable"],
])("on %i it stays, says why and clears the secrets", async (status, message) => {
  answer(status, { error: "x" });
  render(<PasswordChangeForm />);
  submit();
  expect(await screen.findByRole("alert")).toHaveTextContent(message);
  expect(router.push).not.toHaveBeenCalled();
  for (const label of [
    "Current password",
    "New password",
    "Repeat the new password",
    "Authenticator code",
  ]) {
    expect(screen.getByLabelText(label)).toHaveValue("");
  }
});
