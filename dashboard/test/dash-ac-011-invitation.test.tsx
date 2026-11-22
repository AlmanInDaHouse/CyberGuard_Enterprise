import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { encodeQR } from "qr";
import { StrictMode } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import InvitationPage from "../app/invitation/page";

// dash_ac_011 (SPEC-026 §Acceptance criteria): the invitation page reads the token from the
// fragment, removes it from the address, shows the QR code and the URI, and completes the
// invitation; no request carries the token in a URL (§Operational §3 and §9).

const TOKEN = "kV3n_Q8-Zx1aB2cD3eF4gH5iJ6kL7mN8oP9qR0sT1uV";
const URI =
  "otpauth://totp/CyberGuard:ana%40dash011.test?secret=JBSWY3DPEHPK3PXP&issuer=CyberGuard";

const fetchMock = vi.fn(async (_url: string, _init?: RequestInit) => new Response(null));

function answer(status: number, body?: unknown) {
  fetchMock.mockResolvedValueOnce(
    new Response(body === undefined ? null : JSON.stringify(body), { status }),
  );
}

const setItem = vi.spyOn(Storage.prototype, "setItem");

beforeEach(() => {
  fetchMock.mockReset();
  setItem.mockClear();
  vi.stubGlobal("fetch", fetchMock);
  window.history.replaceState(null, "", `/invitation#${TOKEN}`);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** Every request the page made: none carries the token in its URL. */
function noTokenInUrls() {
  for (const [url] of fetchMock.mock.calls) expect(String(url)).not.toContain(TOKEN);
}

function complete(password = "chosen password 1", repeat = password) {
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: password } });
  fireEvent.change(screen.getByLabelText("Repeat the password"), { target: { value: repeat } });
  fireEvent.change(screen.getByLabelText("Authenticator code"), { target: { value: "654321" } });
  fireEvent.click(screen.getByRole("button", { name: "Complete the invitation" }));
}

test("reads the token from the fragment, removes it, shows the QR code and the URI, completes", async () => {
  answer(200, { email: "ana@dash011.test", provisioning_uri: URI });
  render(<InvitationPage />);

  expect(window.location.hash).toBe("");
  expect(window.location.href).not.toContain(TOKEN);
  expect(window.location.pathname).toBe("/invitation");

  expect(await screen.findByText("ana@dash011.test")).toBeInTheDocument();
  const [url, init] = fetchMock.mock.calls[0] ?? [];
  expect(url).toBe("/api/invitations/totp");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(String(init?.body))).toEqual({ token: TOKEN });

  expect(screen.getByText(URI)).toBeInTheDocument();
  const qr = screen.getByRole("img", { name: "QR code of the authenticator's setup" });
  // The QR code is the URI's: one dark square for each dark module of its symbol.
  const modules = encodeQR(URI, "raw", { ecc: "medium", border: 4 });
  const dark = modules.flat().filter(Boolean).length;
  const path = qr.querySelector("path")?.getAttribute("d") ?? "";
  expect(path.match(/h1v1h-1z/g)?.length).toBe(dark);
  expect(qr.getAttribute("viewBox")).toBe(`0 0 ${modules.length} ${modules.length}`);

  answer(204);
  complete();
  expect(await screen.findByText("Your account is ready.")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Sign in" })).toHaveAttribute("href", "/login");
  const [doneUrl, doneInit] = fetchMock.mock.calls[1] ?? [];
  expect(doneUrl).toBe("/api/invitations/complete");
  expect(JSON.parse(String(doneInit?.body))).toEqual({
    token: TOKEN,
    password: "chosen password 1",
    totp_code: "654321",
  });
  // The secret's text and its QR code are gone once the invitation is completed.
  expect(screen.queryByText(URI)).toBeNull();
  expect(screen.queryByRole("img")).toBeNull();

  noTokenInUrls();
  expect(setItem).not.toHaveBeenCalled();
});

test("asks for one secret under React's strict mode", async () => {
  answer(200, { email: "ana@dash011.test", provisioning_uri: URI });
  render(
    <StrictMode>
      <InvitationPage />
    </StrictMode>,
  );
  expect(await screen.findByText("ana@dash011.test")).toBeInTheDocument();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

test("an address with no fragment, as after a reload, asks for nothing", async () => {
  window.history.replaceState(null, "", "/invitation");
  render(<InvitationPage />);
  expect(await screen.findByText(/Open the invitation's link again/)).toBeInTheDocument();
  expect(fetchMock).not.toHaveBeenCalled();
});

test("a fragment that is not a token asks for nothing", async () => {
  window.history.replaceState(null, "", "/invitation#a%20b");
  render(<InvitationPage />);
  expect(await screen.findByText(/Open the invitation's link again/)).toBeInTheDocument();
  expect(fetchMock).not.toHaveBeenCalled();
  expect(window.location.hash).toBe("");
});

test.each([
  [401, "This invitation is not valid: it expired, was used or was withdrawn"],
  [429, "Too many attempts; try again later"],
  [503, "The service is unavailable"],
])("a refused request for the secret (%i) shows no form", async (status, message) => {
  answer(status, { error: "x" });
  render(<InvitationPage />);
  expect(await screen.findByRole("alert")).toHaveTextContent(message);
  expect(screen.queryByLabelText("Password")).toBeNull();
  noTokenInUrls();
});

test.each([
  [400, "Check the fields: a password has 12 to 128 characters, a letter and a digit"],
  [401, "The code was not accepted, or the invitation is no longer valid"],
  [429, "Too many attempts; try again later"],
])("a refused completion (%i) stays and says why", async (status, message) => {
  answer(200, { email: "ana@dash011.test", provisioning_uri: URI });
  render(<InvitationPage />);
  await screen.findByText("ana@dash011.test");
  answer(status, { error: "x" });
  complete();
  expect(await screen.findByRole("alert")).toHaveTextContent(message);
  expect(screen.getByLabelText("Password")).toHaveValue("");
  expect(screen.getByLabelText("Authenticator code")).toHaveValue("");
  expect(screen.getByText(URI)).toBeInTheDocument();
  noTokenInUrls();
});

test("passwords that differ are refused before any request", async () => {
  answer(200, { email: "ana@dash011.test", provisioning_uri: URI });
  render(<InvitationPage />);
  await screen.findByText("ana@dash011.test");
  complete("chosen password 1", "chosen password 2");
  expect(await screen.findByRole("alert")).toHaveTextContent("The passwords differ");
  await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
});
