import { IncidentDetailView } from "@/components/incident-detail";
import { httpApiClient } from "@/lib/api/client";
import { getIncidentDetail } from "@/lib/api/incidents";
import { cookies } from "next/headers";
import { notFound, redirect } from "next/navigation";

export const dynamic = "force-dynamic";

const API_BASE_URL = process.env.API_BASE_URL ?? "http://localhost:8080";

/** Incident detail — the teachable view (SPEC-009 §Operational §3). Server-side:
 *  forward `cgsess` to the read-API; an unauthenticated result redirects to login,
 *  a not-found (incident absent / outside the session's org) renders the 404. */
export default async function IncidentDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const cookieStore = await cookies();
  const result = await getIncidentDetail(httpApiClient(API_BASE_URL), cookieStore.toString(), id);
  if (!result.ok) {
    if (result.reason === "unauthenticated") redirect("/login");
    notFound();
  }

  return (
    <main className="mx-auto max-w-5xl p-6">
      <h1 className="mb-4 text-xl font-semibold">Incident {id}</h1>
      <IncidentDetailView incident={result.data} />
    </main>
  );
}
