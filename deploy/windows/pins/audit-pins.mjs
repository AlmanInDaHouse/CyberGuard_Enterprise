#!/usr/bin/env node
// SPEC-024 Amendment 2026-11-01, point 1: the advisory check of the nuget pins, a job of
// dependency-audit.yml. `dotnet list package --vulnerable` reads PackageReference items, and
// neither the wix tool nor the MSBuild SDK is one, so this reads the feed NuGet's own audit
// reads, nuget.org's vulnerability index, for every pin. Any advisory that covers a pin fails
// the run, whatever its severity; a feed it cannot read fails it too.
//
//   node deploy/windows/pins/audit-pins.mjs [--pages <file>]
//
// --pages reads the feed's pages from a JSON array instead of fetching them.
import { readFileSync, realpathSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const INDEX = "https://api.nuget.org/v3/vulnerabilities/index.json";
const TIMEOUT_MS = 60_000;

function parseVersion(text) {
  const m = /^(\d+(?:\.\d+){0,3})(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(text);
  if (!m) throw new Error(`unreadable version: ${JSON.stringify(text)}`);
  const parts = m[1].split(".").map(Number);
  while (parts.length < 4) parts.push(0);
  return { parts, pre: m[2] ?? null };
}

function compare(a, b) {
  for (let i = 0; i < 4; i++) {
    if (a.parts[i] !== b.parts[i]) return a.parts[i] < b.parts[i] ? -1 : 1;
  }
  if (a.pre === b.pre) return 0;
  if (a.pre === null) return 1;
  if (b.pre === null) return -1;
  return a.pre < b.pre ? -1 : 1;
}

/** Whether `version` falls in a NuGet version range such as `[4.0.0, 4.0.5)`; throws on any other text. */
export function inRange(version, range) {
  const m = /^([[(])\s*([^,\])]*?)\s*(?:(,)\s*([^\])]*?)\s*)?([\])])$/.exec(String(range).trim());
  if (!m) throw new Error(`unreadable version range: ${JSON.stringify(range)}`);
  const [, open, low, comma, high, close] = m;
  const v = parseVersion(version);
  if (comma === undefined) {
    if (open !== "[" || close !== "]" || low === "") throw new Error(`unreadable version range: ${JSON.stringify(range)}`);
    return compare(v, parseVersion(low)) === 0;
  }
  if (low !== "") {
    const c = compare(v, parseVersion(low));
    if (c < 0 || (c === 0 && open === "(")) return false;
  }
  if (high !== "") {
    const c = compare(v, parseVersion(high));
    if (c > 0 || (c === 0 && close === ")")) return false;
  }
  return true;
}

/** The advisories, across the feed's pages, that cover a pinned version. */
export function advisories(pins, pages) {
  const out = [];
  const seen = new Set();
  for (const p of pins.packages) {
    for (const page of pages) {
      for (const a of page[p.id.toLowerCase()] ?? []) {
        if (!inRange(p.version, a.versions)) continue;
        const key = `${p.id} ${a.url} ${a.versions}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push({ id: p.id, version: p.version, url: a.url, severity: a.severity, versions: a.versions });
      }
    }
  }
  return out;
}

async function fetchJson(url) {
  const res = await fetch(url, { signal: AbortSignal.timeout(TIMEOUT_MS) });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return res.json();
}

async function main() {
  const pins = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "pins.json"), "utf8"));
  const i = process.argv.indexOf("--pages");
  const pages =
    i === -1
      ? await Promise.all((await fetchJson(INDEX)).map((entry) => fetchJson(entry["@id"])))
      : JSON.parse(readFileSync(process.argv[i + 1], "utf8"));
  // A shape the check does not read would yield nothing, and pass: none is a failure.
  if (pages.every((page) => Object.keys(page).length === 0)) throw new Error("no advisories read from the feed");
  const found = advisories(pins, pages);
  for (const a of found) {
    process.stderr.write(`advisory: ${a.id} ${a.version} is covered by ${a.url} (severity ${a.severity}, versions ${a.versions})\n`);
  }
  if (found.length > 0) process.exit(1);
  process.stdout.write(`nuget advisories: none covers the ${pins.packages.length} pins\n`);
}

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  main().catch((e) => {
    process.stderr.write(`nuget advisory check: ${e instanceof Error ? e.message : String(e)}\n`);
    process.exit(1);
  });
}
