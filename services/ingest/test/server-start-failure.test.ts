import { mkdtempSync, rmSync } from "node:fs";
import type { AddressInfo, Server } from "node:net";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, inject, test } from "vitest";
import { startIngest } from "../src/server.js";

// Debt #19 (docs/handoff-session-28.md): when startIngest fails after a
// listener is bound, the failure closes that listener as well as the
// services, so an in-process caller is not left holding the port.

/** Bind 127.0.0.1 (the test config's INGEST_BIND_HOST) on `port`, 0 for any. */
function hold(port = 0): Promise<Server> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port }, () => resolve(server));
  });
}

function portOf(server: Server): number {
  return (server.address() as AddressInfo).port;
}

function release(server: Server): Promise<void> {
  return new Promise((resolve) => server.close(() => resolve()));
}

/** A port that was free a moment ago. */
async function freePort(): Promise<number> {
  const probe = await hold();
  const port = portOf(probe);
  await release(probe);
  return port;
}

test("a failed heartbeat listen closes the enroll listener it follows", async () => {
  const config = inject("ingestConfig");
  const enrollPort = await freePort();
  const taken = await hold();
  try {
    await expect(
      startIngest({
        ...config,
        INGEST_ENROLL_PORT: enrollPort,
        INGEST_HEARTBEAT_PORT: portOf(taken),
      }),
    ).rejects.toThrow(`EADDRINUSE: address already in use 127.0.0.1:${portOf(taken)}`);

    // The enroll listener bound before the failure is closed: its port binds again.
    await release(await hold(enrollPort));
  } finally {
    await release(taken);
  }
});

test("the driver's fail-loud refusal leaves both ports free", async () => {
  const config = inject("ingestConfig");
  const enrollPort = await freePort();
  const heartbeatPort = await freePort();
  const emptyRules = mkdtempSync(join(tmpdir(), "cg-no-rules-"));
  try {
    await expect(
      startIngest({
        ...config,
        INGEST_ENROLL_PORT: enrollPort,
        INGEST_HEARTBEAT_PORT: heartbeatPort,
        INGEST_DETECT_INTERVAL_MS: 10_000,
        INGEST_DETECT_RULES_DIR: emptyRules,
      }),
    ).rejects.toThrow("refusing to start a detection-dark driver");

    // The rules are read before a listener opens (SPEC-023 §Operational §2,
    // deploy_ac_013); both ports bind again.
    await release(await hold(enrollPort));
    await release(await hold(heartbeatPort));
  } finally {
    rmSync(emptyRules, { recursive: true, force: true });
  }
});
