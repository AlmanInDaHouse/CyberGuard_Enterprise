#!/usr/bin/env node
// SPEC-023 §Operational §12 (debt #112): the licence check. It reads the licences of the
// production pnpm dependencies (`pnpm licenses list --prod --json`) and of the agent's
// runtime crates (`cargo metadata`, the normal dependencies of cg-agent for the Windows
// target it is released for), and fails, naming each package, on a licence the allowlist
// (.github/scripts/licences-allowed.json) does not admit. A licence is an SPDX expression:
// OR admits it when one side is admitted, AND when both are, `X WITH Y` when X is (an
// exception adds permissions), and the older `X/Y` reads as OR. A package the list names
// is admitted under the licence it names too.
//
//   node .github/scripts/check-licences.mjs [--pnpm <file>] [--cargo <file>] [--report <file>]
//
// --pnpm and --cargo read those commands' JSON from a file instead of running them;
// --report writes every package and its licence as Markdown (a release asset).
import { execFileSync, execSync } from "node:child_process";
import { readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const AGENT_TARGET = "x86_64-pc-windows-msvc";

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

/** cg-agent's runtime crates, cg-agent itself left out: the normal dependencies, transitively. */
export function cargoPackages(json) {
  const byId = new Map(json.packages.map((p) => [p.id, p]));
  const nodes = new Map(json.resolve.nodes.map((n) => [n.id, n]));
  const root = json.packages.find((p) => p.name === "cg-agent");
  if (!root) throw new Error("cargo metadata has no cg-agent");
  const seen = new Set([root.id]);
  const queue = [root.id];
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
  seen.delete(root.id);
  return [...seen].map((id) => {
    const p = byId.get(id);
    return { ecosystem: "crate", name: p.name, version: p.version, licence: p.license ?? "Unknown" };
  });
}

/** The packages the allowlist does not admit, each with the reason. */
export function refused(packages, allowlist) {
  const allowed = new Set(allowlist.licences);
  const named = new Map(allowlist.packages.map((p) => [`${p.ecosystem}:${p.name}`, p]));
  const out = [];
  for (const p of packages) {
    if (named.get(`${p.ecosystem}:${p.name}`)?.licence === p.licence) continue;
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
  const cargo = readJson(arg("--cargo"), "cargo", [
    "metadata",
    "--locked",
    "--format-version",
    "1",
    "--manifest-path",
    join(REPO, "agent", "cg-agent", "Cargo.toml"),
    "--filter-platform",
    AGENT_TARGET,
  ]);
  const npm = pnpmPackages(pnpm);
  const crates = cargoPackages(cargo);
  // A shape the checker does not read would yield nothing, and pass: none is a failure.
  for (const [what, list] of [["npm packages", npm], ["crates", crates]]) {
    if (list.length === 0) {
      process.stderr.write(`licence check: no ${what} read\n`);
      process.exit(1);
    }
  }
  const packages = [...npm, ...crates].sort((a, b) =>
    `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`),
  );
  const report = arg("--report");
  if (report) {
    const rows = packages.map((p) => `| ${p.ecosystem} | ${p.name} | ${p.version} | ${p.licence} |`);
    writeFileSync(report, ["# Licences", "", "| Ecosystem | Package | Version | Licence |", "|---|---|---|---|", ...rows, ""].join("\n"));
  }
  const bad = refused(packages, allowlist);
  for (const p of bad) {
    process.stderr.write(`licence not admitted: ${p.ecosystem} ${p.name}@${p.version}: ${p.licence} (${p.why})\n`);
  }
  if (bad.length > 0) process.exit(1);
  process.stdout.write(`licences admitted: ${packages.length} packages\n`);
}
