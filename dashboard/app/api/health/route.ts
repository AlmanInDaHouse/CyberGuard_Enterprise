/** `GET /api/health` (SPEC-023 §Operational §7): the dashboard's liveness, calling nothing. */
export function GET(): Response {
  return Response.json({ status: "ok" });
}
