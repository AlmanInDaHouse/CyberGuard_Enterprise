import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { type AddressInfo, type Socket, createServer } from "node:net";
import { join } from "node:path";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { type IngestServer, startIngest } from "../src/server.js";
import {
  type CgesEventRow,
  type NetworkEventRow,
  getCgesEvents,
  getNetworkEvents,
  issueToken,
} from "./helpers/db.js";
import { prepareAgent } from "./helpers/marquee-agent.js";

// SPEC-019 net_ac_010 — the network marquee, end to end. The real cg-agent
// binary on its normal path against the real stack (testcontainers + the
// in-process ingest server). A curl.exe probe connects to a listener this
// test process holds on 127.0.0.1. Among that agent's 4001 rows in cges_events,
// read with FINAL, those whose dst_port is the listener's port and whose
// src_port is the port the listener saw as its peer's are exactly two: one
// outbound with the probe's PID and one inbound with this test process's PID,
// both with the probe's address as src_ip and the listener's as dst_ip
// (ADR-0018 §4). The outbound row's process_uid equals the process_uid of the
// probe's Launch row. No 4001 row of that agent carries the agent's own PID.
// The elapsed time is logged on every run and must stay within 45 s
// (NFR-019-003). Windows only, elevated: an unelevated agent exits with code 9.

let config: Config;
let server: IngestServer;

beforeAll(async () => {
  config = inject("ingestConfig");
  server = await startIngest(config);
});

afterAll(async () => {
  await server?.close();
});

/** The peer address as the agent writes it: an IPv4-mapped IPv6 address as IPv4. */
function plainIp(address: string | undefined): string | undefined {
  return address?.startsWith("::ffff:") ? address.slice("::ffff:".length) : address;
}

test.skipIf(process.platform !== "win32")(
  "net_ac_010 marquee: a real connection is stored as one outbound and one inbound 4001 row",
  async () => {
    const marqueeStartMs = Date.now();
    const token = await issueToken(config);
    const agent = prepareAgent({
      enrollUrl: server.enrollUrl,
      heartbeatUrl: server.heartbeatUrl,
      caCertPem: server.caCertPem,
      token,
    });

    // The agent enrolls, opens its ETW session and delivers within the window;
    // the probe runs once it has reached steady state.
    const runPromise = agent.run(25_000);
    await new Promise((resolve) => setTimeout(resolve, 5_000));

    // The listener: answers a minimal HTTP response and closes; keeps the peer.
    let peerPort: number | undefined;
    let peerAddress: string | undefined;
    const listener = createServer((socket: Socket) => {
      peerPort = socket.remotePort;
      peerAddress = plainIp(socket.remoteAddress);
      socket.once("data", () => {
        socket.end("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
      });
      socket.on("error", () => undefined);
    });
    await new Promise<void>((resolve) => listener.listen(0, "127.0.0.1", resolve));
    const listenerPort = (listener.address() as AddressInfo).port;

    const probe = spawn("curl.exe", ["-g", "-s", "-m", "5", `http://127.0.0.1:${listenerPort}/`], {
      stdio: "ignore",
    });
    const probePid = probe.pid;
    if (probePid === undefined) {
      throw new Error("net_ac_010: the probe must start");
    }
    const probeExit = await new Promise<number | null>((resolve) => probe.on("exit", resolve));
    await new Promise<void>((resolve) => listener.close(() => resolve()));

    const result = await runPromise;
    console.info(
      JSON.stringify({
        diag_event: "net_ac_010_agent_stderr_full",
        exit_code: result.exitCode,
        stderr: result.stderr,
      }),
    );

    let agentId: string | undefined;
    let rows: NetworkEventRow[] = [];
    let launches: CgesEventRow[] = [];
    let elapsedSeconds = Number.NaN;
    try {
      const identity = JSON.parse(
        readFileSync(join(agent.identityDir, "identity.json"), "utf-8"),
      ) as { agent_id: string };
      agentId = identity.agent_id;
      rows = await getNetworkEvents(config, agentId);
      launches = await getCgesEvents(config, agentId);
    } finally {
      elapsedSeconds = (Date.now() - marqueeStartMs) / 1000;
      console.info(
        JSON.stringify({
          event: "net_ac_010_marquee_complete",
          marquee_elapsed_seconds: elapsedSeconds,
          budget_seconds: 45,
          within_budget: elapsedSeconds <= 45,
        }),
      );
    }

    const testPid = process.pid;
    const connection = rows.filter((r) => r.dst_port === listenerPort && r.src_port === peerPort);
    // What a failure needs: the endpoints expected, and every 4001 row that
    // touches the listener's port, the peer's port or one of the three PIDs.
    console.info(
      JSON.stringify({
        diag_event: "net_ac_010_rows",
        expected: {
          listener: `127.0.0.1:${listenerPort}`,
          peer: `${peerAddress}:${peerPort}`,
          probe_pid: probePid,
          probe_exit: probeExit,
          test_pid: testPid,
          agent_pid: result.pid,
        },
        network_rows_total: rows.length,
        relevant: rows.filter(
          (r) =>
            r.dst_port === listenerPort ||
            r.src_port === listenerPort ||
            r.src_port === peerPort ||
            r.dst_port === peerPort ||
            [probePid, testPid, result.pid].includes(r.process_pid),
        ),
      }),
    );

    expect(agentId).toBeDefined();
    expect(peerPort, "the listener must see the probe's connection").toBeDefined();
    expect(connection).toHaveLength(2);
    const outbound = connection.find((r) => r.net_direction === "outbound");
    const inbound = connection.find((r) => r.net_direction === "inbound");
    expect(outbound?.process_pid).toBe(probePid);
    expect(inbound?.process_pid).toBe(testPid);
    for (const row of [outbound, inbound]) {
      expect(row?.src_ip).toBe(peerAddress);
      expect(row?.dst_ip).toBe("127.0.0.1");
      expect(row?.net_protocol).toBe("tcp");
      expect(row?.activity_id).toBe(1);
    }

    const probeLaunch = launches.find((r) => r.activity_id === 1 && r.process_pid === probePid);
    expect(probeLaunch, "the probe's Launch row").toBeDefined();
    expect(outbound?.process_uid).toBe(probeLaunch?.process_uid);

    expect(result.pid).toBeDefined();
    expect(rows.filter((r) => r.process_pid === result.pid)).toEqual([]);
    expect(elapsedSeconds).toBeLessThanOrEqual(45);
  },
  60_000,
);
