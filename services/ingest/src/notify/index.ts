// SPEC-014 / ADR-0017 — incident email notification (notify-only, on incident
// CREATE). This module is the ingest detection pipeline's FIRST external (network)
// side-effect; ADR-0017 §2 bounds it to fire-and-forget AFTER the incident upsert
// has committed. It carries NO transport library import — the transport is injected
// (NotifyConfig.mailer), so the detection-cycle tests inject an in-memory fake and
// never load nodemailer (the real transport lives in ./transport.ts).

/**
 * The minimal mail surface a transport must expose. nodemailer's `Transporter`
 * satisfies it structurally (`sendMail`), and a test injects a spy / JSON transport.
 */
export interface NotifyMailer {
  sendMail(message: NotifyMessage): Promise<unknown>;
}

export interface NotifyMessage {
  from: string;
  to: string;
  subject: string;
  text: string;
}

/**
 * The minimal log surface (pino's `Logger` satisfies it). The detect path has no
 * logger of its own; the fire-and-forget failure path writes here.
 */
export interface NotifyLogSink {
  warn(obj: Record<string, unknown>, msg: string): void;
}

/**
 * The notify dependency, injected as a SEPARATE carrier (DetectConfig is untouched).
 * Constructed once at boot in prod (./transport.ts `buildNotifyConfig`, mirroring
 * api's `forensicKey`), or as a fake in a test.
 */
export interface NotifyConfig {
  mailer: NotifyMailer;
  from: string;
  recipient: string;
  log: NotifyLogSink;
}

/** The incident fields the email carries — all available at the upsert seam, no extra read. */
export interface IncidentNotification {
  incidentId: string;
  severityId: number;
  title: string;
  orgId: string;
}

/**
 * Send one best-effort email for a newly-created incident (SPEC-014 §AC notify_ac_001/004).
 *
 * Fire-and-forget (ADR-0017 §2): this function NEVER throws. A send failure (SMTP
 * unreachable, auth rejected, timeout) is caught, logged to the sink, and swallowed
 * — runDetectionCycle has no surrounding catch, so propagating would abort the
 * detection cycle, which the persisted-incident-is-the-truth contract forbids.
 * The caller awaits this (deterministic for tests); awaiting cannot abort the cycle
 * because no error escapes.
 */
export async function notifyIncidentCreated(
  notify: NotifyConfig,
  incident: IncidentNotification,
): Promise<void> {
  try {
    const subject = `[CyberGuard] New incident (severity ${incident.severityId}): ${incident.title}`;
    const text = [
      "A new CyberGuard incident was created.",
      "",
      `Incident: ${incident.incidentId}`,
      `Severity: ${incident.severityId}`,
      `Title:    ${incident.title}`,
      `Org:      ${incident.orgId}`,
    ].join("\n");
    await notify.mailer.sendMail({ from: notify.from, to: notify.recipient, subject, text });
  } catch (err) {
    notify.log.warn(
      {
        err: err instanceof Error ? err.message : String(err),
        incident_id: incident.incidentId,
      },
      "incident notification send failed",
    );
  }
}
