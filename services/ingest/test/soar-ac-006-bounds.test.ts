import { createRequire } from "node:module";
import { type AddressInfo, type Server, type Socket, createServer } from "node:net";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { createBoundedSmtpMailer } from "../src/notify/bounded-send.js";
import { isSendTimeout } from "../src/notify/errors.js";
import { notifyIncidentCreated } from "../src/notify/index.js";
import { buildNotifyConfig } from "../src/notify/transport.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { playbookMail, spyNotify } from "./helpers/notify.js";
import { fakeSmtp } from "./helpers/smtp.js";
import { getExecutions, seedOfficeSpawn, soarHarness, testPlaybook } from "./helpers/soar.js";

// soar_ac_006 — SPEC-022 §Operational §6: a step and a run are bounded, a send is
// closed at its bound, and a cut run neither throws nor holds the cycle.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const EMAIL = (id: string, mode: "live" | "dry_run" = "live") => ({
  id,
  action: "notify.email" as const,
  mode,
  params: { subject: `Pending ${id}`, body: "{{steps}}" },
});

async function incidentFor(org: string, agent: string) {
  const [incident] = await getIncidents(config, { agentId: agent });
  if (!incident) throw new Error(`no incident for ${org}`);
  return incident;
}

test("a step cut by its bound fails, the steps after it are skipped, the cycle returns", async () => {
  const org = "soar-ac-006-step";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  const harness = soarHarness(
    config,
    [
      testPlaybook({
        id: "pb_step_bound",
        steps: [
          { id: "isolate", action: "agent.isolate", mode: "dry_run" },
          EMAIL("notify"),
          EMAIL("after", "dry_run"),
        ],
      }),
    ],
    { stepMs: 200, runMs: 5000 },
  );
  try {
    const spy = spyNotify({ hang: playbookMail });
    const result = await runDetectionCycle(detectConfig(config, org), spy.notify, harness.soar);
    expect(result.alertsWritten).toBe(1);
  } finally {
    await harness.close();
  }
  const [execution] = await getExecutions(config, (await incidentFor(org, agent)).incident_id);
  expect(execution?.status).toBe("failed");
  expect(execution?.steps.map((s) => [s.step_id, s.outcome, s.detail])).toEqual([
    ["isolate", "would_do", expect.stringContaining("would isolate agent")],
    ["notify", "failed", "timed out after 200 ms"],
    ["after", "skipped", "an earlier step failed"],
  ]);
});

test("a run cut by its own bound fails the step it cut", async () => {
  const org = "soar-ac-006-run";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  const harness = soarHarness(
    config,
    [
      testPlaybook({
        id: "pb_run_bound",
        steps: [EMAIL("first"), EMAIL("second"), EMAIL("third")],
      }),
    ],
    { stepMs: 5000, runMs: 300 },
  );
  try {
    const spy = spyNotify({ delay: { ms: 200, which: playbookMail } });
    await runDetectionCycle(detectConfig(config, org), spy.notify, harness.soar);
  } finally {
    await harness.close();
  }
  const [execution] = await getExecutions(config, (await incidentFor(org, agent)).incident_id);
  expect(execution?.status).toBe("failed");
  expect(execution?.steps.map((s) => [s.step_id, s.outcome, s.detail])).toEqual([
    ["first", "done", "sent"],
    ["second", "failed", "run timed out after 300 ms"],
    ["third", "skipped", "an earlier step failed"],
  ]);
});

