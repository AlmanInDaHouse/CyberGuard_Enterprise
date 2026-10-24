/**
 * SPEC-023 §Operational §2: the dashboard checks `API_BASE_URL` when its server starts,
 * and stops with one line naming it when it is malformed.
 */
export async function register(): Promise<void> {
  if (process.env.NEXT_RUNTIME !== "nodejs") return;
  const { apiBaseUrlProblem } = await import("./src/lib/api/client");
  const reason = apiBaseUrlProblem(process.env);
  if (reason !== null) {
    process.stderr.write(`cg-dashboard: fatal: invalid configuration: API_BASE_URL: ${reason}\n`);
    process.exit(1);
  }
}
