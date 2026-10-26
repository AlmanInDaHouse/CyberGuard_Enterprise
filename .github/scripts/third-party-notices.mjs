#!/usr/bin/env node
// SPEC-023 §Data contracts §2, its Amendment 2026-10-26, item 2 (debt #129): an image's
// THIRD-PARTY-NOTICES. It names each Node package the image adds to its base image, each
// package directory of the node_modules the image ships and, with --pnpm, the packages of
// `pnpm licenses list --prod --json` (the dashboard's production dependencies, whose code
// its compiled bundles carry); for each, its name, version, declared licence, author and
// repository, and the text of each licence or notice file the published package holds. A
// package that holds none is named so, and the standard text of its declared licence follows
// once, from .github/scripts/licence-texts/. It fails, naming each package, when a package
// has neither a licence file nor a declared licence whose text that directory holds, or when
// its files are not where --sources says.
//
//   node third-party-notices.mjs --image <name> --base <reference@digest> --base-software <text>
//     --out <file> [--modules <node_modules>] [--sources <node_modules>] [--pnpm <file>]
//
// --modules is the node_modules the image ships. --sources, when given, is where each of its
// packages' published files are read instead, at the same path below it (the dashboard's
// standalone tree keeps a package's code and manifest, not its licence files: its packages
// are read from the build's own node_modules).
import { existsSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const TEXTS = join(dirname(fileURLToPath(import.meta.url)), "licence-texts");

/** A licence or notice file's name: one that names a licence, copying, a notice or a
 *  copyright anywhere (LICENSE, LICENSE-MIT, MIT.LICENSE, COPYING, NOTICE, CopyrightNotice.txt,
 *  ThirdPartyNoticeText.txt). */
const LICENCE_FILE = /licen[cs]e|copying|notice|copyright/i;
/** Code, styles and media that the name pattern would otherwise match (`license.js`). */
const CODE =
  /\.(c|m)?[jt]sx?$|\.(json|map|node|css|scss|less|html?|sh|py|rb|go|rs|c|h|cc|cpp|hpp|wasm|png|jpe?g|gif|svg|ico|webp|woff2?|ttf|otf|eot|gz|tgz|zip|flow|snap)$/i;

/** The package directories below a node_modules: `<name>` and `@scope/<name>`, recursively
 *  through pnpm's `.pnpm/<id>/node_modules`, each once by its real path. */
export function packageDirs(modules) {
  const seen = new Set();
  const out = [];
  const visitModules = (dir) => {
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name === ".bin") continue;
      const path = join(dir, e.name);
      if (e.name === ".pnpm") {
        for (const id of safeList(path)) visitModules(join(path, id, "node_modules"));
        continue;
      }
      if (e.name.startsWith("@")) {
        for (const child of safeList(path)) visitPackage(join(path, child));
        continue;
      }
      visitPackage(path);
    }
  };
  const visitPackage = (path) => {
    let real;
    try {
      real = realpathSync(path);
      if (!statSync(real).isDirectory()) return;
    } catch {
      return; // a dangling link
    }
    if (seen.has(real)) return;
    seen.add(real);
    let manifest;
    try {
      manifest = JSON.parse(readFileSync(join(real, "package.json"), "utf8"));
    } catch {
      return; // not a package
    }
    if (typeof manifest.name !== "string") return; // a manifest that names no package
    out.push({ path, manifest });
    visitModules(join(real, "node_modules"));
  };
  visitModules(modules);
  return out;
}

function safeList(dir) {
  try {
    return readdirSync(dir);
  } catch {
    return [];
  }
}

/** The licence and notice files a package holds, at any depth, its own node_modules aside. */
export function licenceFiles(pkgDir) {
  const out = [];
  const walk = (dir) => {
    for (const e of safeList(dir).sort()) {
      const path = join(dir, e);
      let st;
      try {
        st = statSync(path);
      } catch {
        continue;
      }
      if (st.isDirectory()) {
        if (e !== "node_modules") walk(path);
      } else if (LICENCE_FILE.test(e) && !CODE.test(e)) {
        out.push(path);
      }
    }
  };
  walk(pkgDir);
  return out;
}

function licenceOf(manifest) {
  if (typeof manifest.license === "string") return manifest.license;
  if (manifest.license && typeof manifest.license.type === "string") return manifest.license.type;
  if (Array.isArray(manifest.licenses)) {
    const types = manifest.licenses.map((l) => (typeof l === "string" ? l : l?.type)).filter(Boolean);
    if (types.length > 0) return types.length === 1 ? types[0] : `(${types.join(" OR ")})`;
  }
  return null;
}

function personOf(p) {
  if (!p) return null;
  if (typeof p === "string") return p;
  return [p.name, p.email && `<${p.email}>`, p.url && `(${p.url})`].filter(Boolean).join(" ") || null;
}

function repositoryOf(manifest) {
  const r = manifest.repository;
  if (typeof r === "string") return r;
  if (r && typeof r.url === "string") return r.url;
  return null;
}

/** The SPDX identifiers of an expression, its operators and parentheses aside. */
function licenceIds(expression) {
  return (expression.match(/[^\s()]+/g) ?? []).filter((t) => !["AND", "OR", "WITH"].includes(t));
}

