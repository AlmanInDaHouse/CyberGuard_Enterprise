import type { AddressInfo, Server } from "node:net";
import { createServer } from "node:net";
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

test("a failed heartbeat listen closes the enroll listener it follows", async () => {
  const config = inject("ingestConfig");
  const probe = await hold();
  const enrollPort = portOf(probe);
  await release(probe);
  const taken = await hold();
  try {
    await expect(
      startIngest({
        ...config,
        INGEST_ENROLL_PORT: enrollPort,
        INGEST_HEARTBEAT_PORT: portOf(taken),
      }),
    ).rejects.toThrow(/EADDRINUSE/);

    // The enroll listener bound before the failure is closed: its port binds again.
    await release(await hold(enrollPort));
  } finally {
    await release(taken);
  }
});
