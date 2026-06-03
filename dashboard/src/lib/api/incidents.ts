import type { ApiClient } from "./client";
import type { ReadResult } from "./result";
import type { IncidentDetail, IncidentListItem, Page } from "./types";

// SPEC-009 §Operational §3 — the dashboard's server-side data-access layer.
//
// Consumes the REAL read-API (services/api) — over HTTP in production (forwarding
// the server-side `cgsess` cookie), and in-process via app.inject under test
// (dash_ac_001). The auth→read mapping is pure code: a 200 carries the data; a 401
// (no/invalid/revoked session) becomes `unauthenticated` (the RSC redirects to
// /login); a 404 on a detail read becomes `not_found` (the RSC calls notFound()).
// No client-trusted role/org — the read-API derives both from the session
// (SPEC-009 §Operational §2); this layer only forwards the cookie.

export async function getIncidents(
  client: ApiClient,
  cookieHeader: string | undefined,
): Promise<ReadResult<Page<IncidentListItem>>> {
  const res = await client.get("/v1/incidents", cookieHeader);
  if (res.status === 401) return { ok: false, reason: "unauthenticated" };
  if (res.status !== 200) throw new Error(`read-API GET /v1/incidents: ${res.status}`);
  return { ok: true, data: res.body as Page<IncidentListItem> };
}

export async function getIncidentDetail(
  client: ApiClient,
  cookieHeader: string | undefined,
  id: string,
): Promise<ReadResult<IncidentDetail>> {
  const res = await client.get(`/v1/incidents/${encodeURIComponent(id)}`, cookieHeader);
  if (res.status === 401) return { ok: false, reason: "unauthenticated" };
  if (res.status === 404) return { ok: false, reason: "not_found" };
  if (res.status !== 200) throw new Error(`read-API GET /v1/incidents/:id: ${res.status}`);
  return { ok: true, data: res.body as IncidentDetail };
}
