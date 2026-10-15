import { IncidentDetailView } from "@/components/incident-detail";
import { SignOutButton } from "@/components/sign-out-button";
import { apiBaseUrl, httpApiClient } from "@/lib/api/client";
import { getIncidentDetail } from "@/lib/api/incidents";
import { sessionCookieHeader } from "@/lib/auth/cookies";
import { cookies } from "next/headers";
import { notFound, redirect } from "next/navigation";

export const dynamic = "force-dynamic";

/** Incident detail — the teachable view (SPEC-009 §Operational §3). Server-side:
 *  forward `cgsess`, and no other cookie, to the read-API (its second Amendment
 *  2026-10-15, item 6); an unauthenticated result redirects to login,
 *  a not-found (incident absent / outside the session's org) renders the 404. */
export default async function IncidentDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const cookieStore = await cookies();
  const result = await getIncidentDetail(
    httpApiClient(apiBaseUrl()),
    sessionCookieHeader(cookieStore),
    id,
  );
  if (!result.ok) {
    if (result.reason === "unauthenticated") redirect("/login");
    notFound();
  }

  return (
    <main className="mx-auto max-w-5xl p-6">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-xl font-semibold">Incident {id}</h1>
        <SignOutButton />
      </div>
      <IncidentDetailView incident={result.data} />
    </main>
  );
}
