import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

/** @type {import('next').NextConfig} */
const nextConfig = {
  // We lint with Biome (ts-ci `check-dashboard`, mirroring `check-api`), not ESLint,
  // so Next's build-time ESLint pass is disabled. Type errors still fail the build.
  eslint: { ignoreDuringBuilds: true },
  // The repo is a pnpm workspace; pin the file-tracing root to the monorepo root so
  // Next does not mis-infer it from an unrelated lockfile elsewhere on the machine.
  outputFileTracingRoot: repoRoot,
};

export default nextConfig;
