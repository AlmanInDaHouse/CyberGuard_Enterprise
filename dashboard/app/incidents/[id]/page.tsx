import { AppShell } from "@/components/app-nav";
import { IncidentDetailView } from "@/components/incident-detail";
import { getIncidentDetail } from "@/lib/api/incidents";
import { signedIn } from "@/lib/signed-in";
import { notFound, redirect } from "next/navigation";

export const dynamic = "force-dynamic";

/** Incident detail — the teachable view (SPEC-009 §Operational §3). Server-side: the
 *  session's user first, for the navigation (SPEC-026 §Operational §9), then the incident,
 *  each read with `cgsess` and no other cookie (SPEC-009's second Amendment 2026-10-15,
 *  item 6); an unauthenticated result redirects to login, a not-found (incident absent /
 *  outside the session's org) renders the 404. */
export default async function IncidentDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const { user, client, cookieHeader } = await signedIn();
  const result = await getIncidentDetail(client, cookieHeader, id);
  if (!result.ok) {
    if (result.reason === "unauthenticated") redirect("/login");
    notFound();
  }

  return (
    <AppShell user={user}>
      <h1 className="mb-4 text-xl font-semibold">Incident {id}</h1>
      <IncidentDetailView incident={result.data} />
    </AppShell>
  );
}