/**
 * Builds the file. `packages`: { name, version, manifest, dir }; `bundled`: whether they
 * include the application's production dependencies (--pnpm). Returns { text, errors }.
 */
export function render({ image, base, baseSoftware, packages, texts, bundled = false }) {
  const byKey = new Map();
  for (const p of packages) {
    if (p.name.startsWith("@cyberguard/")) continue; // CyberGuard's own: LICENSE
    const key = `${p.name}@${p.version}`;
    if (!byKey.has(key)) byKey.set(key, p);
  }
  const sorted = [...byKey.values()].sort((a, b) =>
    a.name === b.name ? a.version.localeCompare(b.version) : a.name.localeCompare(b.name),
  );
  const errors = [];
  const needed = new Set();
  const rule = "=".repeat(78);
  const parts = [
    `Third-party notices: CyberGuard ${image}`,
    "",
    "This image carries CyberGuard, under the licence in LICENSE beside this file, and the",
    "third-party software named below.",
    "",
    `Base image: ${base}`,
    `The base image's software (${baseSoftware}) is distributed as its publisher distributes`,
    "it, with the licence information it carries.",
    "",
    `The Node packages named below: ${sorted.length}. They are those of the image's node_modules${
      bundled ? " and the application's production dependencies, whose code its compiled bundles may carry" : ""
    }.`,
    "For each, its name and version, its declared licence, its author and repository where its",
    "manifest names them, and the text of each licence or notice file the package holds.",
  ];
  for (const p of sorted) {
    const licence = licenceOf(p.manifest);
    const author = personOf(p.manifest.author);
    const repository = repositoryOf(p.manifest);
    parts.push("", rule, `${p.name} ${p.version}`, `Licence: ${licence ?? "(none declared)"}`);
    if (author) parts.push(`Author: ${author}`);
    if (repository) parts.push(`Repository: ${repository}`);
    if (!existsSync(join(p.dir, "package.json"))) {
      errors.push(`${p.name}@${p.version}: its files are not at ${p.dir}`);
      continue;
    }
    const files = licenceFiles(p.dir);
    if (files.length > 0) {
      for (const file of files) {
        const rel = relative(p.dir, file).replaceAll("\\", "/");
        parts.push("", `--- ${rel} ---`, readFileSync(file, "utf8").replace(/\r\n/g, "\n").trimEnd());
      }
      continue;
    }
    const ids = licence ? licenceIds(licence) : [];
    const missing = ids.filter((id) => !texts.has(id));
    if (ids.length === 0 || missing.length > 0) {
      errors.push(
        `${p.name}@${p.version}: no licence file, and ${
          ids.length === 0 ? "no declared licence" : `no text held for ${missing.join(", ")}`
        }`,
      );
      continue;
    }
    for (const id of ids) needed.add(id);
    parts.push(
      "",
      `The package holds no licence file. The text of ${ids.join(", ")} follows at the end of`,
      "this file.",
    );
  }
  if (needed.size > 0) {
    parts.push("", rule, "The texts of the licences named above for the packages that hold no licence file");
    for (const id of [...needed].sort()) {
      parts.push("", rule, id, "", texts.get(id).trimEnd());
    }
  }
  parts.push("");
  return { text: parts.join("\n"), errors };
}

function arg(name) {
  const i = process.argv.indexOf(name);
  if (i === -1) return undefined;
  const v = process.argv[i + 1];
  if (v === undefined || v.startsWith("--")) throw new Error(`${name} needs a value`);
  return v;
}

function readTexts() {
  const texts = new Map();
  for (const f of safeList(TEXTS)) {
    if (f.endsWith(".txt")) texts.set(basename(f, ".txt"), readFileSync(join(TEXTS, f), "utf8"));
  }
  return texts;
}

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  try {
    const image = arg("--image");
    const base = arg("--base");
    const baseSoftware = arg("--base-software");
    const out = arg("--out");
    if (!image || !base || !baseSoftware || !out) {
      throw new Error("--image, --base, --base-software and --out are required");
    }
    const packages = [];
    const modules = arg("--modules");
    if (modules) {
      const sources = arg("--sources");
      for (const { path, manifest } of packageDirs(modules)) {
        const dir = sources ? join(sources, relative(modules, path)) : path;
        packages.push({ name: manifest.name, version: manifest.version ?? "", manifest, dir });
      }
    }
    const pnpm = arg("--pnpm");
    if (pnpm) {
      for (const list of Object.values(JSON.parse(readFileSync(pnpm, "utf8")))) {
        for (const p of list) {
          for (const dir of p.paths ?? []) {
            const manifest = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
            if (typeof manifest.name !== "string") continue;
            packages.push({ name: manifest.name, version: manifest.version ?? "", manifest, dir });
          }
        }
      }
    }
    const { text, errors } = render({
      image,
      base,
      baseSoftware,
      packages,
      texts: readTexts(),
      bundled: Boolean(pnpm),
    });
    for (const e of errors) process.stderr.write(`third-party notices: ${e}\n`);
    if (errors.length > 0) process.exit(1);
    writeFileSync(out, text);
    process.stdout.write(`third-party notices: ${out}\n`);
  } catch (e) {
    process.stderr.write(`third-party notices: ${e instanceof Error ? e.message : String(e)}\n`);
    process.exit(1);
  }
}
