import { SignOutButton } from "@/components/sign-out-button";
import type { SessionUser } from "@/lib/api/session";
import Link from "next/link";
import type { ReactNode } from "react";

/** The navigation's entries for a role (SPEC-026 §Operational §1 and §9): every role reads the
 *  incidents and its own account; only an admin reaches the users' page. The role shows or
 *  hides an entry; the api's checks decide. */
export function navEntries(role: string): { href: string; label: string }[] {
  return [
    { href: "/incidents", label: "Incidents" },
    ...(role === "admin" ? [{ href: "/users", label: "Users" }] : []),
    { href: "/account", label: "Account" },
  ];
}

/** The navigation shared by every signed-in page, with the session's user and the sign-out. */
export function AppNav({ user }: { user: SessionUser }) {
  return (
    <header className="border-b border-slate-200 bg-white">
      <div className="mx-auto flex max-w-5xl items-center justify-between gap-4 px-6 py-3">
        <nav aria-label="Main" className="flex items-center gap-4 text-sm">
          <span className="font-semibold">CyberGuard SOC</span>
          {navEntries(user.role).map((entry) => (
            <Link key={entry.href} href={entry.href} className="text-slate-700 hover:underline">
              {entry.label}
            </Link>
          ))}
        </nav>
        <div className="flex items-center gap-3 text-sm text-slate-600">
          <span>{user.email}</span>
          <SignOutButton />
        </div>
      </div>
    </header>
  );
}

/** A signed-in page: the navigation, then the page's content. */
export function AppShell({ user, children }: { user: SessionUser; children: ReactNode }) {
  return (
    <>
      <AppNav user={user} />
      <main className="mx-auto max-w-5xl p-6">{children}</main>
    </>
  );
}
