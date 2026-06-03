import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    // Render tests run under jsdom; the data-access integration test overrides to
    // `node` via a `// @vitest-environment node` directive (it drives testcontainers
    // + the real services/api app in-process, no DOM).
    environment: "jsdom",
    include: ["test/**/*.test.{ts,tsx}"],
    // The data-access integration test spins up Postgres + Redis via testcontainers
    // (the same pattern as services/api) — give it room.
    testTimeout: 120_000,
    hookTimeout: 180_000,
    // Single fork: the data-access test shares one set of containers.
    pool: "forks",
    poolOptions: { forks: { singleFork: true } },
  },
});