test("a step the run's bound reaches before it starts fails with the run's detail", async () => {
  const org = "soar-ac-006-reached";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  // The run's clock: its first send takes the run's whole bound, and no real time.
  let clock = Date.parse("2026-10-15T10:00:00.000Z");
  const harness = soarHarness(
    config,
    [
      testPlaybook({
        id: "pb_run_reached",
        steps: [EMAIL("first"), EMAIL("second"), EMAIL("third", "dry_run")],
      }),
    ],
    { stepMs: 5000, runMs: 300 },
    () => clock,
  );
  try {
    const spy = spyNotify({
      onSend: async (message) => {
        if (playbookMail(message)) clock += 300;
      },
    });
    await runDetectionCycle(detectConfig(config, org), spy.notify, harness.soar);
    // The second step never reached the mailer.
    expect(spy.sent.filter(playbookMail).map((m) => m.subject)).toEqual(["Pending first"]);
  } finally {
    await harness.close();
  }
  const [execution] = await getExecutions(config, (await incidentFor(org, agent)).incident_id);
  expect(execution?.status).toBe("failed");
  expect(execution?.steps.map((s) => [s.step_id, s.outcome, s.detail])).toEqual([
    ["first", "done", "sent"],
    ["second", "failed", "run timed out after 300 ms"],
    ["third", "skipped", "an earlier step failed"],
  ]);
  // Its times are read when the run's bound is found reached, by the run's clock.
  const second = execution?.steps[1];
  expect(second?.started_at).toBe("2026-10-15T10:00:00.300Z");
  expect(second?.finished_at).toBe("2026-10-15T10:00:00.300Z");
});

// A listener that accepts connections and never sends an SMTP greeting.
let silent: Server;
const held = new Set<Socket>();
beforeAll(async () => {
  silent = await new Promise<Server>((resolve, reject) => {
    const server = createServer((socket) => {
      held.add(socket);
    });
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
  });
});
afterAll(async () => {
  for (const socket of held) socket.destroy();
  await new Promise<void>((resolve) => silent.close(() => resolve()));
});

const MESSAGE = {
  from: "soc-noreply@cyberguard.test",
  to: "soc-team@cyberguard.test",
  subject: "s",
  text: "t",
};

function silentServer() {
  return { host: "127.0.0.1", port: (silent.address() as AddressInfo).port, user: "u", pass: "p" };
}

test("a bounded send to a silent server fails within the greeting timeout", async () => {
  const mailer = createBoundedSmtpMailer(silentServer());
  const started = Date.now();
  await expect(mailer.sendMail(MESSAGE, 15_000)).rejects.toThrow();
  const elapsed = Date.now() - started;
  expect(elapsed).toBeGreaterThanOrEqual(4500);
  expect(elapsed).toBeLessThan(9000);
});

test("a bounded send is closed at its own limit", async () => {
  const mailer = createBoundedSmtpMailer(silentServer());
  const started = Date.now();
  const err = await mailer.sendMail(MESSAGE, 300).then(
    () => null,
    (e: unknown) => e,
  );
  expect(isSendTimeout(err)).toBe(true);
  expect(Date.now() - started).toBeLessThan(2000);
});

test("SPEC-014's notification, through the transport it is built with, ends within the same bound", async () => {
  const server = silentServer();
  const built = buildNotifyConfig({
    ...config,
    INGEST_SMTP_HOST: server.host,
    INGEST_SMTP_PORT: server.port,
    INGEST_SMTP_USER: server.user,
    INGEST_SMTP_PASS: server.pass,
    INGEST_SMTP_FROM: MESSAGE.from,
    INGEST_NOTIFY_RECIPIENT: MESSAGE.to,
  });
  if (!built) throw new Error("notification not built");
  const logs: { obj: Record<string, unknown>; msg: string }[] = [];
  const started = Date.now();
  await notifyIncidentCreated(
    { ...built, log: { warn: (obj, msg) => logs.push({ obj, msg }) } },
    { incidentId: "i", severityId: 4, title: "t", orgId: "o" },
  );
  const elapsed = Date.now() - started;
  expect(elapsed).toBeGreaterThanOrEqual(4500);
  expect(elapsed).toBeLessThan(9000);
  expect(logs).toHaveLength(1);
  expect(logs[0]?.msg).toBe("incident notification send failed");
  expect(Object.keys(logs[0]?.obj ?? {}).sort()).toEqual(["code", "incident_id"]);
  // The greeting timeout is the transport's own: a failure with its code.
  expect(logs[0]?.obj.code).toBe("ETIMEDOUT");
});

test("SPEC-014's notification gives its send 15 000 ms", async () => {
  const limits: number[] = [];
  const spy = spyNotify({ limits });
  await notifyIncidentCreated(spy.notify, {
    incidentId: "i",
    severityId: 4,
    title: "t",
    orgId: "o",
  });
  expect(limits).toEqual([15_000]);
});

