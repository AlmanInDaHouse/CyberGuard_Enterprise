import { createRequire } from "node:module";
import { SEND_TIMEOUT_CODE } from "./errors.js";
import type { NotifyMailer, NotifyMessage } from "./index.js";

// SPEC-022 §Operational §6 — the bounded SMTP send that notify.email and SPEC-014's
// notification share. nodemailer's transport cannot abort a send in flight (its
// close() leaves a busy connection open), so each send opens its own SMTPConnection
// and closes it at its time limit: no send outlives its bound.
//
// nodemailer's lib/ modules load through createRequire, as transport.ts loads the
// package; for `require`, 10.1.1's exports map gives each its CommonJS build.

interface SmtpConnection {
  // Two fields private to nodemailer 10.1.1, which package.json pins exactly (debt
  // #75), and `secure` below, public but undocumented; soar_ac_006 fails on another
  // release, and its HELO, mechanism and TLS cases fail if any of the three changes.
  /**
   * The AUTH mechanisms nodemailer matched in the last EHLO reply, among PLAIN, LOGIN,
   * CRAM-MD5 and XOAUTH2. Only that reply fills it (after STARTTLS, the one after
   * TLS): on HELO it stays empty while `allowsAuth` is set to true. 10.1.1 matches
   * whole tokens of an AUTH line; 8.0.10 matched a prefix, so PLAIN-CLIENTTOKEN
   * counted as PLAIN there. The send reads it only to know that AUTH came in an
   * EHLO reply, and reads the names from `_ehloLines` itself.
   */
  _supportedAuth: string[];
  /** The lines of the last EHLO reply, without their reply codes. */
  _ehloLines?: string[];
  /** Whether the connection is encrypted: implicit TLS, or a completed STARTTLS. */
  secure: boolean;
  once(event: "error", listener: (err: Error) => void): void;
  once(event: "end", listener: () => void): void;
  connect(callback: () => void): void;
  login(
    auth: { user: string; pass: string; method: string },
    callback: (err?: Error | null) => void,
  ): void;
  send(
    envelope: { from: string; to: string[] },
    message: Buffer,
    callback: (err?: Error | null) => void,
  ): void;
  quit(): void;
  close(): void;
}

type SmtpConnectionCtor = new (options: Record<string, unknown>) => SmtpConnection;
interface MimeMessage {
  getEnvelope(): { from: string | false; to: string[] };
  build(callback: (err: Error | null, message: Buffer) => void): void;
}
type MailComposerCtor = new (mail: Record<string, unknown>) => { compile(): MimeMessage };

const load = createRequire(import.meta.url);
const SMTPConnection = load("nodemailer/lib/smtp-connection") as SmtpConnectionCtor;
const MailComposer = load("nodemailer/lib/mail-composer") as MailComposerCtor;

/** The transport's own timeouts (SPEC-022 §Operational §6). */
export const SMTP_CONNECTION_TIMEOUT_MS = 5000;
export const SMTP_GREETING_TIMEOUT_MS = 5000;
export const SMTP_SOCKET_TIMEOUT_MS = 10_000;

/** The code of a send whose connection the server closed before it completed. */
export const SEND_CLOSED_CODE = "ECONNECTION";

/** The code of a send refused because the server asks for credentials without TLS. */
export const SEND_NO_TLS_CODE = "ENOTLS";

/**
 * Node's codes for a certificate its TLS client did not verify, those a send that checks
 * no revocation list can meet: the X509_V_ERR names Node gives, `UNSPECIFIED` for the
 * errors it has no name for (a signature digest too weak, a key too small), and the name
 * check's. A send that fails for one of them
 * fails with it, not with nodemailer's ESOCKET, which keeps it as `originalCode`: the
 * code points the operator to the remedy, the relay's CA (SPEC-023 §Operational §14,
 * debt #78).
 */
const CERT_VERIFY_CODES: ReadonlySet<string> = new Set([
  "UNABLE_TO_GET_ISSUER_CERT",
  "UNABLE_TO_GET_ISSUER_CERT_LOCALLY",
  "UNABLE_TO_VERIFY_LEAF_SIGNATURE",
  "UNABLE_TO_DECRYPT_CERT_SIGNATURE",
  "UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY",
  "CERT_SIGNATURE_FAILURE",
  "CERT_NOT_YET_VALID",
  "CERT_HAS_EXPIRED",
  "ERROR_IN_CERT_NOT_BEFORE_FIELD",
  "ERROR_IN_CERT_NOT_AFTER_FIELD",
  "DEPTH_ZERO_SELF_SIGNED_CERT",
  "SELF_SIGNED_CERT_IN_CHAIN",
  "CERT_CHAIN_TOO_LONG",
  "CERT_REVOKED",
  "INVALID_CA",
  "PATH_LENGTH_EXCEEDED",
  "INVALID_PURPOSE",
  "CERT_UNTRUSTED",
  "CERT_REJECTED",
  "UNSPECIFIED",
  "ERR_TLS_CERT_ALTNAME_INVALID",
]);

/** A connection error whose cause is the relay's certificate, with its verification's code. */
export function withVerificationCode(err: Error): Error {
  const e = err as { code?: unknown; originalCode?: unknown };
  if (
    e.code === "ESOCKET" &&
    typeof e.originalCode === "string" &&
    CERT_VERIFY_CODES.has(e.originalCode)
  ) {
    e.code = e.originalCode;
  }
  return err;
}

