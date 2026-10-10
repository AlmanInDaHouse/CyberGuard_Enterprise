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

/** The members every Authentication (3002) element has (SPEC-020 §Data contracts). */
const CgesAuthenticationBase = {
  /** A UUID: the agent generates version 7; the version is not checked. */
  event_id: z.string().uuid(),
  class_uid: z.literal(3002),
  category_uid: z.literal(3),
  /** Logon, the only activity the agent emits (ADR-0019 §2). */
  activity_id: z.literal(1),
  /** String-encoded Unix nanoseconds UTC, as for classes 1007 and 4001 (ADR-0019 §4). */
  time: z.string(),
  /** `name` and `domain` are as Windows wrote them, `-`, or `<withheld>` (SPEC-020 §Operational §3). */
  user: z.object({
    uid: z.string().min(1),
    name: z.string().min(1),
    domain: z.string().min(1),
  }),
  logon_type_id: z.number().int().min(0).max(99),
  auth_protocol: z.string().min(1),
  auth_protocol_id: z.union([z.literal(0), z.literal(1), z.literal(2), z.literal(99)]),
  /** Present only when the event has an address; no port (ADR-0019 §4). */
  src_endpoint: z
    .object({
      ip: z.string().ip(),
      hostname: z.string().min(1).optional(),
    })
    .optional(),
};

/** A Windows status code as lowercase hexadecimal text (SPEC-020 §Data contracts). */
const StatusCodeSchema = z.string().regex(/^0x[0-9a-f]{1,8}$/);

/**
 * SPEC-020 Authentication, a logon that succeeded: no failure codes; the elevated
 * token when the source carried it. `z.never().optional()` refuses a member that is
 * present at all.
 */
const CgesLogonSuccessSchema = z.object({
  ...CgesAuthenticationBase,
  status_id: z.literal(1),
  status_code: z.never().optional(),
  status_detail: z.never().optional(),
  cg_elevated_token: z.boolean().optional(),
});

/** SPEC-020 Authentication, a logon that failed: both codes; no elevated token. */
const CgesLogonFailureSchema = z.object({
  ...CgesAuthenticationBase,
  status_id: z.literal(2),
  status_code: StatusCodeSchema,
  status_detail: StatusCodeSchema,
  cg_elevated_token: z.never().optional(),
});

/**
 * An events[] element: one of three classes, told apart by class_uid (SPEC-019,
 * SPEC-020); an Authentication element is one of two shapes, told apart by
 * status_id. Any other class is invalid.
 */
const CgesEventSchema = z.union([
  CgesProcessActivitySchema,
  CgesNetworkActivitySchema,
  CgesLogonSuccessSchema,
  CgesLogonFailureSchema,
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
   * Each element is Process Activity, Network Activity (SPEC-019) or
   * Authentication (SPEC-020).
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