test("SPEC-014's notification logs ESENDLIMIT when its limit cuts a live send", async () => {
  // Each reply 300 ms late: the transport's own timeouts never run out, the limit does.
  const smtp = await fakeSmtp({ replyDelayMs: 300 });
  try {
    const logs: { obj: Record<string, unknown>; msg: string }[] = [];
    const started = Date.now();
    await notifyIncidentCreated(
      {
        mailer: createBoundedSmtpMailer({
          host: "127.0.0.1",
          port: smtp.port,
          user: "u",
          pass: "p",
        }),
        from: MESSAGE.from,
        recipient: MESSAGE.to,
        log: { warn: (obj, msg) => logs.push({ obj, msg }) },
        sendLimitMs: 500,
      },
      { incidentId: "i", severityId: 4, title: "t", orgId: "o" },
    );
    expect(Date.now() - started).toBeLessThan(2000);
    // The server read EHLO before the cut; the message itself never went.
    expect(smtp.commands.some((c) => /^EHLO/i.test(c))).toBe(true);
    expect(smtp.commands.some((c) => /^DATA/i.test(c))).toBe(false);
    expect(logs.map((l) => [l.msg, l.obj.code])).toEqual([
      ["incident notification send failed", "ESENDLIMIT"],
    ]);
  } finally {
    await smtp.close();
  }
});

test("the installed nodemailer is the release whose private fields the send reads", () => {
  // Debt #75: bounded-send.ts reads _supportedAuth and _ehloLines of this release.
  // A bump reads them again, or replaces the two reads, before it changes this.
  const { version } = createRequire(import.meta.url)("nodemailer/package.json") as {
    version: string;
  };
  expect(version).toBe("8.0.10");
});

test("the bounded send authenticates only when the server offers AUTH", async () => {
  for (const offerAuth of [false, true]) {
    const smtp = await fakeSmtp({ offerAuth });
    try {
      const mailer = createBoundedSmtpMailer({
        host: "127.0.0.1",
        port: smtp.port,
        user: "u",
        pass: "p",
      });
      await mailer.sendMail(MESSAGE, 5000);
      const auth = smtp.commands.filter((c) => /^AUTH/i.test(c));
      expect(auth, `offerAuth=${offerAuth}`).toHaveLength(offerAuth ? 1 : 0);
      // Offered PLAIN and LOGIN, the send takes PLAIN, the first of its order.
      if (offerAuth) expect(auth[0]?.split(" ")[1]).toBe("PLAIN");
    } finally {
      await smtp.close();
    }
  }
});

test("the bounded send takes its envelope from the headers, as SPEC-014's transport did", async () => {
  const smtp = await fakeSmtp();
  try {
    const mailer = createBoundedSmtpMailer({
      host: "127.0.0.1",
      port: smtp.port,
      user: "u",
      pass: "p",
    });
    await mailer.sendMail(
      { ...MESSAGE, from: '"CyberGuard SOC" <soc-noreply@cyberguard.test>' },
      5000,
    );
    expect(smtp.commands).toContain("MAIL FROM:<soc-noreply@cyberguard.test>");
    expect(smtp.commands).toContain("RCPT TO:<soc-team@cyberguard.test>");
  } finally {
    await smtp.close();
  }
});

const LOCAL = (port: number) => ({ host: "127.0.0.1", port, user: "u", pass: "p" });

test("a server that refuses EHLO is reached by HELO and gets no credentials", async () => {
  const smtp = await fakeSmtp({ refuseEhlo: true });
  try {
    await createBoundedSmtpMailer(LOCAL(smtp.port)).sendMail(MESSAGE, 5000);
    expect(smtp.commands.some((c) => /^HELO/i.test(c))).toBe(true);
    expect(smtp.commands.filter((c) => /^AUTH/i.test(c))).toEqual([]);
    expect(smtp.commands).toContain("MAIL FROM:<soc-noreply@cyberguard.test>");
  } finally {
    await smtp.close();
  }
});

