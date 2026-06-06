import { createRequire } from "node:module";
import type { TimelineEvent } from "../read/types.js";

// SPEC-012 §Data contracts §1/§2 — JCS (RFC 8785) canonicalization of one forensic
// evidence unit (a drill `TimelineEvent`). `canonicalize` is the same RFC-8785
// implementation services/ingest uses for the SPEC-003 envelope signature
// (`services/ingest/src/jcs.ts`); `services/api` adopts it as its OWN dependency
// rather than importing ingest's `jcs.ts` (ingest-coupled, no cross-package export —
// SPEC-012 §Scope §2). `canonicalize` is CJS; load via `createRequire` for clean ESM
// interop (mirrors `jcs.ts:1-9`).
const canonicalize = createRequire(import.meta.url)("canonicalize") as (
  value: unknown,
) => string | undefined;

/**
 * The canonical (JCS, RFC 8785) serialization of one evidence unit: object keys are
 * lexicographically ordered, whitespace-free, with a fixed number/string encoding.
 * So `canonicalEvent(e)` is byte-for-byte reproducible regardless of the insertion
 * order of the event's fields — the determinism the hash-chain rests on
 * (SPEC-012 hashchain_ac_002). Throws only on a non-JSON value, which a
 * `TimelineEvent` (plain strings / numbers / null) never is — a programmer error,
 * not an input error.
 */
export function canonicalEvent(event: TimelineEvent): string {
  const canonical = canonicalize(event);
  if (canonical === undefined) {
    throw new Error("forensic: TimelineEvent did not canonicalize (non-JSON value)");
  }
  return canonical;
}
