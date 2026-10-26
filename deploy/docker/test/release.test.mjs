// SPEC-023 §Operational §12, its Amendment 2026-10-26, item 4: the release's bundle step and
// image builds. Its first part tests the bundle step's rewrites on the repository's files.
// Its second reads the release dry run that deploy-ci.yml writes before this file runs
// (CG_TEST_RELEASE_DIR, with CG_TEST_RELEASE_VERSION): deploy_ac_019's dry-run half (each
// image built with BuildKit's SBOM and provenance attached; the bundle) and deploy_ac_001's
// clause over the published bundle's compose (every image pinned by digest).
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, test } from "node:test";
import { LEFT_OUT, rewriteCompose, rewriteReadme } from "../../../.github/scripts/release-bundle.mjs";
import { IMAGES, REGISTRY } from "../../../.github/scripts/release-images.mjs";
import { BUNDLE, ok, REPO, run, SECRETS } from "./lib.mjs";

const DIR = process.env.CG_TEST_RELEASE_DIR;
const VERSION = process.env.CG_TEST_RELEASE_VERSION;
const fakeDigest = (name) => `sha256:${createHash("sha256").update(name).digest("hex")}`;
const FAKE = Object.fromEntries(IMAGES.map(({ name }) => [name, fakeDigest(name)]));

describe("the bundle step's rewrites", () => {
  const compose = readFileSync(join(BUNDLE, "docker-compose.yml"), "utf8");

  test("names each product image by version and digest, and leaves the stores' as they are", () => {
    const out = rewriteCompose(compose, "1.2.3", FAKE);
    const images = [...out.matchAll(/^\s*image:\s*(\S+)\s*$/gm)].map((m) => m[1]);
    const before = [...compose.matchAll(/^\s*image:\s*(\S+)\s*$/gm)].map((m) => m[1]);
    assert.equal(images.length, before.length);
    for (const [i, image] of images.entries()) {
      const product = /\/cyberguard-([a-z]+):/.exec(before[i]);
      if (product) {
        assert.equal(image, `${REGISTRY}/cyberguard-${product[1]}:1.2.3@${FAKE[product[1]]}`);
      } else {
        assert.equal(image, before[i]);
      }
    }
    // The six product services, the two one-shots among them, and each of the four images.
    assert.equal(images.filter((i) => i.startsWith(`${REGISTRY}/`)).length, 6);
    for (const { name } of IMAGES) assert.ok(images.some((i) => i.includes(`/cyberguard-${name}:1.2.3@`)));
  });

  test("refuses a missing digest, and a compose that names an image nowhere", () => {
    const { proxy: _, ...noProxy } = FAKE;
    assert.throws(() => rewriteCompose(compose, "1.2.3", noProxy), /no digest for proxy/);
    const withoutApi = compose.replace(/^.*cyberguard-api:.*$/gm, "");
    assert.throws(() => rewriteCompose(withoutApi, "1.2.3", FAKE), /names no api image/);
  });

  test("refuses an image left without a digest", () => {
    const extra = `${compose}\n  extra:\n    image: example/unpinned:1\n`;
    assert.throws(() => rewriteCompose(extra, "1.2.3", FAKE), /not pinned by digest: example\/unpinned:1/);
  });

  test("release.yml's and deploy-ci.yml's third-party actions are pinned by commit SHA (§Operational §12)", () => {
    for (const workflow of ["release.yml", "deploy-ci.yml"]) {
      const text = readFileSync(join(REPO, ".github", "workflows", workflow), "utf8");
      const uses = [...text.matchAll(/^\s*(?:-\s*)?uses:\s*(\S+)/gm)].map((m) => m[1]);
      assert.ok(uses.length > 0, `${workflow} uses no action`);
      for (const action of uses) assert.match(action, /^[\w.-]+\/[\w.-]+(\/[\w./-]+)?@[0-9a-f]{40}$/, `${workflow}: ${action}`);
    }
  });

  test("points the README at the release's tag", () => {
    const out = rewriteReadme(readFileSync(join(BUNDLE, "README.md"), "utf8"), "1.2.3");
    assert.ok(out.includes("/blob/v1.2.3/docs/operations/install.md"));
    assert.ok(!out.includes("<version>"));
  });
});

/** An OCI layout's blob, by digest. */
function blob(layout, digest) {
  return readFileSync(join(layout, "blobs", ...digest.split(":")));
}
const json = (layout, digest) => JSON.parse(blob(layout, digest).toString("utf8"));

