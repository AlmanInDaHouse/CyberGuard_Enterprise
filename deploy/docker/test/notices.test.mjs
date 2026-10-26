// SPEC-023 §Data contracts §2, its Amendment 2026-10-26, item 2 (debt #129): the generator of
// an image's THIRD-PARTY-NOTICES, over fixtures. That each image carries the file, naming its
// base image and each package directory of its node_modules, is images.test.mjs's.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { packageDirs, render } from "../../../.github/scripts/third-party-notices.mjs";
import { REPO, run } from "./lib.mjs";

const SCRIPT = join(REPO, ".github", "scripts", "third-party-notices.mjs");
const root = mkdtempSync(join(tmpdir(), "cg-notices-"));
after(() => rmSync(root, { recursive: true, force: true }));

/** Writes a package directory: its manifest and the files given, as { name: text }. */
function pkg(dir, manifest, files = {}) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "package.json"), JSON.stringify(manifest));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  return dir;
}

/** A node_modules as pnpm lays it out: its store, a link to a package in it, a scope. */
function modules(name, packages) {
  const nm = join(root, name, "node_modules");
  for (const [id, manifest, files] of packages) {
    pkg(join(nm, ".pnpm", id, "node_modules", manifest.name), manifest, files);
  }
  return nm;
}

const TEXTS = new Map([
  ["MIT", "MIT License\n\nThe MIT text.\n"],
  ["ISC", "ISC License\n\nThe ISC text.\n"],
]);
const BASE = "node:22@sha256:0000";
const opts = { image: "test", base: BASE, baseSoftware: "Node.js and the Alpine Linux packages", texts: TEXTS };

test("each package directory once, through pnpm's store, a link to it and a scope; .bin and a manifest without a name aside", () => {
  const nm = modules("dirs", [
    ["a@1.0.0", { name: "a", version: "1.0.0" }],
    ["@s+b@2.0.0", { name: "@s/b", version: "2.0.0" }],
  ]);
  // A link beside the store, as a hoisted package is, to a package the store holds.
  symlinkSync(join(nm, ".pnpm", "a@1.0.0", "node_modules", "a"), join(nm, "a"), "junction");
  pkg(join(nm, ".pnpm", "a@1.0.0", "node_modules", "a", "node_modules", "c"), { name: "c", version: "3.0.0" });
  mkdirSync(join(nm, ".bin"));
  pkg(join(nm, ".pnpm", "x@0.0.0", "node_modules", "x"), { private: true });
  const found = packageDirs(nm).map((p) => `${p.manifest.name} ${p.manifest.version}`);
  assert.deepEqual(found.sort(), ["@s/b 2.0.0", "a 1.0.0", "c 3.0.0"]);
});

test("a package's licence and notice files, at any depth outside its node_modules, follow its name, licence, author and repository", () => {
  const dir = pkg(
    join(root, "files", "p"),
    { name: "p", version: "1.2.3", license: "MIT", author: { name: "Ann", email: "ann@example.test" }, repository: { url: "git+https://example.test/p.git" } },
    {
      LICENSE: "Copyright (c) 2020 Ann\r\n\r\nThe MIT text.\r\n",
      "NOTICE.md": "A notice.",
      "CopyrightNotice.txt": "Copyright (c) Ann's employer.",
      "ThirdPartyNoticeText.txt": "Notices of what p bundles.",
      "MIT.LICENSE": "A second licence file.",
      "license.js": "code",
      "notice.css": "a style",
      "README.md": "Not a licence.",
    },
  );
  pkg(join(dir, "sub"), {}, { COPYING: "The copying file." });
  pkg(join(dir, "node_modules", "q"), { name: "q", version: "1.0.0" }, { LICENSE: "Not p's." });
  const { text, errors } = render({ ...opts, packages: [{ name: "p", version: "1.2.3", manifest: JSON.parse(readFileSync(join(dir, "package.json"), "utf8")), dir }] });
  assert.deepEqual(errors, []);
  const lines = text.split("\n");
  const at = lines.indexOf("p 1.2.3");
  assert.ok(at > 0, text);
  assert.deepEqual(lines.slice(at, at + 4), ["p 1.2.3", "Licence: MIT", "Author: Ann <ann@example.test>", "Repository: git+https://example.test/p.git"]);
  assert.ok(text.includes("--- LICENSE ---\nCopyright (c) 2020 Ann\n\nThe MIT text."), text);
  assert.ok(text.includes("--- NOTICE.md ---\nA notice."));
  assert.ok(text.includes("--- sub/COPYING ---\nThe copying file."));
  assert.ok(text.includes("--- CopyrightNotice.txt ---\nCopyright (c) Ann's employer."));
  assert.ok(text.includes("--- ThirdPartyNoticeText.txt ---\nNotices of what p bundles."));
  assert.ok(text.includes("--- MIT.LICENSE ---\nA second licence file."));
  for (const other of ["license.js", "notice.css", "README.md", "Not p's.", "Not a licence."]) {
    assert.ok(!text.includes(other), `${other} in:\n${text}`);
  }
  assert.ok(lines.includes(`Base image: ${BASE}`));
  assert.ok(text.includes("(Node.js and the Alpine Linux packages) is distributed as its publisher distributes"));
});

