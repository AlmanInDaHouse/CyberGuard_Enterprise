import type { Generated } from "kysely";

/** Kysely database interface for the SPEC-004 Postgres schema. */
export interface Database {
  ca: CaTable;
  agents: AgentsTable;
  enrollment_tokens: EnrollmentTokensTable;
}

export interface CaTable {
  id: Generated<number>;
  cert_pem: string;
  private_key: Buffer;
  created_at: Generated<Date>;
}

export interface AgentsTable {
  agent_id: string;
  org_id: Generated<string>;
  pubkey: Buffer;
  cert_pem: string;
  enrolled_at: Generated<Date>;
  expires_at: Date;
  last_seen: Date | null;
}

export type TokenState = "issued" | "consumed" | "expired";

export interface EnrollmentTokensTable {
  token: string;
  org_id: Generated<string>;
  scope: Generated<string>;
  state: Generated<TokenState>;
  issued_at: Generated<Date>;
  expires_at: Date;
  consumed_at: Date | null;
  consumed_by_agent_id: string | null;
}
