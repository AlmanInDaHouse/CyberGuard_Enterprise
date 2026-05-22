import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
    // Integration tests (B4) spin up testcontainers; give them room.
    testTimeout: 120_000,
    hookTimeout: 120_000,
    pool: "forks",
  },
});