test("a manifest's homepage is not given as its repository, and a package without an author gets no author line", () => {
  const manifest = { name: "h", version: "1.0.0", license: "MIT", homepage: "https://example.test/h" };
  const dir = pkg(join(root, "homepage", "h"), manifest);
  const { text, errors } = render({ ...opts, packages: [{ name: "h", version: "1.0.0", manifest, dir }] });
  assert.deepEqual(errors, []);
  const lines = text.split("\n");
  const at = lines.indexOf("h 1.0.0");
  assert.deepEqual(lines.slice(at, at + 4), [
    "h 1.0.0",
    "Licence: MIT",
    "",
    "The package holds no licence file. The text of MIT follows at the end of",
  ]);
  assert.ok(!text.includes("https://example.test/h"), text);
  assert.ok(!/author/i.test(text.slice(text.indexOf("h 1.0.0"))), text);
});

test("a package that holds no licence file is named so, and the text of its declared licence follows once", () => {
  const packages = ["m1", "m2"].map((name) => {
    const manifest = { name, version: "1.0.0", license: "MIT" };
    return { name, version: "1.0.0", manifest, dir: pkg(join(root, "none", name), manifest) };
  });
  const manifest = { name: "i", version: "1.0.0", license: "(ISC OR MIT)" };
  packages.push({ name: "i", version: "1.0.0", manifest, dir: pkg(join(root, "none", "i"), manifest) });
  const { text, errors } = render({ ...opts, packages });
  assert.deepEqual(errors, []);
  assert.equal(text.split("The package holds no licence file.").length - 1, 3);
  assert.equal(text.split("The MIT text.").length - 1, 1, text);
  assert.equal(text.split("The ISC text.").length - 1, 1, text);
});

test("CyberGuard's own packages are left out, and a package at one version is named once", () => {
  const manifest = { name: "d", version: "1.0.0", license: "MIT" };
  const dir = pkg(join(root, "own", "d"), manifest, { LICENSE: "D's licence." });
  const own = { name: "@cyberguard/x", version: "0.0.0", manifest: { name: "@cyberguard/x" }, dir };
  const { text, errors } = render({ ...opts, packages: [own, { name: "d", version: "1.0.0", manifest, dir }, { name: "d", version: "1.0.0", manifest, dir }] });
  assert.deepEqual(errors, []);
  assert.ok(!text.includes("@cyberguard/x"));
  assert.equal(text.split("\n").filter((l) => l === "d 1.0.0").length, 1);
  assert.match(text, /^The Node packages named below: 1\. They are those of the image's node_modules\.$/m);
  // With the application's production dependencies (--pnpm), the file says so.
  const bundled = render({ ...opts, bundled: true, packages: [{ name: "d", version: "1.0.0", manifest, dir }] }).text;
  assert.match(
    bundled,
    /^The Node packages named below: 1\. They are those of the image's node_modules and the application's production dependencies, whose code its compiled bundles may carry\.$/m,
  );
});

test("the build fails, naming each package, when one holds no licence file and declares no licence, or one whose text it does not hold", () => {
  const nm = modules("refused", [
    ["ok@1.0.0", { name: "ok", version: "1.0.0", license: "MIT" }, { LICENSE: "Fine." }],
    ["bare@1.0.0", { name: "bare", version: "1.0.0" }],
    ["odd@2.0.0", { name: "odd", version: "2.0.0", license: "MIT AND WTFPL" }],
  ]);
  const out = join(root, "refused", "THIRD-PARTY-NOTICES");
  const args = [SCRIPT, "--image", "test", "--base", BASE, "--base-software", "Node.js", "--modules", nm, "--out", out];
  const r = run(process.execPath, args);
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, /^third-party notices: bare@1\.0\.0: no licence file, and no declared licence$/m);
  assert.match(r.stderr, /^third-party notices: odd@2\.0\.0: no licence file, and no text held for WTFPL$/m);
  assert.doesNotMatch(r.stderr, /ok@1\.0\.0/);
  assert.equal(existsSync(out), false);
});

