/**
 * SPEC-008 §Data contracts §2 — append-only audit of every auth event + role
 * change (threat model § cg-api Repudiation). INSERT-only path (no update/delete).
 *
 * SPEC-008 Amendment 2026-10-15 item 6: `userId` is the acting principal, `targetId`
 * the user acted on (the incident, on a forensic row), `correlationId` the request's id, and `detail` holds only role
 * and status values; on the forensic rows (SPEC-009's third Amendment 2026-10-15), the
 * count of events signed and the hashes that identify them; on an `access.denied` row
 * (SPEC-008 Amendment 2026-10-16), the refusal's reason and route pattern; never an
 * address, a password, a code or a token.
 */
export interface AuditEvent {
  userId?: string | null;
  orgId?: string;
  action: string;
  targetId?: string | null;
  outcome: "ok" | "denied" | "error";
  sourceIp?: string | null;
  correlationId?: string | null;
  detail?: Record<string, string> | null;
}

/** What an audit row is written through: a pool, or a client inside a transaction. */
export interface Queryable {
  query(text: string, values?: unknown[]): Promise<unknown>;
}

export async function appendAudit(db: Queryable, e: AuditEvent): Promise<void> {
  await db.query(
    `INSERT INTO audit_log
       (user_id, org_id, action, target_id, outcome, source_ip, correlation_id, detail)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
    [
      e.userId ?? null,
      e.orgId ?? "default",
      e.action,
      e.targetId ?? null,
      e.outcome,
      e.sourceIp ?? null,
      e.correlationId ?? null,
      e.detail ? JSON.stringify(e.detail) : null,
    ],
  );
}