describe("the release dry run", { skip: !DIR && "no release dry run (CG_TEST_RELEASE_DIR)" }, () => {
  const digests = DIR ? JSON.parse(readFileSync(join(DIR, "digests.json"), "utf8")) : {};
  const bundle = DIR ? join(DIR, `cyberguard-deploy-${VERSION}.tar.gz`) : "";

  for (const { name } of IMAGES) {
    test(`deploy_ac_019: the ${name} image carries an SBOM and a provenance attestation`, () => {
      const layout = join(DIR, name);
      const top = JSON.parse(readFileSync(join(layout, "index.json"), "utf8"));
      assert.equal(top.manifests[0].digest, digests[name]);
      const index = json(layout, digests[name]);
      const image = index.manifests.find((m) => m.platform?.os === "linux" && m.platform?.architecture === "amd64");
      assert.ok(image, "no linux/amd64 image in the index");
      const attestation = index.manifests.find(
        (m) =>
          m.annotations?.["vnd.docker.reference.type"] === "attestation-manifest" &&
          m.annotations?.["vnd.docker.reference.digest"] === image.digest,
      );
      assert.ok(attestation, "no attestation manifest for the image");
      const layers = json(layout, attestation.digest).layers;
      const types = layers.map((l) => l.annotations?.["in-toto.io/predicate-type"]);
      assert.ok(types.includes("https://spdx.dev/Document"), `predicates: ${types}`);
      assert.ok(types.some((t) => /^https:\/\/slsa\.dev\/provenance\//.test(t ?? "")), `predicates: ${types}`);
      // The labels the release sets from its version and its commit.
      const config = json(layout, json(layout, image.digest).config.digest);
      // The labels of §Data contracts §2 and no other of their prefix, as the release builds
      // them (Amendment 2026-10-26, item 1): the release adds none of its own.
      const oci = Object.keys(config.config.Labels).filter((k) => k.startsWith("org.opencontainers.image."));
      assert.deepEqual(
        oci.sort(),
        ["description", "documentation", "licenses", "revision", "source", "title", "url", "vendor", "version"].map(
          (k) => `org.opencontainers.image.${k}`,
        ),
      );
      assert.equal(config.config.Labels["org.opencontainers.image.version"], VERSION);
      if (process.env.CG_TEST_REVISION) {
        assert.equal(config.config.Labels["org.opencontainers.image.revision"], process.env.CG_TEST_REVISION);
      }
      // The SBOM lists the operating-system packages and, in a Node image, the Node packages
      // installed in it (Amendment 2026-10-26, item 4), each by its package URL's type.
      const sbom = layers.find((l) => l.annotations?.["in-toto.io/predicate-type"] === "https://spdx.dev/Document");
      const packages = JSON.parse(blob(layout, sbom.digest).toString("utf8")).predicate.packages;
      const named = (type, pkg) =>
        packages.some(
          (p) =>
            p.name === pkg &&
            (p.externalRefs ?? []).some((r) => r.referenceType === "purl" && r.referenceLocator.startsWith(`pkg:${type}/`)),
        );
      assert.ok(named("apk", "musl"), "the SBOM names no Alpine package");
      const node = { ingest: "fastify", api: "fastify", dashboard: "next" }[name];
      if (node) assert.ok(named("npm", node), `the SBOM names no ${node}`);
      if (name === "dashboard") {
        assert.ok(!packages.some((p) => p.name === "sharp" || p.name.startsWith("@img/")), "the SBOM names sharp");
      }
    });
  }

  test("deploy_ac_019: the bundle holds the tracked files of deploy/docker/, with their modes, in one directory", () => {
    const top = `cyberguard-deploy-${VERSION}/`;
    const listing = ok("tar", ["-tzvf", bundle]).split("\n");
    const files = new Map();
    for (const line of listing) {
      const fields = line.trim().split(/\s+/);
      const path = fields.at(-1);
      assert.ok(path.startsWith(top), `outside ${top}: ${path}`);
      if (!fields[0].startsWith("d")) files.set(path.slice(top.length), fields[0]);
    }
    // The tracked files, with the modes the repository gives them, but test/, proxy/ and the
    // tests' override (§Operational §12 as the Amendment 2026-10-26, item 4, states it).
    const outside = (f) => f.startsWith("test/") || f.startsWith("proxy/") || f === "docker-compose.test.yml";
    const tracked = new Map(
      ok("git", ["ls-files", "-s", "deploy/docker"], { cwd: REPO })
        .split("\n")
        .map((l) => /^(\d{6}) \S+ \d+\t(.+)$/.exec(l))
        .map(([, mode, path]) => [path.slice("deploy/docker/".length), mode === "100755" ? "-rwxr-xr-x" : "-rw-r--r--"])
        .filter(([path]) => !outside(path)),
    );
    assert.deepEqual([...files.keys()].sort(), [...tracked.keys()].sort());
    for (const [path, mode] of tracked) assert.equal(files.get(path), mode, path);
    for (const script of ["setup.sh", "backup.sh", "restore.sh"]) assert.equal(tracked.get(script), "-rwxr-xr-x", script);
    // The script under test leaves out the same three.
    assert.deepEqual([...LEFT_OUT].sort(), ["docker-compose.test.yml", "proxy", "test"]);
  });

  test("deploy_ac_001: the published compose pins every image by digest, the product images by the dry run's", () => {
    const work = mkdtempSync(join(tmpdir(), "cg-release-"));
    ok("tar", ["-xzf", bundle, "-C", work]);
    const dir = join(work, `cyberguard-deploy-${VERSION}`);
    const env = [
      "CG_DASHBOARD_HOSTNAME=cg.example.test",
      "CG_AGENT_SERVER_NAMES=cg.example.test",
      ...SECRETS.map((s) => `${s}=${"a".repeat(64)}`),
    ];
    writeFileSync(join(dir, ".env"), `${env.join("\n")}\n`);
    // Without the variables a shell may set that Compose would read before .env: an empty
    // COMPOSE_FILE is not an unset one, but a path, the directory itself.
    const shell = { ...process.env };
    for (const name of Object.keys(shell)) {
      if (name.startsWith("COMPOSE_") || name.startsWith("CG_")) delete shell[name];
    }
    const r = run("docker", ["compose", "config", "--format", "json"], { cwd: dir, env: shell });
    assert.equal(r.status, 0, r.stderr);
    const services = JSON.parse(r.stdout).services;
    for (const [service, { image }] of Object.entries(services)) {
      assert.match(image, /@sha256:[0-9a-f]{64}$/, `${service}: ${image}`);
      const product = /\/cyberguard-([a-z]+):/.exec(image);
      if (product) assert.equal(image, `${REGISTRY}/cyberguard-${product[1]}:${VERSION}@${digests[product[1]]}`);
    }
    assert.ok(
      readFileSync(join(dir, "README.md"), "utf8").includes(`/blob/v${VERSION}/docs/operations/install.md`),
      "the README does not point at the release's tag",
    );
  });
});
