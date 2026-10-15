"use client";

import { Button } from "@/components/ui/button";
import { useRouter } from "next/navigation";
import { type FormEvent, useEffect, useState } from "react";

/** What the view says for each answer of `/api/auth/login` (SPEC-009's second
 *  Amendment 2026-10-15, item 7). Any other answer, or none, is the service's. */
const MESSAGES: Record<number, string> = {
  400: "Check the fields",
  401: "Sign-in failed",
  403: "The request was refused",
  429: "Too many attempts; try again later",
};
const UNAVAILABLE = "The service is unavailable";

const SIGNED_OUT_NOTICE =
  "This browser is signed out; the server may keep the session until it expires.";

/**
 * Login view — email, password and TOTP code, posted as JSON to the dashboard's own
 * `/api/auth/login`, which sets the session cookies (item 3). On success it navigates to
 * the incidents; otherwise it shows a message by status and keeps the email address.
 */
export function LoginForm({ signedOutNotice = false }: { signedOutNotice?: boolean }) {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [totp, setTotp] = useState("");
  const [message, setMessage] = useState<string | undefined>();
  const [pending, setPending] = useState(false);

  // Going back from here shows no incident the router cached for an earlier session.
  useEffect(() => {
    router.refresh();
  }, [router]);

  async function onSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setPending(true);
    setMessage(undefined);
    let status = 0;
    try {
      const res = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email, password, totp_code: totp }),
      });
      status = res.status;
    } catch {
      status = 0;
    }
    if (status === 200) {
      router.push("/incidents");
      return;
    }
    setPassword("");
    setTotp("");
    setMessage(MESSAGES[status] ?? UNAVAILABLE);
    setPending(false);
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-sm flex-col justify-center gap-4 p-6">
      <h1 className="text-xl font-semibold">CyberGuard SOC — sign in</h1>
      {signedOutNotice && <output className="text-sm text-slate-700">{SIGNED_OUT_NOTICE}</output>}
      <form className="flex flex-col gap-3" onSubmit={onSubmit}>
        <label className="flex flex-col gap-1 text-sm" htmlFor="email">
          Email
          <input
            id="email"
            type="email"
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-2"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm" htmlFor="password">
          Password
          <input
            id="password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-2"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm" htmlFor="totp">
          Authenticator code
          <input
            id="totp"
            inputMode="numeric"
            autoComplete="one-time-code"
            value={totp}
            onChange={(e) => setTotp(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-2"
          />
        </label>
        {message && (
          <p role="alert" className="text-sm text-red-700">
            {message}
          </p>
        )}
        <Button type="submit" disabled={pending}>
          Sign in
        </Button>
      </form>
    </main>
  );
}
