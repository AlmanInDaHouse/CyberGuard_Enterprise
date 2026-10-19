import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
    // Shared PG/CH/Redis backends are started once for the whole suite.
    globalSetup: ["./test/global-setup.ts"],
    // Integration tests spin up testcontainers; give them room.
    testTimeout: 120_000,
    hookTimeout: 180_000,
    // One test file at a time, each in a process of its own: the suite shares
    // one set of containers, and the marquees run a real agent against them.
    pool: "forks",
    fileParallelism: false,
  },
});
