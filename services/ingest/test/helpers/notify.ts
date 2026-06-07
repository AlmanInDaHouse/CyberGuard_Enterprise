import type { NotifyConfig, NotifyMessage } from "../../src/notify/index.js";

export interface NotifySpy {
  /** Inject as the optional 2nd arg of runDetectionCycle. */
  notify: NotifyConfig;
  /** Messages the fake transport accepted (empty when `throwOnSend`). */
  sent: NotifyMessage[];
  /** Log lines the fire-and-forget failure path emitted to the sink. */
  logs: { obj: Record<string, unknown>; msg: string }[];
}

/**
 * A fake, in-memory NotifyConfig for the notify_ac suite (SPEC-014 §AC): captures
 * sent messages + log lines, opens NO socket. `throwOnSend` makes the transport
 * reject (fire-and-forget AC). No nodemailer, no network — CI-able.
 */
export function spyNotify(opts: { throwOnSend?: boolean } = {}): NotifySpy {
  const sent: NotifyMessage[] = [];
  const logs: { obj: Record<string, unknown>; msg: string }[] = [];
  const notify: NotifyConfig = {
    from: "soc-noreply@cyberguard.test",
    recipient: "soc-team@cyberguard.test",
    mailer: {
      sendMail: async (message) => {
        if (opts.throwOnSend) {
          throw new Error("smtp unreachable (fake)");
        }
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
