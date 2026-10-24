// SPEC-022 §Operational §4 / NFR-022-003 — how a failed send is reported. A pure
// module: notify/index.ts and the SOAR run use it without loading nodemailer.

/** The code a send rejects with when its time limit runs out. */
export const SEND_TIMEOUT_CODE = "ESENDLIMIT";

/**
 * How a failed send is reported, in a record or a log line: the error's code and
 * the server's numeric response code, never the message, which can quote the
 * recipient (SPEC-022 §Operational §4, NFR-022-003).
 */
export function sendFailureCode(err: unknown): string {
  if (err === null || typeof err !== "object") return "unknown";
  const { code, responseCode } = err as { code?: unknown; responseCode?: unknown };
  const parts: string[] = [];
  // 40: the longest certificate verification code is 34 (SPEC-023 §Operational §14).
  if (typeof code === "string" && /^[A-Z0-9_]{1,40}$/.test(code)) parts.push(code);
  if (typeof responseCode === "number" && Number.isInteger(responseCode)) {
    parts.push(String(responseCode));
  }
  return parts.length > 0 ? parts.join(" ") : "unknown";
}

/** Whether a send failed because its time limit ran out. */
export function isSendTimeout(err: unknown): boolean {
  return (
    err !== null &&
    typeof err === "object" &&
    (err as { code?: unknown }).code === SEND_TIMEOUT_CODE
  );
}
