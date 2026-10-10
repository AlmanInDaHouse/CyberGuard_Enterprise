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

/** SPEC-005 CGES Process sub-object (event.process) per ADR-0011 + γ DDL. */
const CgesProcessSchema = z.object({
  pid: z.number().int().nonnegative(),
  uid: z.string().min(1),
  name: z.string().min(1),
  /** Always present; null on AC-004 cache-miss; string-encoded nanos otherwise. */
  created_time: z.string().nullable(),
  /** Absent on AC-005 Launch path; integer on Terminate. */
  exit_code: z.number().int().optional(),
  /** Null on AC-007 PPID unresolvable; integer otherwise. */
  parent_pid: z.number().int().nonnegative().nullable(),
  command_line: z.string(),
  subject_user_sid: z.string(),
  image_file_name: z.string(),
});

/** SPEC-005 CGES Process Activity event shape (envelope.events[] element). */
const CgesProcessActivitySchema = z.object({
  event_id: z.string().min(1),
  /** Strict literal per OCSF Process Activity + ADR-0006 + ADR-0011. */
  class_uid: z.literal(1007),
  /** Launch=1, Terminate=2 per ADR-0011 §3 CGES wire allow-list. */
  activity_id: z.union([z.literal(1), z.literal(2)]),
  process: CgesProcessSchema,
  /** String-encoded Unix nanoseconds UTC; avoids IEEE 754 precision loss. */
  time: z.string(),
});

/** SPEC-019 network endpoint: a valid IP address in text and a TCP port. */
const NetworkEndpointSchema = z.object({
  ip: z.string().ip(),
  port: z.number().int().min(0).max(65535),
});

/**
 * SPEC-019 CGES Network Activity event shape (envelope.events[] element): a TCP
 * connection opened (ADR-0018 §2–§6). `src_endpoint` is the initiator and
 * `dst_endpoint` the acceptor (ADR-0018 §4); `actor.process.uid` is present only
 * when the agent knew the process's creation time.
 */
const CgesNetworkActivitySchema = z.object({
  /** A UUID: the agent generates version 7; the version is not checked (SPEC-019 §Operational §6). */
  event_id: z.string().uuid(),
  class_uid: z.literal(4001),
  /** Open, the only activity the agent emits (ADR-0018 §2). */
  activity_id: z.literal(1),
  /** String-encoded Unix nanoseconds UTC, as for class 1007 (ADR-0018 §6). */
  time: z.string(),
  src_endpoint: NetworkEndpointSchema,
  dst_endpoint: NetworkEndpointSchema,
  connection_info: z.object({
    protocol_name: z.literal("tcp"),
    direction: z.enum(["outbound", "inbound"]),
  }),
  actor: z.object({
    process: z.object({
      /** Fits the UInt32 `process_pid` column (SPEC-019 Amendment 2026-10-10). */
      pid: z.number().int().min(0).max(4_294_967_295),
      uid: z.string().min(1).optional(),
    }),
  }),
});

/** An events[] element: one of the two shapes, told apart by class_uid (SPEC-019). */
const CgesEventSchema = z.discriminatedUnion("class_uid", [
  CgesProcessActivitySchema,
  CgesNetworkActivitySchema,
]);
export type CgesEvent = z.infer<typeof CgesEventSchema>;

const InnerEnvelopeSchema = z.object({
  envelope_version: z.string().min(1),
  agent: AgentBlockSchema,
  sequence_number: z.number().int().nonnegative(),
  sent_at: z.string().min(1),
  status: z.enum(["online", "going_offline"]),
  uptime_seconds: z.number().int().nonnegative(),
  /**
   * SPEC-005 events extension. Optional + default([]) for backward
   * compat with SPEC-001/002/003 envelopes that do not carry events
   * per SPEC-001 amendment 2026-05-23 narrowing-not-overriding semantics.
   * Each element is Process Activity or Network Activity (SPEC-019).
   */
  events: z.array(CgesEventSchema).optional().default([]),
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
