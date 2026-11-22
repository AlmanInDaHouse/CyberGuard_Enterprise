import { AppShell } from "@/components/app-nav";
import { IncidentsTable } from "@/components/incidents-table";
import { getIncidents } from "@/lib/api/incidents";
import { signedIn } from "@/lib/signed-in";
import { redirect } from "next/navigation";

// Reading the session cookie + the read-API makes this request-time dynamic; never
// statically prerendered (so `next build` compiles without executing the read).
export const dynamic = "force-dynamic";

/** Incidents list (SPEC-009 §Operational §3). Server-side: the session's user first, for the
 *  navigation (SPEC-026 §Operational §9), then the list, each read with `cgsess` and no other
 *  cookie (SPEC-009's second Amendment 2026-10-15, item 6); an unauthenticated result
 *  redirects to login. */
export default async function IncidentsPage() {
  const { user, client, cookieHeader } = await signedIn();
  const result = await getIncidents(client, cookieHeader);
  if (!result.ok) redirect("/login");

  return (
    <AppShell user={user}>
      <h1 className="mb-4 text-xl font-semibold">Incidents</h1>
      <IncidentsTable items={result.data.items} />
    </AppShell>
  );
}
