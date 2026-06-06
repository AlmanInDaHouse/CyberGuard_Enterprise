import type { TimelineEvent } from "../read/types.js";
import { canonicalEvent } from "./canonical.js";

// SPEC-012 §Data contracts §2 — the per-event SHA-256 evidence hash-chain over the
// canonicalized drill output. This is PURE integrity (gate A+B): it computes the
// chain head `chain_N`; it does NOT sign it (the Ed25519 `root_signature` over
// `chain_N` is gate C). SHA-256 uses native WebCrypto (the `jcs.ts:11` precedent;
// Node >= 22 per `package.json`). No crypto dependency is added — only `canonicalize`.

const subtle = globalThis.crypto.subtle;

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await subtle.digest("SHA-256", bytes));
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

/** Lowercase hex of a digest. */
export function toHex(bytes: Uint8Array): string {
  let hex = "";
  for (const b of bytes) {
    hex += b.toString(16).padStart(2, "0");
  }
  return hex;
}

/** `evidence_n.hash = SHA-256(UTF-8(canonical(evidence_n)))` — 32 bytes. */
export async function evidenceHash(event: TimelineEvent): Promise<Uint8Array> {
  return sha256(new TextEncoder().encode(canonicalEvent(event)));
}

/**
 * The chain head `chain_N` over `events` IN THE GIVEN ORDER — the drill serves the
 * total order `(time, event_id)` (SPEC-010 Amendment 2026-06-06 + Pieza A), so the
 * order is the evidence order:
 *
 *   chain_0 = SHA-256("")                                 (the empty input)
 *   chain_n = SHA-256(chain_{n-1} ++ evidence_n.hash)     (raw 32-byte concat, no sep)
 *
 * There is NO separate `timestamp_n` term — the event_time already lives inside
 * `canonical(evidence_n)` (SPEC-012 §Data contracts §2). Returns the 32-byte
 * `chain_N`. For zero events, `chain_N = chain_0 = SHA-256("")`.
 */
export async function computeChain(events: TimelineEvent[]): Promise<Uint8Array> {
  let chain = await sha256(new Uint8Array(0)); // chain_0 = SHA-256("")
  for (const event of events) {
    chain = await sha256(concat(chain, await evidenceHash(event)));
  }
  return chain;
}

/** Hex of `chain_N`. */
export async function computeChainHex(events: TimelineEvent[]): Promise<string> {
  return toHex(await computeChain(events));
}

/**
 * Integrity verification (gate A+B): recompute `chain_N` from `events` (in order) and
 * compare to `expectedChainHex`. This is NOT signature verification — Ed25519 over
 * `chain_N` is gate C; this proves only that the events reproduce the expected chain
 * head, i.e. they have not been altered, reordered, added to, or dropped.
 */
export async function verifyChain(
  events: TimelineEvent[],
  expectedChainHex: string,
): Promise<boolean> {
  return (await computeChainHex(events)) === expectedChainHex.toLowerCase();
}
