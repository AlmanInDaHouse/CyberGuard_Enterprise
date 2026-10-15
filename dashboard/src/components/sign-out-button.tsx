"use client";

import { Button } from "@/components/ui/button";
import { useRouter } from "next/navigation";
import { useState } from "react";

/**
 * Sign-out, on the incident views (SPEC-009's second Amendment 2026-10-15, item 7):
 * posts to the dashboard's `/api/auth/logout`. On `200` it navigates to the login view;
 * on `502` or `503`, which cleared the cookies without the api ending the session, to
 * the login view with its notice; on `403` it stays and says the request was refused.
 * When it navigates it also drops the router's cached views (`router.refresh()`), so
 * that going back shows no incident without asking the server.
 */
export function SignOutButton() {
  const router = useRouter();
  const [message, setMessage] = useState<string | undefined>();
  const [pending, setPending] = useState(false);

  async function onClick() {
    setPending(true);
    setMessage(undefined);
    let status = 0;
    try {
      status = (await fetch("/api/auth/logout", { method: "POST" })).status;
    } catch {
      status = 0;
    }
    if (status === 200) {
      router.push("/login");
      router.refresh();
      return;
    }
    if (status === 502 || status === 503) {
      router.push("/login?notice=signed-out");
      router.refresh();
      return;
    }
    setMessage(status === 403 ? "The request was refused" : "The service is unavailable");
    setPending(false);
  }

  return (
    <div className="flex items-center gap-3">
      {message && (
        <p role="alert" className="text-sm text-red-700">
          {message}
        </p>
      )}
      <Button onClick={onClick} disabled={pending}>
        Sign out
      </Button>
    </div>
  );
}