test("with --sources, a package's files are read there, at its path below --modules; one that is not there fails the build", () => {
  // The dashboard's standalone tree: each package's manifest and code, without its licence.
  const nm = modules("standalone", [
    ["s@1.0.0", { name: "s", version: "1.0.0", license: "MIT" }],
    ["t@1.0.0", { name: "t", version: "1.0.0", license: "MIT" }],
  ]);
  // The install it was traced from, at the same paths: s's licence; t is not there.
  const sources = join(root, "install", "node_modules");
  pkg(join(sources, ".pnpm", "s@1.0.0", "node_modules", "s"), { name: "s", version: "1.0.0", license: "MIT" }, { LICENSE: "S's licence." });
  const out = join(root, "standalone", "THIRD-PARTY-NOTICES");
  const args = [SCRIPT, "--image", "test", "--base", BASE, "--base-software", "Node.js", "--modules", nm, "--sources", sources, "--out", out];
  const r = run(process.execPath, args);
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, /^third-party notices: t@1\.0\.0: its files are not at .*t$/m);
  assert.doesNotMatch(r.stderr, /s@1\.0\.0/);
  assert.equal(existsSync(out), false);
  pkg(join(sources, ".pnpm", "t@1.0.0", "node_modules", "t"), { name: "t", version: "1.0.0", license: "MIT" }, { LICENSE: "T's licence." });
  const again = run(process.execPath, args);
  assert.equal(again.status, 0, again.stderr);
  const text = readFileSync(out, "utf8");
  assert.ok(text.includes("--- LICENSE ---\nS's licence."), text);
  assert.ok(text.includes("--- LICENSE ---\nT's licence."), text);
});

test("with --pnpm, each package `pnpm licenses list --json` lists is named, from its path", () => {
  const nm = modules("pnpm", [["ok@1.0.0", { name: "ok", version: "1.0.0", license: "MIT" }, { LICENSE: "Fine." }]]);
  const bundledDir = pkg(join(root, "pnpm", "bundled"), { name: "bundled", version: "4.5.6", license: "ISC" }, { LICENSE: "Bundled's licence." });
  const list = join(root, "pnpm", "licences.json");
  writeFileSync(list, JSON.stringify({ ISC: [{ name: "bundled", versions: ["4.5.6"], paths: [bundledDir] }] }));
  const out = join(root, "pnpm", "THIRD-PARTY-NOTICES");
  const r = run(process.execPath, [SCRIPT, "--image", "test", "--base", BASE, "--base-software", "Node.js", "--modules", nm, "--pnpm", list, "--out", out]);
  assert.equal(r.status, 0, r.stderr);
  const lines = readFileSync(out, "utf8").split("\n");
  assert.ok(lines.includes("ok 1.0.0") && lines.includes("bundled 4.5.6"), lines.join("\n"));
  assert.ok(lines.includes("Bundled's licence."));
  assert.ok(lines.some((l) => l.endsWith("and the application's production dependencies, whose code its compiled bundles may carry.")));
});

test("run over a node_modules whose packages it can attribute, it writes the file", () => {
  const nm = modules("written", [["ok@1.0.0", { name: "ok", version: "1.0.0", license: "MIT" }, { LICENSE: "Fine." }]]);
  const out = join(root, "written", "THIRD-PARTY-NOTICES");
  const r = run(process.execPath, [SCRIPT, "--image", "test", "--base", BASE, "--base-software", "Node.js", "--modules", nm, "--out", out]);
  assert.equal(r.status, 0, r.stderr);
  const text = readFileSync(out, "utf8");
  assert.ok(text.startsWith("Third-party notices: CyberGuard test\n"));
  assert.ok(text.split("\n").includes("ok 1.0.0"));
  assert.ok(text.includes("--- LICENSE ---\nFine."));
});
