import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { withVerificationCode } from "../src/notify/bounded-send.js";
import { sendFailureCode } from "../src/notify/errors.js";
import { notifyIncidentCreated } from "../src/notify/index.js";
import { buildNotifyConfig } from "../src/notify/transport.js";
import { type FakeSmtp, fakeSmtp } from "./helpers/smtp.js";
import { type RelayPki, relayPki } from "./helpers/tls-cert.js";

// deploy_ac_018 — SPEC-023 §Operational §14 (debt #78), ingest's suite: with
// INGEST_SMTP_CA_FILE naming a test CA, a notification reaches a local relay whose
// certificate that CA issued; without it, the send fails, and its log line carries the
// certificate's verification code, not ESOCKET. The SOAR step's error is
// soar-ac-006-bounds.test.ts's, which this criterion restates.

const MESSAGE = { from: "soc-noreply@cyberguard.test", to: "soc@cyberguard.test" };

let config: Config;
let pki: RelayPki;
let smtp: FakeSmtp;
let dir: string;
let caFile: string;

beforeAll(async () => {
  config = inject("ingestConfig");
  pki = await relayPki();
  dir = mkdtempSync(join(tmpdir(), "cg-relay-ca-"));
  caFile = join(dir, "relay-ca.pem");
  writeFileSync(caFile, pki.caPem);
  smtp = await fakeSmtp({ offerAuth: true, starttls: pki.relay });
});

afterAll(async () => {
  await smtp?.close();
  rmSync(dir, { recursive: true, force: true });
});

/** SPEC-014's notification, built from `config` against the test's relay. */
function notifyWith(extra: Partial<Config>, port = smtp.port) {
  const built = buildNotifyConfig({
    ...config,
    INGEST_SMTP_HOST: "127.0.0.1",
    INGEST_SMTP_PORT: port,
    INGEST_SMTP_USER: "u",
    INGEST_SMTP_PASS: "p",
    INGEST_SMTP_FROM: MESSAGE.from,
    INGEST_NOTIFY_RECIPIENT: MESSAGE.to,
    ...extra,
  });
  if (!built) throw new Error("notification not built");
  const logs: { obj: Record<string, unknown>; msg: string }[] = [];
  const notify = {
    ...built,
    log: { warn: (obj: Record<string, unknown>, msg: string) => logs.push({ obj, msg }) },
  };
  return { notify, logs };
}

const INCIDENT = { incidentId: "i", severityId: 4, title: "t", orgId: "o" };

test("with INGEST_SMTP_CA_FILE naming the relay's CA, a notification reaches the relay over TLS", async () => {
  const before = smtp.commandsOverTls.length;
  const { notify, logs } = notifyWith({ INGEST_SMTP_CA_FILE: caFile });
  await notifyIncidentCreated(notify, INCIDENT);
  expect(logs).toEqual([]);
  const overTls = smtp.commandsOverTls.slice(before);
  expect(overTls).toContain(`MAIL FROM:<${MESSAGE.from}>`);
  expect(overTls).toContain("DATA");
});

test("without it, the send fails with the certificate's verification code, and the relay gets nothing", async () => {
  const before = smtp.commands.length;
  const { notify, logs } = notifyWith({ INGEST_SMTP_CA_FILE: undefined });
  await notifyIncidentCreated(notify, INCIDENT);
  expect(logs.map((l) => [l.msg, l.obj.code])).toEqual([
    ["incident notification send failed", "UNABLE_TO_VERIFY_LEAF_SIGNATURE"],
  ]);
  expect(smtp.commands.slice(before).filter((c) => /^(AUTH|MAIL)/i.test(c))).toEqual([]);
});

/** The code SPEC-014's notification logs against a relay serving `relay`, the CA file set. */
async function codeAgainst(relay: RelayPki["relay"]): Promise<unknown> {
  const other = await fakeSmtp({ offerAuth: true, starttls: relay });
  try {
    const { notify, logs } = notifyWith({ INGEST_SMTP_CA_FILE: caFile }, other.port);
    await notifyIncidentCreated(notify, INCIDENT);
    expect(other.commands.filter((c) => /^(AUTH|MAIL)/i.test(c))).toEqual([]);
    return logs[0]?.obj.code;
  } finally {
    await other.close();
  }
}

test("a relay certificate the CA issued for another name fails with the name check's code", async () => {
  const elsewhere = await relayPki({ ip: "127.0.0.2" });
  // The CA file trusts this other PKI's CA, so the name is all that fails.
  writeFileSync(caFile, elsewhere.caPem);
  try {
    expect(await codeAgainst(elsewhere.relay)).toBe("ERR_TLS_CERT_ALTNAME_INVALID");
  } finally {
    writeFileSync(caFile, pki.caPem);
  }
});

test("a connection error carries the verification's code, for the codes Node gives a certificate it refuses", () => {
  // As nodemailer reports one: its own ESOCKET, Node's code kept as originalCode.
  const failed = (originalCode: string) =>
    Object.assign(new Error("x"), { code: "ESOCKET", originalCode });
  for (const code of [
    "UNABLE_TO_VERIFY_LEAF_SIGNATURE",
    "UNABLE_TO_GET_ISSUER_CERT_LOCALLY",
    "UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY",
    "SELF_SIGNED_CERT_IN_CHAIN",
    "CERT_HAS_EXPIRED",
    // What Node names a verification error it has no name for (a digest too weak).
    "UNSPECIFIED",
    "ERR_TLS_CERT_ALTNAME_INVALID",
  ]) {
    const err = withVerificationCode(failed(code));
    expect((err as { code?: unknown }).code).toBe(code);
    expect(sendFailureCode(err)).toBe(code);
  }
  // Another socket error keeps nodemailer's code.
  expect((withVerificationCode(failed("ECONNRESET")) as { code?: unknown }).code).toBe("ESOCKET");
});
