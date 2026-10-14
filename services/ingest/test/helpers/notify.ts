import type { NotifyConfig, NotifyMessage } from "../../src/notify/index.js";

export interface NotifySpy {
  /** Inject as the optional 2nd arg of runDetectionCycle. */
  notify: NotifyConfig;
  /** Messages the fake transport accepted (empty when `throwOnSend`). */
  sent: NotifyMessage[];
  /** Log lines the fire-and-forget failure path emitted to the sink. */
  logs: { obj: Record<string, unknown>; msg: string }[];
}

/** Which messages a behaviour applies to: all of them, or those the predicate picks. */
type Which = boolean | ((message: NotifyMessage) => boolean);

const applies = (which: Which | undefined, message: NotifyMessage): boolean =>
  typeof which === "function" ? which(message) : which === true;

export interface SpyOptions {
  /** Reject every send: with this error, or with a generic one when `true`. */
  throwOnSend?: boolean | Error;
  /** Never settle the sends it picks (the caller's own bound must cut them). */
  hang?: Which;
  /** Delay the sends it picks by `ms` before accepting them. */
  delay?: { ms: number; which?: Which };
  /** Run before a send is accepted. */
  onSend?: () => Promise<void>;
}

/**
 * A fake, in-memory NotifyConfig for the notify_ac and soar_ac suites (SPEC-014,
 * SPEC-022): captures sent messages and log lines, opens NO socket. No nodemailer,
 * no network — CI-able. The time limit a caller passes is ignored, as a send that
 * hangs shows the caller's own bound.
 */
export function spyNotify(opts: SpyOptions = {}): NotifySpy {
  const sent: NotifyMessage[] = [];
  const logs: { obj: Record<string, unknown>; msg: string }[] = [];
  const notify: NotifyConfig = {
    from: "soc-noreply@cyberguard.test",
    recipient: "soc-team@cyberguard.test",
    mailer: {
      sendMail: async (message) => {
        if (opts.throwOnSend instanceof Error) throw opts.throwOnSend;
        if (opts.throwOnSend) {
          throw new Error("smtp unreachable (fake)");
        }
        if (applies(opts.hang, message)) await new Promise(() => {});
        if (opts.delay && applies(opts.delay.which ?? true, message)) {
          await new Promise((resolve) => setTimeout(resolve, opts.delay?.ms));
        }
        if (opts.onSend) await opts.onSend();
        sent.push(message);
        return { messageId: "fake-test-message" };
      },
    },
    log: {
      warn: (obj, msg) => {
        logs.push({ obj, msg });
      },
    },
  };
  return { notify, sent, logs };
}

/** Picks a playbook's email: its subject does not start like SPEC-014's announcement. */
export const playbookMail = (message: NotifyMessage): boolean =>
  !message.subject.startsWith("[CyberGuard] New incident");
