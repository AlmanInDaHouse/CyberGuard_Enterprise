import type { Pool } from "pg";

/**
 * SPEC-008 §Data contracts §2 — append-only audit of every auth event + role
 * change (threat model § cg-api Repudiation). INSERT-only path (no update/delete).
 */
export interface AuditEvent {
  userId?: string | null;
  orgId?: string;
  action: string;
  targetId?: string | null;
  outcome: "ok" | "denied" | "error";
  sourceIp?: string | null;
  correlationId?: string | null;
}

export async function appendAudit(pool: Pool, e: AuditEvent): Promise<void> {
  await pool.query(
    `INSERT INTO audit_log (user_id, org_id, action, target_id, outcome, source_ip, correlation_id)
     VALUES ($1, $2, $3, $4, $5, $6, $7)`,
    [
      e.userId ?? null,
      e.orgId ?? "default",
      e.action,
      e.targetId ?? null,
      e.outcome,
      e.sourceIp ?? null,
      e.correlationId ?? null,
    ],
  );
}