test("a server that advertises only mechanisms the send does not use gets no credentials", async () => {
  // XOAUTH2 alone made nodemailer's login read an OAuth token that is not there,
  // and the TypeError ended the process. nodemailer also reads a name that only
  // starts a token (PLAIN-CLIENTTOKEN) as that mechanism.
  for (const offerAuth of ["GSSAPI XOAUTH2", "PLAIN-CLIENTTOKEN OAUTHBEARER LOGINX"]) {
    const smtp = await fakeSmtp({ offerAuth });
    try {
      await createBoundedSmtpMailer(LOCAL(smtp.port)).sendMail(MESSAGE, 5000);
      expect(
        smtp.commands.filter((c) => /^AUTH/i.test(c)),
        offerAuth,
      ).toEqual([]);
      expect(smtp.commands).toContain("MAIL FROM:<soc-noreply@cyberguard.test>");
    } finally {
      await smtp.close();
    }
  }
});

test("the bounded send names the first of PLAIN, LOGIN and CRAM-MD5 that the server advertises", async () => {
  const smtp = await fakeSmtp({ offerAuth: "XOAUTH2 LOGIN" });
  try {
    await createBoundedSmtpMailer(LOCAL(smtp.port)).sendMail(MESSAGE, 5000);
    expect(smtp.commands.filter((c) => /^AUTH/i.test(c)).map((c) => c.split(" ")[1])).toEqual([
      "LOGIN",
    ]);
  } finally {
    await smtp.close();
  }
});

test("a server that closes before its greeting fails the send at once, by its code", async () => {
  const smtp = await fakeSmtp({ closeBeforeGreeting: true });
  try {
    const started = Date.now();
    const err = await createBoundedSmtpMailer(LOCAL(smtp.port))
      .sendMail(MESSAGE, 5000)
      .then(
        () => null,
        (e: unknown) => e,
      );
    expect(Date.now() - started).toBeLessThan(2000);
    expect(isSendTimeout(err)).toBe(false);
    expect((err as { code?: unknown } | null)?.code).toBe("ECONNECTION");
  } finally {
    await smtp.close();
  }
});

/** Run a one-email playbook for a new High incident, its mail going to `port`. */
async function runOneEmailAgainst(org: string, port: number) {
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  const built = buildNotifyConfig({
    ...config,
    INGEST_SMTP_HOST: "127.0.0.1",
    INGEST_SMTP_PORT: port,
    INGEST_SMTP_USER: "u",
    INGEST_SMTP_PASS: "p",
    INGEST_SMTP_FROM: MESSAGE.from,
    INGEST_NOTIFY_RECIPIENT: MESSAGE.to,
  });
  if (!built) throw new Error("notification not built");
  // A step bound above the greeting timeout, so the transport's own timeout ends first.
  const harness = soarHarness(
    config,
    [testPlaybook({ id: "pb_one_email", steps: [EMAIL("notify")] })],
    { stepMs: 8000, runMs: 15_000 },
  );
  try {
    await runDetectionCycle(
      detectConfig(config, org),
      { ...built, log: { warn: () => {} } },
      harness.soar,
    );
  } finally {
    await harness.close();
  }
  const [execution] = await getExecutions(config, (await incidentFor(org, agent)).incident_id);
  return execution;
}

test("a run whose server closes the connection records a failure, not a timeout", async () => {
  const smtp = await fakeSmtp({ closeBeforeGreeting: true });
  try {
    const execution = await runOneEmailAgainst("soar-ac-006-closed", smtp.port);
    expect(execution?.status).toBe("failed");
    expect(execution?.steps.map((s) => [s.step_id, s.outcome, s.detail])).toEqual([
      ["notify", "failed", "send failed: ECONNECTION"],
    ]);
  } finally {
    await smtp.close();
  }
});

test("a run whose server never greets records the transport's timeout as a failure", async () => {
  const execution = await runOneEmailAgainst(
    "soar-ac-006-greeting",
    (silent.address() as AddressInfo).port,
  );
  expect(execution?.status).toBe("failed");
  expect(execution?.steps.map((s) => [s.step_id, s.outcome, s.detail])).toEqual([
    ["notify", "failed", "send failed: ETIMEDOUT"],
  ]);
});
