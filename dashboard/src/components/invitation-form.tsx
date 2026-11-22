"use client";

import { QrCode } from "@/components/qr-code";
import { Button } from "@/components/ui/button";
import Link from "next/link";
import { type FormEvent, useEffect, useRef, useState } from "react";

/** An invitation as the api writes it: base64url, bounded as the api's schema bounds it. */
const INVITATION = /^[A-Za-z0-9_-]{1,256}$/;

const UNAVAILABLE = "The service is unavailable";
const TOO_MANY = "Too many attempts; try again later";

const SECRET_MESSAGES: Record<number, string> = {
  401: "This invitation is not valid: it expired, was used or was withdrawn. Ask an admin for a new one.",
  429: TOO_MANY,
};

const COMPLETE_MESSAGES: Record<number, string> = {
  400: "Check the fields: a password has 12 to 128 characters, a letter and a digit",
  401: "The code was not accepted, or the invitation is no longer valid",
  429: TOO_MANY,
};

type Setup = { email: string; provisioning_uri: string };

type State =
  | { step: "reading" }
  | { step: "no-invitation" }
  | { step: "refused"; message: string }
  | { step: "setup"; token: string; setup: Setup }
  | { step: "done" };

const inputClass = "rounded-md border border-slate-300 px-3 py-2";

/** POST a JSON body to one of the dashboard's invitation handlers; `null` when it did not
 *  answer. */
async function post(path: string, body: unknown): Promise<Response | null> {
  try {
    return await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    return null;
  }
}

/**
 * `/invitation` (SPEC-026 §Operational §3 and §9, dash_ac_011). The invitation arrives in the
 * address's fragment, which reaches no server; the page reads it once, removes it from the
 * address bar, and keeps it in memory alone. It asks the dashboard's `/api/invitations/totp`
 * for a secret, with the invitation in the body, shows the provisioning URI as a QR code and as
 * text, and completes the invitation with the chosen password and a code of that secret. A
 * reload, whose address holds no invitation, asks for nothing; opening the link again shows a
 * new secret (Decision record 6).
 */
export function InvitationForm() {
  const [state, setState] = useState<State>({ step: "reading" });
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [code, setCode] = useState("");
  const [message, setMessage] = useState<string | undefined>();
  const [pending, setPending] = useState(false);
  // React's strict mode runs an effect twice: the invitation is read, and a secret asked for,
  // once.
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    const fragment = window.location.hash.slice(1);
    window.history.replaceState(null, "", window.location.pathname + window.location.search);
    if (!INVITATION.test(fragment)) {
      setState({ step: "no-invitation" });
      return;
    }
    const token = fragment;
    void (async () => {
      const res = await post("/api/invitations/totp", { token });
      if (res?.status === 200) {
        const setup = (await res.json()) as Setup;
        setState({ step: "setup", token, setup });
        return;
      }
      setState({
        step: "refused",
        message: (res && SECRET_MESSAGES[res.status]) ?? UNAVAILABLE,
      });
    })();
  }, []);

  async function onSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (state.step !== "setup") return;
    setMessage(undefined);
    if (password !== repeat) {
      setMessage("The passwords differ");
      return;
    }
    setPending(true);
    const res = await post("/api/invitations/complete", {
      token: state.token,
      password,
      totp_code: code,
    });
    setPassword("");
    setRepeat("");
    setCode("");
    setPending(false);
    if (res?.status === 204) {
      setState({ step: "done" });
      return;
    }
    setMessage((res && COMPLETE_MESSAGES[res.status]) ?? UNAVAILABLE);
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-4 p-6">
      <h1 className="text-xl font-semibold">CyberGuard SOC — your invitation</h1>
      {state.step === "reading" && (
        <p className="text-sm text-slate-700">Reading the invitation…</p>
      )}
      {state.step === "no-invitation" && (
        <p className="text-sm text-slate-700">
          This page holds no invitation. Open the invitation's link again, as the admin gave it to
          you.
        </p>
      )}
      {state.step === "refused" && (
        <p role="alert" className="text-sm text-red-700">
          {state.message}
        </p>
      )}
      {state.step === "done" && (
        <p className="text-sm text-slate-700">
          Your account is ready.{" "}
          <Link href="/login" className="underline">
            Sign in
          </Link>
        </p>
      )}
      {state.step === "setup" && (
        <>
          <p className="text-sm text-slate-700">
            The account: <span className="font-medium">{state.setup.email}</span>
          </p>
          <p className="text-sm text-slate-700">
            Scan the code with your authenticator app, or enter the text below in it. Opening the
            link again shows a new code, which replaces this one.
          </p>
          <QrCode
            text={state.setup.provisioning_uri}
            label="QR code of the authenticator's setup"
          />
          <code className="break-all rounded-md bg-slate-100 p-2 text-xs">
            {state.setup.provisioning_uri}
          </code>
          <form className="flex flex-col gap-3" onSubmit={onSubmit}>
            <label className="flex flex-col gap-1 text-sm" htmlFor="invitation-password">
              Password
              <input
                id="invitation-password"
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className={inputClass}
              />
            </label>
            <label className="flex flex-col gap-1 text-sm" htmlFor="invitation-repeat">
              Repeat the password
              <input
                id="invitation-repeat"
                type="password"
                autoComplete="new-password"
                value={repeat}
                onChange={(e) => setRepeat(e.target.value)}
                className={inputClass}
              />
            </label>
            <label className="flex flex-col gap-1 text-sm" htmlFor="invitation-totp">
              Authenticator code
              <input
                id="invitation-totp"
                inputMode="numeric"
                autoComplete="one-time-code"
                value={code}
                onChange={(e) => setCode(e.target.value)}
                className={inputClass}
              />
            </label>
            {message && (
              <p role="alert" className="text-sm text-red-700">
                {message}
              </p>
            )}
            <Button type="submit" disabled={pending}>
              Complete the invitation
            </Button>
          </form>
        </>
      )}
    </main>
  );
}
