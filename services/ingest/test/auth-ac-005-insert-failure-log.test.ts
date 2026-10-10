import { expect, test } from "vitest";
import { insertFailureLogFields } from "../src/routes/heartbeat.js";

// SPEC-020 auth_ac_005 — no row in the server's log (§Operational §7). ClickHouse
// quotes the offending row in the message of an INSERT error; the route's
// insert-failure log line is built from the error's code and name only, for every
// class, so a logon's user name (or a process command line) never reaches it.

const DISTINCTIVE = "auth-ac-005-distinctive-user";

/** An error shaped as @clickhouse/client raises one for a row it cannot parse. */
function clickHouseError(): Error {
  const e = new Error(
    `Cannot parse input: expected '"' before: '{"user_name":"${DISTINCTIVE}","user_domain":"WS-0042"}': (at row 3)`,
  ) as Error & { code: string; type: string };
  e.name = "ClickHouseError";
  e.code = "27";
  e.type = "CANNOT_PARSE_INPUT_ASSERTION_FAILED";
  return e;
}

test("auth_ac_005: the insert-failure log fields carry the error's code and name, not the row", () => {
  const fields = insertFailureLogFields(clickHouseError());
  const text = JSON.stringify(fields);
  expect(text).not.toContain(DISTINCTIVE);
  expect(text).not.toContain("WS-0042");
  expect(fields).toEqual({
    err_name: "ClickHouseError",
    err_code: "27",
    err_type: "CANNOT_PARSE_INPUT_ASSERTION_FAILED",
  });
});

test("auth_ac_005: an error without a code is logged by name alone", () => {
  const fields = insertFailureLogFields(new TypeError(`bad row ${DISTINCTIVE}`));
  expect(JSON.stringify(fields)).not.toContain(DISTINCTIVE);
  expect(fields).toEqual({ err_name: "TypeError", err_code: null, err_type: null });
});