export interface SmtpServer {
  host: string;
  port: number;
  user: string;
  pass: string;
  /**
   * Trusted roots for the relay's certificate: INGEST_SMTP_CA_FILE's certificates, or the
   * tests'. Absent, Node's roots apply (an empty list would replace them with none).
   */
  tls?: { ca: string | string[] };
}

/** The mechanisms a user name and password authenticate with, in nodemailer's order. */
const PASSWORD_MECHANISMS = ["PLAIN", "LOGIN", "CRAM-MD5"];

/**
 * The mechanism the send authenticates with: the first of PLAIN, LOGIN and CRAM-MD5
 * that the server's EHLO reply names as a whole token of its AUTH line, or null to
 * send no credentials (SPEC-022 second Amendment 2026-10-14). Not `allowsAuth`: on
 * HELO nodemailer sets it "assuming" AUTH, and its login then falls back to PLAIN
 * for a server that never asked for credentials. Not nodemailer's own choice
 * either: with no mechanism named, a reply listing only XOAUTH2 makes the login
 * read an OAuth token that is not there, a TypeError thrown where nothing catches
 * it.
 */
function authMechanism(connection: SmtpConnection): string | null {
  if (!Array.isArray(connection._supportedAuth) || connection._supportedAuth.length === 0) {
    return null;
  }
  const advertised = new Set<string>();
  for (const line of connection._ehloLines ?? []) {
    const names = /^AUTH[\s=](.*)$/i.exec(line)?.[1];
    for (const name of names?.trim().split(/[\s=]+/) ?? []) advertised.add(name.toUpperCase());
  }
  return PASSWORD_MECHANISMS.find((m) => advertised.has(m)) ?? null;
}

/**
 * A mailer whose every send opens one SMTP connection, with SPEC-014's TLS
 * (implicit TLS on 465, otherwise STARTTLS when the server offers it), and closes
 * it when the send's time limit runs out. It authenticates only with a mechanism the
 * server's EHLO reply advertised (`authMechanism`), so credentials never go to a
 * server that did not ask for them, and only over TLS: a server that asks for them
 * on a connection in clear text gets none, and the send fails as `ENOTLS` (SPEC-022
 * Amendment 2026-10-15). It takes the envelope from the message's
 * headers, so a sender with a display name still works. A connection the server
 * closes fails the send at once, as `ECONNECTION` (SPEC-014's transport failed it
 * about a second later, with no code); only the send's own limit reads as a timeout.
 */
export function createBoundedSmtpMailer(server: SmtpServer): NotifyMailer {
  return {
    sendMail(message: NotifyMessage, timeoutMs: number): Promise<void> {
      return new Promise<void>((resolve, reject) => {
        const connection = new SMTPConnection({
          host: server.host,
          port: server.port,
          secure: server.port === 465,
          connectionTimeout: SMTP_CONNECTION_TIMEOUT_MS,
          greetingTimeout: SMTP_GREETING_TIMEOUT_MS,
          socketTimeout: SMTP_SOCKET_TIMEOUT_MS,
          ...(server.tls === undefined ? {} : { tls: server.tls }),
        });
        let settled = false;
        const finish = (err?: Error | null) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          try {
            connection.close();
          } catch {
            // already closed
          }
          if (err) reject(err);
          else resolve();
        };
        const timer = setTimeout(() => {
          finish(
            Object.assign(new Error(`timed out after ${timeoutMs} ms`), {
              code: SEND_TIMEOUT_CODE,
            }),
          );
        }, timeoutMs);
        connection.once("error", (err) => finish(withVerificationCode(err)));
        // nodemailer ends a connection the server closed before its greeting without
        // an error; a send still open at that point has failed.
        connection.once("end", () =>
          finish(
            Object.assign(new Error("connection closed before the send completed"), {
              code: SEND_CLOSED_CODE,
            }),
          ),
        );
        const deliver = () => {
          const mail = new MailComposer({
            from: message.from,
            to: message.to,
            subject: message.subject,
            text: message.text,
          }).compile();
          const envelope = mail.getEnvelope();
          mail.build((buildErr, raw) => {
            if (buildErr) return finish(buildErr);
            connection.send({ from: envelope.from || "", to: envelope.to }, raw, (sendErr) => {
              if (sendErr) return finish(sendErr);
              connection.quit();
              finish();
            });
          });
        };
        // A throw here would run inside nodemailer's event handler, where nothing
        // catches it and it ends the process; it fails the send instead.
        connection.connect(() => {
          try {
            const method = authMechanism(connection);
            if (method === null) return deliver();
            if (!connection.secure) {
              return finish(
                Object.assign(
                  new Error("the server asks for credentials on a connection without TLS"),
                  { code: SEND_NO_TLS_CODE },
                ),
              );
            }
            connection.login({ user: server.user, pass: server.pass, method }, (loginErr) => {
              if (loginErr) return finish(loginErr);
              deliver();
            });
          } catch (err) {
            finish(err instanceof Error ? err : new Error(String(err)));
          }
        });
      });
    },
  };
}
