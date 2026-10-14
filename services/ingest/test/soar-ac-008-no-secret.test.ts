import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import type { NotifyConfig } from "../src/notify/index.js";
import { buildNotifyConfig } from "../src/notify/transport.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";
import { type FakeSmtp, fakeSmtp } from "./helpers/smtp.js";
import {
  type SoarHarness,
  getExecutions,
  seedOfficeSpawn,
  shippedPlaybooks,
  soarHarness,
} from "./helpers/soar.js";

// soar_ac_008 — SPEC-022 NFR-022-003: no record field and no log line holds an SMTP
// credential or the configured recipient's address. A failed send is reported by
// its code only, even when the server's error quotes the address. The mailer is
// the production one, configured with a password, against a local SMTP server that
// asks for it.

const RECIPIENT = "soc-team@cyberguard.test";
const PASSWORD = "s3cret-smtp-pass";

let config: Config;
let harness: SoarHarness;
beforeAll(() => {
  config = inject("ingestConfig");
  harness = soarHarness(config, shippedPlaybooks());
});
afterAll(async () => {
  await harness?.close();
});

type LogLine = { obj: Record<string, unknown>; msg: string };

/** The production notification carrier against `smtp`, its log captured. */
function realCarrier(smtp: FakeSmtp): { notify: NotifyConfig; logs: LogLine[] } {
  const built = buildNotifyConfig({
    ...config,
    INGEST_SMTP_HOST: "127.0.0.1",
    INGEST_SMTP_PORT: smtp.port,
    INGEST_SMTP_USER: "cg-notify",
    INGEST_SMTP_PASS: PASSWORD,
    INGEST_SMTP_FROM: "soc-noreply@cyberguard.test",
    INGEST_NOTIFY_RECIPIENT: RECIPIENT,
  });
  if (!built) throw new Error("notification not built");
  const logs: LogLine[] = [];
  return { notify: { ...built, log: { warn: (obj, msg) => logs.push({ obj, msg }) } }, logs };
}

async function runFor(org: string, notify: NotifyConfig) {
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  await runDetectionCycle(detectConfig(config, org), notify, harness.soar);
  const [incident] = await getIncidents(config, { agentId: agent });
  const [execution] = await getExecutions(config, incident?.incident_id ?? "");
  if (!execution) throw new Error("no execution");
  return execution;
}

function everything(...parts: unknown[]): string {
  return JSON.stringify(parts);
}

/** Whether the server received the password, in an AUTH PLAIN line. */
function passwordSent(smtp: FakeSmtp): boolean {
  return smtp.commands.some((c) => {
    const payload = /^AUTH PLAIN (\S+)$/i.exec(c)?.[1];
    return (
      payload !== undefined && Buffer.from(payload, "base64").toString("utf8").includes(PASSWORD)
    );
  });
}

test("a successful run, with the password in use, records and logs neither it nor the address", async () => {
  const smtp = await fakeSmtp({ offerAuth: true });
  try {
    const carrier = realCarrier(smtp);
    const before = harness.logs.length;
    const execution = await runFor("soar-ac-008-ok", carrier.notify);

    expect(execution.status).toBe("completed");
    expect(execution.steps[1]).toMatchObject({ step_id: "notify", outcome: "done" });
    expect(passwordSent(smtp)).toBe(true);
    const seen = everything(execution, harness.logs.slice(before), carrier.logs);
    expect(seen).not.toContain(RECIPIENT);
    expect(seen).not.toContain(PASSWORD);
  } finally {
    await smtp.close();
  }
});

test("a send the server refuses, quoting the address, is recorded and logged by its code", async () => {
  const smtp = await fakeSmtp({
    offerAuth: true,
    rcptReply: `550 5.1.1 <${RECIPIENT}>: user unknown`,
  });
  try {
    const carrier = realCarrier(smtp);
    // The error the mailer gives here quotes the address.
    const err = await carrier.notify.mailer
      .sendMail({ from: carrier.notify.from, to: RECIPIENT, subject: "s", text: "t" }, 5000)
      .then(
        () => null,
        (e: unknown) => e,
      );
    expect(String((err as Error | null)?.message)).toContain(RECIPIENT);

    const before = harness.logs.length;
    const execution = await runFor("soar-ac-008-refused", carrier.notify);

    expect(execution.status).toBe("failed");
    expect(execution.steps[1]).toMatchObject({
      step_id: "notify",
      outcome: "failed",
      detail: "send failed: EENVELOPE 550",
    });
    // SPEC-014's own failure log carries the code too.
    expect(carrier.logs).toEqual([
      {
        obj: { code: "EENVELOPE 550", incident_id: expect.any(String) },
        msg: "incident notification send failed",
      },
    ]);
    expect(passwordSent(smtp)).toBe(true);
    const seen = everything(execution, harness.logs.slice(before), carrier.logs);
    expect(seen).not.toContain(RECIPIENT);
    expect(seen).not.toContain(PASSWORD);
  } finally {
    await smtp.close();
  }
});

test("a failed send whose error quotes the address and a secret is recorded and logged by its code", async () => {
  const rejected = Object.assign(
    new Error(
      `Can't send mail - all recipients were rejected: 550 5.1.1 <${RECIPIENT}>: user unknown; auth ${PASSWORD}`,
    ),
    { code: "EENVELOPE", responseCode: 550 },
  );
  const spy = spyNotify({ throwOnSend: rejected });
  const before = harness.logs.length;
  const execution = await runFor("soar-ac-008-failed", spy.notify);

  expect(execution.status).toBe("failed");
  expect(execution.steps[1]).toMatchObject({
    step_id: "notify",
    outcome: "failed",
    detail: "send failed: EENVELOPE 550",
  });
  expect(spy.logs).toEqual([
    {
      obj: { code: "EENVELOPE 550", incident_id: expect.any(String) },
      msg: "incident notification send failed",
    },
  ]);
  const seen = everything(execution, harness.logs.slice(before), spy.logs);
  expect(seen).not.toContain(RECIPIENT);
  expect(seen).not.toContain(PASSWORD);
});
