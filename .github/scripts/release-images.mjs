#!/usr/bin/env node
// SPEC-023 §Operational §12 and its Amendment 2026-10-26, item 4: the release's image builds,
// which release.yml pushes and deploy-ci.yml's dry run keeps on disk. Each of the four images
// is built from the repository's root for linux/amd64, with BuildKit's SBOM and provenance
// attestations, tagged `<version>` and `sha-<commit>`, in a docker-container builder whose
// BuildKit image, like the SBOM generator, is pinned by digest. It writes <out>/digests.json,
// each image's index digest: the one the bundle pins and the attestations name.
//
//   node .github/scripts/release-images.mjs --version <v> --revision <full sha>
//     --mode push|oci --out <dir>
//
// push: to ghcr.io (the caller has logged in). oci: each image as an OCI layout in
// <out>/<name>/, pushed nowhere.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

export const REGISTRY = "ghcr.io/almanindahouse";
export const IMAGES = [
  { name: "ingest", dockerfile: "services/ingest/Dockerfile" },
  { name: "api", dockerfile: "services/api/Dockerfile" },
  { name: "dashboard", dockerfile: "dashboard/Dockerfile" },
  { name: "proxy", dockerfile: "deploy/docker/proxy/Dockerfile" },
];
/** BuildKit v0.34.0, the image the builder runs. */
export const BUILDKIT =
  "moby/buildkit:v0.34.0@sha256:b059f8d7226d0b326bc871af489a5dddf65e36af4d20d14251b072974d9ee87c";
/** The SBOM generator, docker/buildkit-syft-scanner 1.12.0. */
export const SBOM_GENERATOR =
  "docker/buildkit-syft-scanner:1.12.0@sha256:ae4f3b554449e7e25548e7d8ccc029d17357348e30c6e3df01b92bc93654d6a9";
const BUILDER = "cg-release";

/** The tag a release version may have (`<major>.<minor>.<patch>`, or the dry run's). */
const VERSION = /^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$/;

/** The `docker buildx build` arguments for one image. */
export function buildArgs({ name, dockerfile }, { version, revision, mode, out }) {
  const ref = `${REGISTRY}/cyberguard-${name}`;
  return [
    "buildx",
    "build",
    "--builder",
    BUILDER,
    "--platform",
    "linux/amd64",
    "--build-arg",
    `CG_VERSION=${version}`,
    "--build-arg",
    `CG_REVISION=${revision}`,
    "--file",
    dockerfile,
    "--tag",
    `${ref}:${version}`,
    "--tag",
    `${ref}:sha-${revision}`,
    "--attest",
    `type=sbom,generator=${SBOM_GENERATOR}`,
    "--attest",
    "type=provenance,mode=max",
    "--metadata-file",
    join(out, `${name}.metadata.json`),
    ...(mode === "push" ? ["--push"] : ["--output", `type=oci,dest=${join(out, name)},tar=false`]),
    ".",
  ];
}

function arg(name) {
  const i = process.argv.indexOf(name);
  if (i === -1) return undefined;
  const v = process.argv[i + 1];
  if (v === undefined || v.startsWith("--")) throw new Error(`${name} needs a value`);
  return v;
}

const docker = (args, opts = {}) =>
  execFileSync("docker", args, { cwd: REPO, stdio: ["ignore", "inherit", "inherit"], ...opts });

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  try {
    const version = arg("--version");
    const revision = arg("--revision");
    const mode = arg("--mode");
    const out = arg("--out");
    if (!version || !VERSION.test(version)) throw new Error(`--version: not a release version: ${version}`);
    if (!revision || !/^[0-9a-f]{40}$/.test(revision)) throw new Error("--revision: not a full commit SHA");
    if (mode !== "push" && mode !== "oci") throw new Error("--mode: push or oci");
    if (!out) throw new Error("--out is required");
    mkdirSync(out, { recursive: true });
    try {
      execFileSync("docker", ["buildx", "inspect", BUILDER], { stdio: "ignore" });
    } catch {
      docker(["buildx", "create", "--name", BUILDER, "--driver", "docker-container", "--driver-opt", `image=${BUILDKIT}`]);
    }
    const digests = {};
    for (const image of IMAGES) {
      docker(buildArgs(image, { version, revision, mode, out }));
      const metadata = JSON.parse(readFileSync(join(out, `${image.name}.metadata.json`), "utf8"));
      const digest = metadata["containerimage.digest"];
      if (!/^sha256:[0-9a-f]{64}$/.test(digest ?? "")) throw new Error(`${image.name}: no digest in the build's metadata`);
      if (mode === "oci") {
        // The layout's index names the image's own index, which is what a push would pin.
        const index = JSON.parse(readFileSync(join(out, image.name, "index.json"), "utf8"));
        if (index.manifests?.[0]?.digest !== digest) {
          throw new Error(`${image.name}: the layout's index names ${index.manifests?.[0]?.digest}, not ${digest}`);
        }
      }
      digests[image.name] = digest;
    }
    writeFileSync(join(out, "digests.json"), `${JSON.stringify(digests, null, 2)}\n`);
    process.stdout.write(`${JSON.stringify(digests)}\n`);
  } catch (e) {
    process.stderr.write(`release images: ${e instanceof Error ? e.message : String(e)}\n`);
    process.exit(1);
  }
}
