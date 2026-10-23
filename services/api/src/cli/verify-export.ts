// SPEC-023 §Operational §11 (ADR-0016 Amendment 2026-10-19 item 2) — the verifier of a
// forensic export. It starts from the fingerprint recorded at installation, not from the
// export: it refuses to run without one, refuses an export whose embedded key is not the
// recorded one, and only then recomputes the chain (SPEC-012 §2) and checks the signature.
// It reads no store and needs no configuration.
import { readFileSync } from "node:fs";
import { fingerprintOf } from "../forensic/fingerprint.js";
import { computeChain, toHex } from "../forensic/hashchain.js";
import { isMain } from "../is-main.js";
import type { TimelineEvent } from "../read/types.js";

/** What the verifier decided: its line and its exit code. */
export interface Verdict {
  code: 0 | 1 | 2;
  line: string;
}

const USAGE = "usage: verify-export --fingerprint sha256:<hex> <export.json>";
const FINGERPRINT = /^sha256:[0-9a-f]{64}$/;

function refused(reason: string): Verdict {
  return { code: 2, line: `refused: ${reason}\n${USAGE}` };
}

interface ExportShape {
  events: TimelineEvent[];
  chain_root: string;
  root_signature: string;
  forensic_pubkey: string;
}

function shapeOf(value: unknown): ExportShape | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  return Array.isArray(v.events) &&
    typeof v.chain_root === "string" &&
    typeof v.root_signature === "string" &&
    typeof v.forensic_pubkey === "string"
    ? (v as unknown as ExportShape)
    : null;
}

/**
 * Verify the export at `file` against the recorded `fingerprint`. The hexadecimal digits
 * are compared without case, since some tools print a SHA-256 in capitals.
 */
export async function verifyExport(recorded: string, file: string): Promise<Verdict> {
  const fingerprint = `${recorded.slice(0, 7)}${recorded.slice(7).toLowerCase()}`;
  if (!FINGERPRINT.test(fingerprint)) {
    return refused("the fingerprint is sha256: and 64 hexadecimal digits");
  }
  let text: string;
  try {
    text = readFileSync(file, "utf8");
  } catch {
    return refused(`cannot read ${file}`);
  }
  let exp: ExportShape | null;
  try {
    exp = shapeOf(JSON.parse(text));
  } catch {
    exp = null;
  }
  if (!exp) return refused("the file is not a forensic export");
  const key = Buffer.from(exp.forensic_pubkey, "base64url");
  if (key.length !== 32 || (await fingerprintOf(key)) !== fingerprint) {
    return { code: 1, line: "the export's key does not match the recorded fingerprint" };
  }
  const head = await computeChain(exp.events);
  if (toHex(head) !== exp.chain_root) {
    return { code: 1, line: "the events do not give the export's chain head" };
  }
  const subtle = globalThis.crypto.subtle;
  const publicKey = await subtle.importKey("raw", key, { name: "Ed25519" }, false, ["verify"]);
  const signature = Buffer.from(exp.root_signature, "base64url");
  const ok = await subtle.verify({ name: "Ed25519" }, publicKey, signature, head);
  return ok
    ? { code: 0, line: `verified: ${exp.events.length} events, chain head ${toHex(head)}` }
    : { code: 1, line: "the signature does not verify" };
}

/** The command line: `--fingerprint sha256:<hex> <export.json>`. */
export async function runVerifyExport(argv: string[]): Promise<Verdict> {
  const i = argv.indexOf("--fingerprint");
  if (i === -1 || argv[i + 1] === undefined) return refused("--fingerprint is required");
  const fingerprint = argv[i + 1] as string;
  const files = argv.filter((_, j) => j !== i && j !== i + 1);
  if (files.length !== 1) return refused("one export file");
  return verifyExport(fingerprint, files[0] as string);
}

if (isMain(import.meta.url)) {
  runVerifyExport(process.argv.slice(2)).then(
    (v) => {
      // The verdict on stdout; a refusal, with the usage, on stderr.
      (v.code === 2 ? process.stderr : process.stdout).write(`${v.line}\n`);
      process.exit(v.code);
    },
    (e: unknown) => {
      process.stderr.write(`cg-api verify-export: fatal: ${String(e)}\n`);
      process.exit(1);
    },
  );
}
