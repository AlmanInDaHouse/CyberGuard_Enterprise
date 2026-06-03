/** The result of a server-side read against the read-API. `unauthenticated`
 *  (no/invalid/revoked session, surfaced as a read-API 401) is the signal the RSC
 *  page turns into a `redirect("/login")`; `not_found` (a read-API 404, e.g. an
 *  incident id outside the session's org — no existence oracle, SPEC-009 §Security)
 *  the page turns into `notFound()`. Keeping these decisions in the page (Next
 *  runtime) leaves the auth→read mapping testable in plain code. */
export type ReadResult<T> =
  | { ok: true; data: T }
  | { ok: false; reason: "unauthenticated" | "not_found" };
