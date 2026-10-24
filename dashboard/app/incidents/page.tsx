import { IncidentsTable } from "@/components/incidents-table";
import { SignOutButton } from "@/components/sign-out-button";
import { apiBaseUrl, httpApiClient } from "@/lib/api/client";
import { getIncidents } from "@/lib/api/incidents";
import { sessionCookieHeader } from "@/lib/auth/cookies";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

// Reading the session cookie + the read-API makes this request-time dynamic; never
// statically prerendered (so `next build` compiles without executing the read).
export const dynamic = "force-dynamic";

/** Incidents list (SPEC-009 §Operational §3). Server-side: forward `cgsess`, and no
 *  other cookie, to the read-API (its second Amendment 2026-10-15, item 6); an
 *  unauthenticated result redirects to login. */
export default async function IncidentsPage() {
  const cookieStore = await cookies();
  const result = await getIncidents(httpApiClient(apiBaseUrl()), sessionCookieHeader(cookieStore));
  if (!result.ok) redirect("/login");

  return (
    <main className="mx-auto max-w-5xl p-6">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-xl font-semibold">Incidents</h1>
        <SignOutButton />
      </div>
      <IncidentsTable items={result.data.items} />
    </main>
  );
}
