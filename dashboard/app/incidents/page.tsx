import { IncidentsTable } from "@/components/incidents-table";
import { httpApiClient } from "@/lib/api/client";
import { getIncidents } from "@/lib/api/incidents";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

// Reading the cgsess cookie + the read-API makes this request-time dynamic; never
// statically prerendered (so `next build` compiles without executing the read).
export const dynamic = "force-dynamic";

const API_BASE_URL = process.env.API_BASE_URL ?? "http://localhost:8080";

/** Incidents list (SPEC-009 §Operational §3). Server-side: forward `cgsess` to the
 *  read-API; an unauthenticated result redirects to login. */
export default async function IncidentsPage() {
  const cookieStore = await cookies();
  const result = await getIncidents(httpApiClient(API_BASE_URL), cookieStore.toString());
  if (!result.ok) redirect("/login");

  return (
    <main className="mx-auto max-w-5xl p-6">
      <h1 className="mb-4 text-xl font-semibold">Incidents</h1>
      <IncidentsTable items={result.data.items} />
    </main>
  );
}
