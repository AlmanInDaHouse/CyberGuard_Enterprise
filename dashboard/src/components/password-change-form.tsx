"use client";

import { Button } from "@/components/ui/button";
import { useRouter } from "next/navigation";
import { type FormEvent, useState } from "react";

/** What the form says for each answer of `/api/account/password`. Any other answer, or none,
 *  is the service's. */
const MESSAGES: Record<number, string> = {
  400: "Check the fields: a new password has 12 to 128 characters, a letter and a digit",
  401: "The current password or the code was not accepted",
  403: "The request was refused",
  429: "Too many attempts; try again later",
};
const UNAVAILABLE = "The service is unavailable";

const inputClass = "rounded-md border border-slate-300 px-3 py-2";

/**
 * `/account`'s password change (SPEC-026 §Operational §9, phase 1): the current password, the
 * new one twice and a code, posted as JSON to the dashboard's `/api/account/password`. The api
 * ends every session of the user and the handler clears both cookies, so on `200` the form goes
 * to `/login` and drops the router's cached views. Otherwise it says why and clears the fields.
 */
export function PasswordChangeForm() {
  const router = useRouter();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const [code, setCode] = useState("");
  const [message, setMessage] = useState<string | undefined>();
  const [pending, setPending] = useState(false);

  function clear() {
    setCurrent("");
    setNext("");
    setRepeat("");
    setCode("");
  }

  async function onSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setMessage(undefined);
    if (next !== repeat) {
      setMessage("The new passwords differ");
      return;
    }
    setPending(true);
    let status = 0;
    try {
      const res = await fetch("/api/account/password", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ currentPassword: current, newPassword: next, totp_code: code }),
      });
      status = res.status;
    } catch {
      status = 0;
    }
    clear();
    if (status === 200) {
      router.push("/login");
      router.refresh();
      return;
    }
    setMessage(MESSAGES[status] ?? UNAVAILABLE);
    setPending(false);
  }

  return (
    <form className="flex max-w-sm flex-col gap-3" onSubmit={onSubmit}>
      <h2 className="text-lg font-semibold">Change password</h2>
      <p className="text-sm text-slate-700">
        Every session of yours ends, this one included; sign in again with the new password.
      </p>
      <label className="flex flex-col gap-1 text-sm" htmlFor="current-password">
        Current password
        <input
          id="current-password"
          type="password"
          autoComplete="current-password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          className={inputClass}
        />
      </label>
      <label className="flex flex-col gap-1 text-sm" htmlFor="new-password">
        New password
        <input
          id="new-password"
          type="password"
          autoComplete="new-password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
          className={inputClass}
        />
      </label>
      <label className="flex flex-col gap-1 text-sm" htmlFor="repeat-password">
        Repeat the new password
        <input
          id="repeat-password"
          type="password"
          autoComplete="new-password"
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
          className={inputClass}
        />
      </label>
      <label className="flex flex-col gap-1 text-sm" htmlFor="password-totp">
        Authenticator code
        <input
          id="password-totp"
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
        Change password
      </Button>
    </form>
  );
}
