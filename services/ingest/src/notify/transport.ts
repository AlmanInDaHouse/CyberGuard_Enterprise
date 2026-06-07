import { createRequire } from "node:module";
import type Nodemailer from "nodemailer";
import { pino } from "pino";
import type { Config } from "../config.js";
import type { NotifyConfig } from "./index.js";

// nodemailer is CJS; load via createRequire for clean ESM interop — the same
// pattern ingest already uses for `canonicalize` (src/jcs.ts). The `import type`
// pulls @types/nodemailer at compile time only; the runtime value comes from
// createRequire. This module is imported ONLY by services.ts (the boot graph),
// never by the detect-cycle tests, so the tests never load nodemailer.
const nodemailer = createRequire(import.meta.url)("nodemailer") as typeof Nodemailer;

/**
 * Build the production NotifyConfig from the optional SMTP block (SPEC-014
 * §Data contracts §3). config.ts's superRefine guarantees the block is
 * all-or-nothing, so a single present field implies all are present.
 *
 * Returns `null` when SMTP is unconfigured — notification is then disabled
 * cleanly and the boot does NOT fail (ADR-0017: test-validated altitude; no
 * production detection driver consumes this yet). Constructed once per process,
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
  const transporter = nodemailer.createTransport({
    host,
    port,
    secure: port === 465,
    auth: { user, pass },
  });
  const log = pino({ name: "ingest-notify", level: config.INGEST_LOG_LEVEL });

  return {
    mailer: { sendMail: (message) => transporter.sendMail(message) },
    from,
    recipient,
    log,
  };
}
