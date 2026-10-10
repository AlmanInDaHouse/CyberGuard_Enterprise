import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import { type LogonEventRow, getLogonEvents, issueToken } from "./helpers/db.js";
import { prepareAgent } from "./helpers/marquee-agent.js";

// SPEC-020 auth_ac_013 — the logon marquee, end to end. The real cg-agent binary
// on its normal path against the real stack (testcontainers + the in-process
// ingest server). The test removes any connection to \\127.0.0.1, runs
// `net use \\127.0.0.1\IPC$` once with a random user name and password, which
// fails, and once with the current credentials, which succeeds, and removes the
// connection. Among that agent's 3002 rows in cges_events, read with FINAL, there
// is a failure whose user_name is <withheld> and whose status_detail is
// 0xc0000064, and a success whose user_uid is the current user's SID and whose
// logon_type_id is 3. The diagnostic output is redacted (SPEC-020 §Operational
// §11): no name, a SID as prefix and RID. The elapsed time is logged on every run
// and must stay within 45 s (NFR-020-003). Windows only, elevated: an unelevated
// agent exits with code 9.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

const SHARE = "\\\\127.0.0.1\\IPC$";

/** Run net.exe; its exit status, never its output (which may name the user). */
function net(args: string[]): number | null {
  return spawnSync("net", args, { stdio: "ignore", windowsHide: true }).status;
}

/** The current user's SID, from `whoami /user`. */
function currentUserSid(): string {
  const out = spawnSync("whoami", ["/user", "/fo", "csv", "/nh"], { encoding: "utf-8" });
  return (out.stdout.trim().split(",").pop() ?? "").replace(/"/g, "");
}

/** A SID as the gate may print it: authority and first sub-authority, then the RID. */
function redactSid(sid: string): string {
  const parts = sid.split("-");
  return parts.length <= 5 ? sid : `${parts.slice(0, 4).join("-")}-...-${parts.at(-1)}`;
}

/** Remove every connection to \\127.0.0.1 that `net use` lists. */
function removeLoopbackConnections(): void {
  const listing = spawnSync("net", ["use"], { encoding: "utf-8" }).stdout ?? "";
  for (const token of listing.split(/\s+/)) {
    if (token.toLowerCase().startsWith("\\\\127.0.0.1\\")) {
      net(["use", token, "/delete", "/y"]);
    }
  }
}

/** A row as the gate may print it. */
function redactRow(r: LogonEventRow): Record<string, unknown> {
  return {
    status_id: r.status_id,
    user_uid: redactSid(r.user_uid),
    name_withheld: r.user_name === "<withheld>",
    logon_type_id: r.logon_type_id,
    status_code: r.status_code,
    status_detail: r.status_detail,
    auth_protocol: r.auth_protocol,
    source_present: r.src_ip !== "",
    elevated_token: r.elevated_token,
  };
}

test.skipIf(process.platform !== "win32")(
  "auth_ac_013 marquee: a refused and an accepted logon are stored as 3002 rows",
  async () => {
    const marqueeStartMs = Date.now();
    let elapsedSeconds = Number.NaN;
    let elapsedLogged = false;
    const logElapsed = (): void => {
      elapsedSeconds = (Date.now() - marqueeStartMs) / 1000;
      elapsedLogged = true;
      console.info(
        JSON.stringify({
          event: "auth_ac_013_marquee_complete",
          marquee_elapsed_seconds: elapsedSeconds,
          budget_seconds: 45,
          within_budget: elapsedSeconds <= 45,
        }),
      );
    };
    try {
      const token = await issueToken(config);
      const agent = prepareAgent({
        enrollUrl: server.enrollUrl,
        heartbeatUrl: server.heartbeatUrl,
        caCertPem: server.caCertPem,
        token,
      });

      // The agent enrolls, opens its sources and delivers within the window;
      // the logons happen once it has reached steady state.
      const runPromise = agent.run(25_000);
      await new Promise((resolve) => setTimeout(resolve, 5_000));

      const random = globalThis.crypto.randomUUID().replace(/-/g, "");
      removeLoopbackConnections();
      const refused = net([
        "use",
        SHARE,
        `Cg-${random}!`,
        `/user:cg-nouser-${random.slice(0, 12)}`,
      ]);
      const accepted = net(["use", SHARE]);
      removeLoopbackConnections();
      const sid = currentUserSid();

      const result = await runPromise;
      console.info(
        JSON.stringify({
          diag_event: "auth_ac_013_agent",
          exit_code: result.exitCode,
          net_refused_status: refused,
          net_accepted_status: accepted,
        }),
      );

      const identity = JSON.parse(
        readFileSync(join(agent.identityDir, "identity.json"), "utf-8"),
      ) as {
        agent_id: string;
      };
      const rows = await getLogonEvents(config, identity.agent_id);
      logElapsed();
      console.info(
        JSON.stringify({
          diag_event: "auth_ac_013_rows",
          current_user: redactSid(sid),
          logon_rows_total: rows.length,
          rows: rows.map(redactRow),
        }),
      );

      expect(refused, "the logon with a random name must be refused").not.toBe(0);
      expect(accepted, "net use with the current credentials must succeed").toBe(0);
      const failure = rows.find(
        (r) =>
          r.status_id === 2 && r.user_name === "<withheld>" && r.status_detail === "0xc0000064",
      );
      expect(failure, "a withheld failure with 0xc0000064").toBeDefined();
      expect(failure?.user_domain).toBe("<withheld>");
      const success = rows.find(
        (r) => r.status_id === 1 && r.user_uid === sid && r.logon_type_id === 3,
      );
      expect(success, "a success of the current user, logon type 3").toBeDefined();
      expect(elapsedSeconds).toBeLessThanOrEqual(45);
    } finally {
      // NFR-020-003: logged on every run, a failed one included.
      if (!elapsedLogged) {
        logElapsed();
      }
    }
  },
  60_000,
);
