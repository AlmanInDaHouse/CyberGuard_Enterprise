import type { ApiClient } from "./client";
import { NotImplementedError } from "./errors";
import type { ReadResult } from "./result";
import type { IncidentDetail, IncidentListItem, Page } from "./types";

// SPEC-009 §Operational §3 — the dashboard's server-side data-access layer.
//
// HARNESS-FIRST RED (PART 2/2): these are STUBS. The GREEN gate implements them as:
// call `client.get(...)` with the forwarded `cgsess` cookie; map a 200 to
// `{ ok: true, data }`, a 401 (no/invalid/revoked session) to
// `{ ok: false, reason: "unauthenticated" }` (the RSC page redirects to /login).
// They throw NotImplementedError so dash_ac_001 fails on ABSENT LOGIC — never on a
// broken harness (the harness builds the real read-API in-process and seeds data).

export function getIncidents(
  _client: ApiClient,
  _cookieHeader: string | undefined,
): Promise<ReadResult<Page<IncidentListItem>>> {
  throw new NotImplementedError("getIncidents");
}

export function getIncidentDetail(
  _client: ApiClient,
  _cookieHeader: string | undefined,
  _id: string,
): Promise<ReadResult<IncidentDetail>> {
  throw new NotImplementedError("getIncidentDetail");
}
