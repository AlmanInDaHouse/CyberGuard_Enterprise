#!/usr/bin/env node
// SPEC-023 §Data contracts §1 and §Operational §12, its Amendment 2026-10-26, item 4: the
// release's bundle step, which release.yml publishes and deploy-ci.yml's dry run checks. It
// takes the files of deploy/docker/ the commit tracks, with their modes, leaves out test/,
// proxy/ and the tests' override, names each product image in the compose as
// `<name>:<version>@sha256:<digest>`, points the README at the release's tag, and packs
// cyberguard-deploy-<version>.tar.gz, which unpacks into cyberguard-deploy-<version>/.
//
//   node .github/scripts/release-bundle.mjs --version <v> --digests <digests.json> --out <dir>
//     [--ref <commit>]
//
// Its packing needs GNU tar and gzip (the release's ubuntu runner has both).
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { IMAGES, REGISTRY } from "./release-images.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

/** What the published bundle leaves out of deploy/docker/ (§Data contracts §1). */
export const LEFT_OUT = ["test", "proxy", "docker-compose.test.yml"];

/**
 * The compose with each product image named `<registry>/cyberguard-<name>:<version>@<digest>`.
 * Refuses a compose that names an image of the four nowhere, or that keeps an image not
 * pinned by digest.
 */
export function rewriteCompose(text, version, digests) {
  let out = text;
  for (const { name } of IMAGES) {
    const digest = digests[name];
    if (!/^sha256:[0-9a-f]{64}$/.test(digest ?? "")) throw new Error(`no digest for ${name}`);
    const re = new RegExp(`^(\\s*image:\\s*)${REGISTRY}/cyberguard-${name}:[^\\s@]+\\s*$`, "gm");
    if (!re.test(out)) throw new Error(`the compose names no ${name} image`);
    out = out.replace(re, (_, lead) => `${lead}${REGISTRY}/cyberguard-${name}:${version}@${digest}`);
  }
  for (const line of out.split("\n")) {
    const m = /^\s*image:\s*(\S+)\s*$/.exec(line);
    if (m && !/@sha256:[0-9a-f]{64}$/.test(m[1])) throw new Error(`an image not pinned by digest: ${m[1]}`);
  }
  return out;
}

/** The README with the release's version in place of `<version>`. */
export function rewriteReadme(text, version) {
  if (!text.includes("<version>")) throw new Error("the README names no <version>");
  return text.replaceAll("<version>", version);
}

function arg(name) {
  const i = process.argv.indexOf(name);
  if (i === -1) return undefined;
  const v = process.argv[i + 1];
  if (v === undefined || v.startsWith("--")) throw new Error(`${name} needs a value`);
  return v;
}

const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { cwd: REPO, maxBuffer: 256 * 1024 * 1024, ...opts });

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  try {
    const version = arg("--version");
    const digestsFile = arg("--digests");
    const out = arg("--out");
    const ref = arg("--ref") ?? "HEAD";
    if (!version || !digestsFile || !out) throw new Error("--version, --digests and --out are required");
    const digests = JSON.parse(readFileSync(digestsFile, "utf8"));
    const name = `cyberguard-deploy-${version}`;
    mkdirSync(out, { recursive: true });
    const work = mkdtempSync(join(tmpdir(), "cg-bundle-"));
    try {
      // The tracked files with their modes: git's archive, with the umask that keeps 0644 and
      // 0755 (its default, 0002, would write 0664 and 0775).
      const archive = run("git", ["-c", "tar.umask=0022", "archive", "--format=tar", ref, "deploy/docker"]);
      run("tar", ["-x", "-C", work], { input: archive });
      const dir = join(work, name);
      renameSync(join(work, "deploy", "docker"), dir);
      for (const entry of LEFT_OUT) rmSync(join(dir, entry), { recursive: true, force: true });
      const compose = join(dir, "docker-compose.yml");
      writeFileSync(compose, rewriteCompose(readFileSync(compose, "utf8"), version, digests));
      const readme = join(dir, "README.md");
      writeFileSync(readme, rewriteReadme(readFileSync(readme, "utf8"), version));
      // Reproducible: sorted, owned by root, dated at the commit, gzip without a name or time.
      const mtime = run("git", ["log", "-1", "--format=%ct", ref], { encoding: "utf8" }).trim();
      const tar = run("tar", [
        "--sort=name",
        "--owner=0",
        "--group=0",
        "--numeric-owner",
        `--mtime=@${mtime}`,
        "-c",
        "-C",
        work,
        name,
      ]);
      writeFileSync(join(out, `${name}.tar.gz`), run("gzip", ["-n", "-9"], { input: tar }));
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
    process.stdout.write(`${join(out, `${name}.tar.gz`)}\n`);
  } catch (e) {
    process.stderr.write(`release bundle: ${e instanceof Error ? e.message : String(e)}\n`);
    process.exit(1);
  }
}
