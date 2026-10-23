import { spawnSync } from "node:child_process";
import { createHash, type webcrypto } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { norm, pdfText, stripWs } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// deploy_ac_010 — SPEC-023 §Data contracts §4 and §5, §Operational §11 (ADR-0016 Amendment
// 2026-10-19), the api's half: `forensic-key.js` prints the key the `forensic_key` row
// holds and the SHA-256 of its 32 bytes; the export carries `forensic_pubkey_sha256` and
// the report its text; `verify-export.js` verifies an export against a recorded
// fingerprint, and refuses or fails each way the SPEC names. The bundle's half (the export
// requested on the `app` network, the verifier from the api's image) is the bundle's test.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const ORG = "deploy-ac-010";

let config: Config;
let apph: Awaited<ReturnType<typeof buildTestApp>>;
let dir: string;
let exportBody: Record<string, unknown> & { events: Record<string, unknown>[] };
let incidentId: string;

/** The process environment without the API_* variables a developer's shell may set. */
function baseEnv(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("API_")));
}

function run(script: string, argv: string[], env: Record<string, string> = {}) {
  const r = spawnSync(process.execPath, ["--import", "tsx", script, ...argv], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...baseEnv(), ...env },
    timeout: 60_000,
  });
  return { status: r.status, stdout: r.stdout, stderr: r.stderr };
}

async function storedPublicKey(): Promise<Buffer> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<{ public_key: Buffer }>(
      "SELECT public_key FROM forensic_key WHERE id = 1",
    );
    return r.rows[0]?.public_key as Buffer;
  } finally {
    await pool.end();
  }
}

const sha256Hex = (bytes: Uint8Array): string => createHash("sha256").update(bytes).digest("hex");

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  dir = mkdtempSync(join(tmpdir(), "cg-deploy-010-"));

  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  incidentId = globalThis.crypto.randomUUID();
  const event1 = globalThis.crypto.randomUUID();
  const event2 = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, ORG);
  await insertCgesEvent(config, {
    agentId,
    eventId: event1,
    orgId: ORG,
    time: "2026-10-23 11:00:00.000000000",
    processPid: 7001,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: event2,
    orgId: ORG,
    time: "2026-10-23 11:00:02.000000000",
    processPid: 7002,
  });
  await insertAlertRow(config, { alertId, agentId, orgId: ORG, sourceEvents: [event1, event2] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: ORG, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-deploy-010",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: ORG,
  });
  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/forensic-export`,
    cookies: { cgsess: "tok-deploy-010" },
  });
  expect(res.statusCode).toBe(200);
  exportBody = res.json();
});

afterAll(async () => {
  await apph?.close();
  if (dir) rmSync(dir, { recursive: true, force: true });
});

describe("forensic-key.js", () => {
  test("prints the stored key in base64url and the SHA-256 of its 32 bytes, from API_PG_URL alone", async () => {
    const r = run("src/cli/forensic-key.ts", [], { API_PG_URL: config.API_PG_URL });
    expect(r.status, r.stderr).toBe(0);
    const stored = await storedPublicKey();
    expect(stored.length).toBe(32);
    expect(r.stdout).toBe(`${stored.toString("base64url")}\nsha256:${sha256Hex(stored)}\n`);
  });
});

describe("the export and the report carry the fingerprint", () => {
  test("the export's forensic_pubkey_sha256 is the fingerprint of its forensic_pubkey", async () => {
    const stored = await storedPublicKey();
    expect(exportBody.forensic_pubkey).toBe(stored.toString("base64url"));
    expect(exportBody.forensic_pubkey_sha256).toBe(`sha256:${sha256Hex(stored)}`);
  });

  test("the report prints the fingerprint and checks the key against the recorded one", async () => {
    const res = await apph.app.inject({
      method: "GET",
      url: `/v1/incidents/${incidentId}/report`,
      cookies: { cgsess: "tok-deploy-010" },
    });
    expect(res.statusCode).toBe(200);
    const text = await pdfText(res.rawPayload);
    expect(stripWs(text)).toContain(stripWs(String(exportBody.forensic_pubkey_sha256)));
    const t = norm(text);
    expect(t).toContain("recorded at installation");
    expect(t).not.toContain("open questions 1");
  });
});

describe("verify-export.js", () => {
  const fingerprint = () => String(exportBody.forensic_pubkey_sha256);
  function write(name: string, body: unknown): string {
    const file = join(dir, name);
    writeFileSync(file, JSON.stringify(body));
    return file;
  }
  // The verifier reads no store and needs no configuration: no API_* variable is set.
  const verify = (argv: string[]) => run("src/cli/verify-export.ts", argv);

  test("verifies an export against its fingerprint: exit 0", () => {
    const r = verify(["--fingerprint", fingerprint(), write("ok.json", exportBody)]);
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout.trim()).toBe(`verified: 2 events, chain head ${exportBody.chain_root}`);
  });

  test("without --fingerprint it refuses: exit 2", () => {
    const r = verify([write("nofp.json", exportBody)]);
    expect(r.status).toBe(2);
  });

  test("with another fingerprint: the key's mismatch line, exit 1", () => {
    const other = `sha256:${"0".repeat(64)}`;
    const r = verify(["--fingerprint", other, write("otherfp.json", exportBody)]);
    expect(r.status).toBe(1);
    expect(r.stdout.trim()).toBe("the export's key does not match the recorded fingerprint");
  });

  test("with an event's field changed: the chain-head line, exit 1", () => {
    const events = exportBody.events.map((e, i) => (i === 1 ? { ...e, process_pid: 9999 } : e));
    const r = verify([
      "--fingerprint",
      fingerprint(),
      write("event.json", { ...exportBody, events }),
    ]);
    expect(r.status).toBe(1);
    expect(r.stdout.trim()).toBe("the events do not give the export's chain head");
  });

  test("with the signature changed: the signature line, exit 1", () => {
    const sig = Buffer.from(String(exportBody.root_signature), "base64url");
    sig[0] = (sig[0] ?? 0) ^ 0xff;
    const r = verify([
      "--fingerprint",
      fingerprint(),
      write("sig.json", { ...exportBody, root_signature: sig.toString("base64url") }),
    ]);
    expect(r.status).toBe(1);
    expect(r.stdout.trim()).toBe("the signature does not verify");
  });

  test("an export whose key was replaced, chain and signature consistent with it, fails on the key", async () => {
    // A server that re-signed with its own key embeds that key: the fingerprint catches it.
    const keys = (await globalThis.crypto.subtle.generateKey({ name: "Ed25519" }, true, [
      "sign",
      "verify",
    ])) as webcrypto.CryptoKeyPair;
    const raw = new Uint8Array(await globalThis.crypto.subtle.exportKey("raw", keys.publicKey));
    const head = Buffer.from(String(exportBody.chain_root), "hex");
    const sig = new Uint8Array(
      await globalThis.crypto.subtle.sign({ name: "Ed25519" }, keys.privateKey, head),
    );
    const forged = {
      ...exportBody,
      forensic_pubkey: Buffer.from(raw).toString("base64url"),
      forensic_pubkey_sha256: `sha256:${sha256Hex(raw)}`,
      root_signature: Buffer.from(sig).toString("base64url"),
    };
    const r = verify(["--fingerprint", fingerprint(), write("forged.json", forged)]);
    expect(r.status).toBe(1);
    expect(r.stdout.trim()).toBe("the export's key does not match the recorded fingerprint");
  });
});
