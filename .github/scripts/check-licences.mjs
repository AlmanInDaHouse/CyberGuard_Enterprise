#!/usr/bin/env node
// SPEC-023 §Operational §12 (debt #112): the licence check. It reads the licences of the
// production pnpm dependencies (`pnpm licenses list --prod --json`) and of the runtime crates
// of the agent and of the setup's cg-setup (`cargo metadata` of the workspace, the normal
// dependencies of cg-agent and cg-setup for the Windows target they are released for; SPEC-024
// Amendment 2026-11-14, point 4), and fails, naming each package, on a licence the allowlist
// (.github/scripts/licences-allowed.json) does not admit. A licence is an SPDX expression:
// OR admits it when one side is admitted, AND when both are, `X WITH Y` when X is (an
// exception adds permissions), and the older `X/Y` reads as OR. A package the list names
// is admitted under the licence it names too. The nuget pins that the agent's setup or MSI
// carries (deploy/windows/pins/pins.json) declare their licence as a file, the WiX EULA, so
// they are admitted by name alone, under MS-RL (SPEC-024 Amendment 2026-11-01, point 1).
//
//   node .github/scripts/check-licences.mjs [--pnpm <file>] [--cargo <file>] [--nuget <file>] [--report <file>]
//
// --pnpm and --cargo read those commands' JSON from a file instead of running them, --nuget
// another pins file;
// --report writes every package and its licence as Markdown (a release asset), then the WiX
// files the setup carries and WiX's LICENSE.TXT, MS-RL's text (the Amendment 2026-11-14, point 4).
import { execFileSync, execSync } from "node:child_process";
import { readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const AGENT_TARGET = "x86_64-pc-windows-msvc";
/** The workspace's packages whose runtime crates reach an endpoint: the agent, and cg-setup inside the setup. */
const CARGO_ROOTS = ["cg-agent", "cg-setup"];
/** WiX's LICENSE.TXT, beside the theme whose MS-RL headers name it (the Amendment 2026-11-14, point 3). */
const WIX_LICENCE = join(REPO, "deploy", "windows", "bundle", "theme", "LICENSE.TXT");

/** Whether `expression` is admitted by the set of licence identifiers `allowed`. */
export function admitted(expression, allowed) {
  const tokens = String(expression ?? "")
    .replace(/\s*\/\s*/g, " OR ")
    .match(/\(|\)|[^\s()]+/g);
  if (!tokens) return false;
  let i = 0;
  const peek = () => tokens[i];
  const next = () => tokens[i++];
  const or = () => {
    let v = and();
    while (peek() === "OR") {
      next();
      const w = and();
      v = v || w;
    }
    return v;
  };
  const and = () => {
    let v = atom();
    while (peek() === "AND") {
      next();
      const w = atom();
      v = v && w;
    }
    return v;
  };
  const atom = () => {
    const t = next();
    if (t === "(") {
      const v = or();
      if (next() !== ")") throw new Error(`unbalanced licence expression: ${expression}`);
      return v;
    }
    if (t === undefined || t === ")" || t === "AND" || t === "OR" || t === "WITH") {
      throw new Error(`malformed licence expression: ${expression}`);
    }
    if (peek() === "WITH") {
      next();
      const exception = next();
      if (exception === undefined || ["(", ")", "AND", "OR", "WITH"].includes(exception)) {
        throw new Error(`malformed licence expression: ${expression}`);
      }
      return allowed.has(`${t} WITH ${exception}`) || allowed.has(t);
    }
    return allowed.has(t);
  };
  const v = or();
  if (i !== tokens.length) throw new Error(`malformed licence expression: ${expression}`);
  return v;
}

/** The production pnpm packages: { ecosystem, name, version, licence }. */
export function pnpmPackages(json) {
  const out = [];
  for (const [licence, pkgs] of Object.entries(json)) {
    for (const p of pkgs) {
      for (const version of p.versions ?? [p.version]) {
        out.push({ ecosystem: "npm", name: p.name, version, licence: p.license ?? licence });
      }
    }
  }
  return out;
}

/**
 * The runtime crates of cg-agent and of cg-setup, read from one cargo metadata of the workspace:
 * their normal dependencies, transitively, each crate once, the workspace's own packages left
 * out (cg-setup's cg-agent is a path dependency, not a third party's; the Amendment 2026-11-14,
 * point 4).
 */
export function cargoPackages(json) {
  const byId = new Map(json.packages.map((p) => [p.id, p]));
  const nodes = new Map(json.resolve.nodes.map((n) => [n.id, n]));
  const roots = CARGO_ROOTS.map((name) => {
    const root = json.packages.find((p) => p.name === name);
    if (!root) throw new Error(`cargo metadata has no ${name}`);
    return root.id;
  });
  const own = new Set([...roots, ...(json.workspace_members ?? [])]);
  const seen = new Set(roots);
  const queue = [...roots];
  while (queue.length > 0) {
    const node = nodes.get(queue.shift());
    for (const dep of node?.deps ?? []) {
      const normal = (dep.dep_kinds ?? []).some((k) => k.kind === null);
      if (normal && !seen.has(dep.pkg)) {
        seen.add(dep.pkg);
        queue.push(dep.pkg);
      }
    }
  }
  for (const id of own) seen.delete(id);
  return [...seen].map((id) => {
    const p = byId.get(id);
    return { ecosystem: "crate", name: p.name, version: p.version, licence: p.license ?? "Unknown" };
  });
}

/** The nuget pins whose binaries the setup or the MSI carries (SPEC-024 Amendment 2026-11-01, point 1). */
export function nugetPackages(pins) {
  return pins.packages.filter((p) => p.carried).map((p) => ({ ecosystem: "nuget", name: p.id, version: p.version, licence: p.licence ?? "Unknown" }));
}

/**
 * The report's section on the WiX files the setup carries (SPEC-024 Amendment 2026-11-14,
 * point 4): each named as the setup carries it, their WiX release and commit (`pins`), that they
 * are distributed under MS-RL, the source archive of point 3, and `licence`, WiX's LICENSE.TXT
 * (its copyright line and MS-RL's full text), verbatim. The release MSI carries no WiX file.
 */
export function wixSection(pins, licence) {
  const release = pins.wixRelease;
  const short = /^(.+)\+([0-9a-f]{7,40})$/.exec(pins.wixInformationalVersion ?? "");
  const commit = pins.wixCommit ?? "";
  if (!release || !short || short[1] !== release || !/^[0-9a-f]{40}$/.test(commit) || !commit.startsWith(short[2])) {
    throw new Error(`pins name no WiX release and commit: ${pins.wixRelease} ${pins.wixInformationalVersion} ${pins.wixCommit}`);
  }
  // A fence longer than any run of backticks in the text, so that the text cannot close it.
  const fence = "`".repeat(Math.max(3, ...[...licence.matchAll(/`+/g)].map((m) => m[0].length + 1)));
  return [
    "## WiX files the setup carries",
    "",
    `The setup carries these files of WiX ${release}, commit \`${commit}\`, distributed under the Microsoft Reciprocal License (MS-RL):`,
    "",
    "- Burn's engine, which is the setup's own executable: its code unmodified, its version resource the setup's, which keeps WiX's copyright line;",
    "- the standard bootstrapper application, `wixstdba.exe`, and its `logo.png`, unmodified;",
    "- the theme, `thm.xml` and `thm.wxl`, modified from WiX's `HyperlinkTheme` files.",
    "",
    `Their source, and the setup's own, is the release's asset \`wix-${release}-source.tar.gz\`. The release's MSI carries no WiX file.`,
    "",
    "WiX's `LICENSE.TXT`, its copyright line and MS-RL's full text:",
    "",
    `${fence}text`,
    `${licence}${licence.endsWith("\n") ? "" : "\n"}${fence}`,
    "",
  ].join("\n");
}

/** The packages the allowlist does not admit, each with the reason. */
export function refused(packages, allowlist) {
  const allowed = new Set(allowlist.licences);
  const named = new Map(allowlist.packages.map((p) => [`${p.ecosystem}:${p.name}`, p]));
  const out = [];
  for (const p of packages) {
    if (named.get(`${p.ecosystem}:${p.name}`)?.licence === p.licence) continue;
    // A nuget package declares its licence as a file, so the pin's own field proves nothing.
    if (p.ecosystem === "nuget") {
      out.push({ ...p, why: "a nuget pin is admitted by name alone, and the list does not name it under this licence" });
      continue;
    }
    let ok;
    try {
      ok = admitted(p.licence, allowed);
    } catch (e) {
      out.push({ ...p, why: e instanceof Error ? e.message : String(e) });
      continue;
    }
    if (!ok) out.push({ ...p, why: "not in the allowlist" });
  }
  return out;
}

function arg(name) {
  const i = process.argv.indexOf(name);
  return i === -1 ? undefined : process.argv[i + 1];
}

function readJson(file, command, args) {
  if (file) return JSON.parse(readFileSync(file, "utf8"));
  const options = { cwd: REPO, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 };
  // On Windows pnpm is a .cmd, which Node runs only through a shell; the command is fixed.
  if (process.platform === "win32" && command === "pnpm") {
    return JSON.parse(execSync([command, ...args].join(" "), options));
  }
  return JSON.parse(execFileSync(command, args, options));
}

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  const allowlist = JSON.parse(readFileSync(join(REPO, ".github", "scripts", "licences-allowed.json"), "utf8"));
  const pnpm = readJson(arg("--pnpm"), "pnpm", ["licenses", "list", "--prod", "--json"]);
  // The workspace's metadata, which holds both roots.
  const cargo = readJson(arg("--cargo"), "cargo", [
    "metadata",
    "--locked",
    "--format-version",
    "1",
    "--manifest-path",
    join(REPO, "Cargo.toml"),
    "--filter-platform",
    AGENT_TARGET,
  ]);
  const npm = pnpmPackages(pnpm);
  const crates = cargoPackages(cargo);
  const pins = JSON.parse(readFileSync(arg("--nuget") ?? join(REPO, "deploy", "windows", "pins", "pins.json"), "utf8"));
  const nuget = nugetPackages(pins);
  // A shape the checker does not read would yield nothing, and pass: none is a failure.
  for (const [what, list] of [["npm packages", npm], ["crates", crates], ["nuget packages", nuget]]) {
    if (list.length === 0) {
      process.stderr.write(`licence check: no ${what} read\n`);
      process.exit(1);
    }
  }
  const packages = [...npm, ...crates, ...nuget].sort((a, b) =>
    `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`),
  );
  const report = arg("--report");
  if (report) {
    const rows = packages.map((p) => `| ${p.ecosystem} | ${p.name} | ${p.version} | ${p.licence} |`);
    const table = ["# Licences", "", "| Ecosystem | Package | Version | Licence |", "|---|---|---|---|", ...rows, ""].join("\n");
    writeFileSync(report, `${table}\n${wixSection(pins, readFileSync(WIX_LICENCE, "utf8"))}`);
  }
  const bad = refused(packages, allowlist);
  for (const p of bad) {
    process.stderr.write(`licence not admitted: ${p.ecosystem} ${p.name}@${p.version}: ${p.licence} (${p.why})\n`);
  }
  if (bad.length > 0) process.exit(1);
  process.stdout.write(`licences admitted: ${packages.length} packages\n`);
}
