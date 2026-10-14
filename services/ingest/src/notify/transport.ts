import { pino } from "pino";
import type { Config } from "../config.js";
import { createBoundedSmtpMailer } from "./bounded-send.js";
import type { NotifyConfig } from "./index.js";

// This module is imported ONLY by services.ts (the boot graph), never by the
// detect-cycle tests, so the tests never load nodemailer. The send is the bounded
// one of ./bounded-send.ts (SPEC-022 §Operational §6), one SMTP connection per
// send, closed at its time limit.

/**
 * Build the production NotifyConfig from the optional SMTP block (SPEC-014
 * §Data contracts §3). config.ts's superRefine guarantees the block is
 * all-or-nothing, so a single present field implies all are present.
 *
 * Returns `null` when SMTP is unconfigured — notification is then disabled
 * cleanly and the boot does NOT fail (ADR-0017: test-validated altitude,
 * resolved by the prod-driver — the production detection driver now consumes
 * this). Constructed once per process,
 * mirroring api's `forensicKey` (ADR-0017 §Decision §2).
 */
export function buildNotifyConfig(config: Config): NotifyConfig | null {
  const host = config.INGEST_SMTP_HOST;
  const port = config.INGEST_SMTP_PORT;
  const user = config.INGEST_SMTP_USER;
  const pass = config.INGEST_SMTP_PASS;
  const from = config.INGEST_SMTP_FROM;
  const recipient = config.INGEST_NOTIFY_RECIPIENT;
  if (
    host === undefined ||
    port === undefined ||
    user === undefined ||
    pass === undefined ||
    from === undefined ||
    recipient === undefined
  ) {
    return null;
  }

  // Generic SMTP (ADR-0017 §Decision §1) — Gmail is reached as one SMTP host
  // (smtp.gmail.com) among many, never the Gmail REST API / OAuth.
  const mailer = createBoundedSmtpMailer({ host, port, user, pass });
  const log = pino({ name: "ingest-notify", level: config.INGEST_LOG_LEVEL });

  return {
    mailer,
    from,
    recipient,
    log,
  };
}
