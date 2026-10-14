import { createHash } from "node:crypto";
import { canonicalize } from "../jcs.js";
import type { StepRecord } from "./types.js";

// SPEC-022 §Data contracts §3 — SPEC-012's chain construction over the steps of an
// execution record (services/api/src/forensic/hashchain.ts applies the same one
// to evidence):
//
//   link_0      = SHA-256("")
//   step_n.hash = SHA-256(UTF-8(JCS(step_n without "link")))
//   link_n      = SHA-256(link_{n-1} ++ step_n.hash)     (raw 32-byte digests)
//   chain_head  = link_N
//
// The chain is unsigned (ADR-0021 §4): it shows an edit that does not recompute it.

/** A step as recorded, before its link is known. */
export type UnlinkedStep = Omit<StepRecord, "link">;

function sha256(...parts: Uint8Array[]): Buffer {
  const hash = createHash("sha256");
  for (const part of parts) hash.update(part);
  return hash.digest();
}

function stepHash(step: UnlinkedStep): Buffer {
  const canonical = canonicalize(step);
  if (canonical === undefined) throw new Error("soar: a step did not canonicalize");
  return sha256(Buffer.from(canonical, "utf8"));
}

/** Link each step to the one before it; the head is the last link (`link_0` for none). */
export function chainSteps(steps: UnlinkedStep[]): { steps: StepRecord[]; head: string } {
  let link = sha256(new Uint8Array(0));
  const linked = steps.map((step) => {
    link = sha256(link, stepHash(step));
    return { ...step, link: link.toString("hex") };
  });
  return { steps: linked, head: link.toString("hex") };
}

/** Recompute the head from stored steps, ignoring their stored links. */
export function recomputeHead(steps: StepRecord[]): string {
  return chainSteps(steps.map(({ link: _link, ...rest }) => rest)).head;
}
