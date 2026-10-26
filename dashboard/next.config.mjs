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
  // The image runs Next's standalone server (SPEC-023 §Data contracts §2); with the tracing
  // root above it is .next/standalone/dashboard/server.js. Its tree links the pnpm store
  // with symbolic links, which Windows refuses to an unprivileged process, so a Windows
  // checkout builds without it; the image and CI build on Linux.
  output: process.platform === "win32" ? undefined : "standalone",
  // The dashboard renders no optimised image, and its image carries no image-processing
  // library (sharp is left out of the install, SPEC-023 Amendment 2026-10-26, item 3): the
  // optimiser's route, /_next/image, answers 404.
  images: { unoptimized: true },
};

export default nextConfig;
