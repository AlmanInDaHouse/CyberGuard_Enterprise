"use client";

import { Button } from "@/components/ui/button";
import { type FormEvent, useState } from "react";

/**
 * Login view — email + password + TOTP → the dashboard-origin auth proxy
 * (SPEC-009 §Operational §3 / §Data contracts §4).
 *
 * HARNESS-FIRST RED (PART 2/2): presentational shell only — the submit→proxy wiring
 * lands at the GREEN gate (the auth proxy is exercised end-to-end by the demo path
 * SC-READ-001; it is not one of the dash_ac component/data-access ACs).
 */
export default function LoginPage() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [totp, setTotp] = useState("");

  function onSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    // GREEN gate: POST to the dashboard-origin auth proxy, then redirect to /incidents.
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-sm flex-col justify-center gap-4 p-6">
      <h1 className="text-xl font-semibold">CyberGuard SOC — sign in</h1>
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
        <Button type="submit">Sign in</Button>
      </form>
    </main>
  );
}
