/** The result of a server-side read against the read-API. `unauthenticated`
 *  (no/invalid/revoked session, surfaced as a read-API 401) is the signal the RSC
 *  page turns into a `redirect("/login")` — keeping the redirect decision in the
 *  page (Next runtime) and the auth→read mapping testable in plain code. */
export type ReadResult<T> = { ok: true; data: T } | { ok: false; reason: "unauthenticated" };
