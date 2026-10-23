import { getIncidentEventTimeline } from "../read/queries.js";
import type { EventTimeline, TimelineEvent } from "../read/types.js";
import type { Services } from "../services.js";
import { fingerprintOf } from "./fingerprint.js";
import { computeChain, toHex } from "./hashchain.js";

// SPEC-012 §Data contracts §3 — the on-demand forensic snapshot export. For an incident's
// SPEC-010 drill timeline, it assembles: the canonicalized evidence (events, total order),
// the chain head `chain_N` (hex), the Ed25519 `root_signature` over `chain_N`, and the
// EMBEDDED forensic public key.
//
// PURE + HTTP-free: it takes the wired services and returns the export object (or null when
// the incident does not exist / is cross-org), so the crypto path is testable without
// Fastify (hashchain_ac_001 / _006).
//
// The chain is NOT recomputed here — it reuses computeChain from hashchain.ts (gate A+B,
// NO reimplementation). This module's only addition over A+B is the SIGNATURE (gate C):
//
//   root_signature = Ed25519_sign(forensic_key, chain_N)   over the 32 bytes of chain_N
//
// For an incident whose drill resolves to ZERO events, N = 0 and chain_N = chain_0 =
// SHA-256("") (computeChain's empty-input return, hashchain.ts:50) — a well-defined
// empty-evidence root (SPEC-012 §Data contracts §2 line 58).
//
// ANTI-PATTERN GUARD (SPEC-012 §Compliance line 107): `forensic_pubkey` is embedded for
// CONVENIENCE, NOT as a trust root. A verifier MUST check `root_signature` against a key it
// trusts OUT-OF-BAND; verifying against the embedded key ALONE is a forbidden anti-pattern
// (a compromised server could re-sign with its own key and embed the matching pubkey, so the
// signature would "verify" against itself). The out-of-band anchor is the key's fingerprint,
// recorded outside CyberGuard at installation (ADR-0016 Amendment 2026-10-19); the export
// carries it as `forensic_pubkey_sha256`, a convenience too, and `verify-export.js` checks the
// embedded key against the RECORDED fingerprint (SPEC-023 §Operational §11). The forensic
// PRIVATE key never appears in this object (the shape has no field for it; the key is
// non-extractable).

export interface ForensicExport {
  incident_id: string;
  /** The canonicalized-evidence events in total order (time ASC, event_id ASC). */
  events: TimelineEvent[];
  /** `chain_N` (the chain head) as lowercase hex. */
  chain_root: string;
  /** Ed25519 signature over the 32 bytes of `chain_N`, base64url. */
  root_signature: string;
  /** The raw 32-byte Ed25519 forensic public key, base64url (convenience, NOT a trust root). */
  forensic_pubkey: string;
  /** The key's fingerprint, `sha256:<hex>` (SPEC-023 §Data contracts §5): a reader compares
   *  it with the one recorded at installation; the verifier checks the key against that. */
  forensic_pubkey_sha256: string;
}

export async function buildForensicExport(
  services: Pick<Services, "pg" | "ch" | "forensicKey">,
  orgId: string,
  incidentId: string,
): Promise<ForensicExport | null> {
  const timeline = await getIncidentEventTimeline(services.pg, services.ch, orgId, incidentId);
  if (!timeline) {
    return null;
  }
  return sealTimeline(services, timeline);
}

/**
 * SPEC-012's seal over a timeline: the chain of its events and the forensic key's signature
 * of the chain head. The step the export and the report share, so that a report seals the
 * timeline of its own read (SPEC-013 Amendment 2026-10-16).
 */
export async function sealTimeline(
  services: Pick<Services, "forensicKey">,
  timeline: EventTimeline,
): Promise<ForensicExport> {
  const chainN = await computeChain(timeline.events); // 32 bytes; N=0 → SHA-256("")
  const signature = await services.forensicKey.sign(chainN);

  return {
    incident_id: timeline.incident_id,
    events: timeline.events,
    chain_root: toHex(chainN),
    root_signature: Buffer.from(signature).toString("base64url"),
    forensic_pubkey: Buffer.from(services.forensicKey.publicKeyRaw).toString("base64url"),
    forensic_pubkey_sha256: await fingerprintOf(services.forensicKey.publicKeyRaw),
  };
}
