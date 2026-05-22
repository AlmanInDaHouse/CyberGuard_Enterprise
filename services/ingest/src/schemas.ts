import { z } from "zod";

/** SPEC-002 enrollment request (SPEC-004 FR-003). */
export const EnrollRequestSchema = z.object({
  envelope_version: z.string().min(1),
  enrollment_token: z.string().min(1),
  // base64url-unpadded raw Ed25519 public key — must decode to 32 bytes.
  agent_pubkey: z.string().refine((v) => decodeB64url(v)?.length === 32, {
    message: "agent_pubkey must be a base64url-encoded 32-byte key",
  }),
  agent_hostname: z.string().min(1),
  agent_platform: z.string().min(1),
  agent_version: z.string().min(1),
});
export type EnrollRequest = z.infer<typeof EnrollRequestSchema>;

const AgentBlockSchema = z.object({
  agent_id: z.string().min(1),
  agent_version: z.string().min(1),
  agent_platform: z.string().min(1),
  agent_hostname: z.string().min(1),
});

const InnerEnvelopeSchema = z.object({
  envelope_version: z.string().min(1),
  agent: AgentBlockSchema,
  sequence_number: z.number().int().nonnegative(),
  sent_at: z.string().min(1),
  status: z.enum(["online", "going_offline"]),
  uptime_seconds: z.number().int().nonnegative(),
});

/** SPEC-003 outer signed envelope (SPEC-004 FR-009). */
export const OuterEnvelopeSchema = z.object({
  outer_envelope_version: z.string().min(1),
  agent_id: z.string().min(1),
  sequence_number: z.number().int().nonnegative(),
  nonce: z.string().min(1),
  sent_at: z.string().min(1),
  body: InnerEnvelopeSchema,
  signature: z.string().min(1),
});
export type OuterEnvelope = z.infer<typeof OuterEnvelopeSchema>;

export function decodeB64url(v: string): Buffer | null {
  if (!/^[A-Za-z0-9_-]+$/.test(v)) {
    return null;
  }
  try {
    return Buffer.from(v, "base64url");
  } catch {
    return null;
  }
}
