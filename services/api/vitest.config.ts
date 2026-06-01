import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
    // Shared Postgres + Redis backends are started once for the whole suite.
    globalSetup: ["./test/global-setup.ts"],
    // Integration tests spin up testcontainers; give them room.
    testTimeout: 120_000,
    hookTimeout: 180_000,
    // Single fork: the suite shares one set of containers.
    pool: "forks",
    poolOptions: { forks: { singleFork: true } },
  },
});
