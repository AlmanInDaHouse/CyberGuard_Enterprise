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
  /** SPEC-025 §Data contracts §1. */
  enrolled_by_token_id: string | null;
  hostname: string | null;
  platform: string | null;
  agent_version: string | null;
  revoked_at: Date | null;
  revocation_reason: string | null;
  process_started_at: Date | null;
  previous_process_started_at: Date | null;
  duplicate_signal: "superseded_certificate" | "interleaved_process" | null;
  duplicate_signal_at: Date | null;
  pending_cert_pem: string | null;
}

export type TokenState = "issued" | "consumed" | "expired";

/** SPEC-025 §Data contracts §1: the token's SHA-256 alone, under its identifier. */
export interface EnrollmentTokensTable {
  token_id: string;
  token_sha256: Buffer;
  org_id: Generated<string>;
  scope: Generated<string>;
  state: Generated<TokenState>;
  issued_at: Generated<Date>;
  expires_at: Date;
  consumed_at: Date | null;
  max_uses: Generated<number>;
  uses: Generated<number>;
  withdrawn_at: Date | null;
}
