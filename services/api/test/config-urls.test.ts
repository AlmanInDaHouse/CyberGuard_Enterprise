import { expect, test } from "vitest";
import { ConfigError, loadConfigFor } from "../src/config.js";

// The store URLs as zod 3.25.76 read them, kept across the upgrade to zod 4: whatever
// `new URL()` parses is accepted, and the value reaches the clients as it was set. zod 4's
// `.url()` would trim it and delete its tabs and line breaks, accepting values zod 3 refused
// (a leading no-break space) and handing the clients another string. Each expected value is
// what zod 3 answered.

const CASES: [string, boolean][] = [
  ["postgres://u:p@h:5432/db", true],
  ["postgres://h/db\r", true],
  ["postgres://u:pa\tss@h/db", true],
  [" postgres://h/db", true],
  [" postgres://h/db", false],
  ["not a url", false],
];

test.each(CASES)("API_PG_URL=%j is accepted: %s, and kept as written", (value, accepted) => {
  for (const name of ["API_PG_URL", "API_REDIS_URL", "API_CH_URL"] as const) {
    if (accepted) {
      expect(loadConfigFor([name], { [name]: value })[name]).toBe(value);
    } else {
      expect(() => loadConfigFor([name], { [name]: value })).toThrow(
        new ConfigError(name, "must be a URL"),
      );
    }
  }
});

// The other reasons the checks give, which zod 4's issues carry otherwise than zod 3's.
test("an unset variable is required; an empty one must not be empty; a level must be one of pino's", () => {
  expect(() => loadConfigFor(["API_DB_ENC_PASSPHRASE"], {})).toThrow(
    new ConfigError("API_DB_ENC_PASSPHRASE", "required"),
  );
  expect(() => loadConfigFor(["API_DB_ENC_PASSPHRASE"], { API_DB_ENC_PASSPHRASE: "" })).toThrow(
    new ConfigError("API_DB_ENC_PASSPHRASE", "must not be empty"),
  );
  expect(() => loadConfigFor(["API_LOG_LEVEL"], { API_LOG_LEVEL: "loud" })).toThrow(
    new ConfigError(
      "API_LOG_LEVEL",
      "must be one of fatal, error, warn, info, debug, trace, silent",
    ),
  );
});
