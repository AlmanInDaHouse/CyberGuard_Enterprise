#!/usr/bin/env node
// SPEC-024 Amendment 2026-11-01, point 1: each nuget package is restored by its pinned version
// and checked against its recorded hash before use. Neither `dotnet tool restore` nor the
// MSBuild SDK resolver checks one, and the lock file covers PackageReference items alone, so
// this script hashes the .nupkg NuGet retained against nuget.org's published packageHash. The
// retained .nupkg.sha512 is not read: `dotnet tool restore` writes the nuspec's hash there.
//
//   node deploy/windows/pins/verify-pins.mjs [--packages <dir>] [--tool]
//
// Run after `dotnet restore pins/Pins.csproj` and before anything loads a package. --tool also
// asks the restored `wix` for its build and EULA ID, after `dotnet tool restore`.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Where the pin files disagree with each other, or fall short of an exact pin. */
export function consistency({ pins, globalJson, toolManifest, pinsProject }) {
  const out = [];
  if (pins.eulaId !== "wix7") out.push(`pins.json names EULA ${pins.eulaId}; the EULA Manuel accepted is wix7`);
  if (globalJson.sdk?.version !== pins.dotnetSdk) out.push(`global.json pins the SDK at ${globalJson.sdk?.version}, pins.json at ${pins.dotnetSdk}`);
  if (globalJson.sdk?.rollForward !== "disable") out.push(`global.json sets rollForward to ${globalJson.sdk?.rollForward}, not disable`);

  // SPEC-024 Amendment 2026-11-14, point 3: the release's commit and its tree, by their full IDs,
  // and the informational version the tool reports naming that commit.
  for (const key of ["wixCommit", "wixTree"]) {
    if (!/^[0-9a-f]{40}$/.test(String(pins[key] ?? ""))) out.push(`pins.json's ${key} ${pins[key]} is not 40 lower-case hex digits`);
  }
  const short = /^[^+]+\+([0-9a-f]{7,40})$/.exec(String(pins.wixInformationalVersion ?? ""))?.[1];
  if (!short || !String(pins.wixCommit ?? "").startsWith(short)) {
    out.push(`pins.json's wixInformationalVersion ${pins.wixInformationalVersion} does not name wixCommit ${pins.wixCommit}`);
  }

  const seen = new Set();
  for (const p of pins.packages) {
    if (seen.has(p.id)) out.push(`${p.id} is listed twice in pins.json`);
    seen.add(p.id);
    if (p.version !== pins.wixRelease) out.push(`${p.id} is pinned at ${p.version}, off the release ${pins.wixRelease}`);
    if (Buffer.from(String(p.sha512), "base64").length !== 64) out.push(`${p.id}: its hash is not a base64 SHA-512`);
    if (typeof p.carried !== "boolean") out.push(`${p.id}: carried is not stated`);
    if (p.carried && !p.licence) out.push(`${p.id} is carried and names no licence`);
  }

  const sdk = globalJson["msbuild-sdks"]?.["WixToolset.Sdk"];
  if (sdk !== pins.wixRelease) out.push(`global.json pins WixToolset.Sdk at ${sdk}, off the release ${pins.wixRelease}`);
  const tool = toolManifest.tools?.wix?.version;
  if (tool !== pins.wixRelease) out.push(`the tool manifest pins wix at ${tool}, off the release ${pins.wixRelease}`);

  const matched = [...pinsProject.matchAll(/<PackageDownload\s+Include="([^"]+)"\s+Version="([^"]+)"\s*\/>/g)];
  const written = pinsProject.match(/<PackageDownload\b/g)?.length ?? 0;
  if (written !== matched.length) out.push(`Pins.csproj has a PackageDownload this check does not read: only the form <PackageDownload Include="…" Version="…" /> is pinned`);
  if (/<PackageReference\b/.test(pinsProject)) out.push("Pins.csproj has a PackageReference, which pins.json does not pin");
  const downloads = new Map(matched.map((m) => [m[1], m[2]]));
  for (const p of pins.packages) {
    const version = downloads.get(p.id);
    if (version === undefined) out.push(`Pins.csproj does not download ${p.id}`);
    else if (version !== `[${p.version}]`) out.push(`Pins.csproj takes ${p.id} at ${version}, not the exact [${p.version}]`);
  }
  for (const id of downloads.keys()) {
    if (!seen.has(id)) out.push(`Pins.csproj downloads ${id}, which pins.json does not pin`);
  }
  return out;
}

/** The pinned packages that are absent from NuGet's folder or differ from their recorded hash. */
export function verifyPackages(pins, packagesDir) {
  if (pins.packages.length === 0) return ["pins.json lists no package"];
  const out = [];
  for (const p of pins.packages) {
    const id = p.id.toLowerCase();
    const file = join(packagesDir, id, p.version, `${id}.${p.version}.nupkg`);
    if (!existsSync(file)) {
      out.push(`${p.id} ${p.version}: not found at ${file}`);
      continue;
    }
    const actual = createHash("sha512").update(readFileSync(file)).digest("base64");
    if (actual !== p.sha512) out.push(`${p.id} ${p.version}: its hash ${actual} is not the recorded ${p.sha512}`);
  }
  return out;
}

/** Whether `wix --version` and `wix eula --help` show the pinned build and the accepted EULA. */
export function toolProblems(pins, versionOutput, eulaHelp) {
  const out = [];
  const version = versionOutput.trim();
  if (version !== pins.wixInformationalVersion) out.push(`the wix tool reports ${version}, not ${pins.wixInformationalVersion}`);
  if (!new RegExp(`eulaId = ${pins.eulaId}\\b`).test(eulaHelp)) out.push(`the wix tool does not name the EULA ID ${pins.eulaId}`);
  return out;
}

function arg(name) {
  const i = process.argv.indexOf(name);
  return i === -1 ? undefined : process.argv[i + 1];
}

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  const json = (p) => JSON.parse(readFileSync(join(ROOT, p), "utf8"));
  // SPEC-024 Amendment 2026-11-01, point 1: the .NET CLI runs with its telemetry off.
  if (process.env.DOTNET_CLI_TELEMETRY_OPTOUT !== "1") {
    process.stderr.write("pin check: DOTNET_CLI_TELEMETRY_OPTOUT is not 1: the .NET CLI's telemetry must be off\n");
    process.exit(1);
  }
  const pins = json("pins/pins.json");
  const packages = arg("--packages") ?? process.env.NUGET_PACKAGES ?? join(homedir(), ".nuget", "packages");
  const problems = [
    ...consistency({
      pins,
      globalJson: json("global.json"),
      toolManifest: json(".config/dotnet-tools.json"),
      pinsProject: readFileSync(join(ROOT, "pins", "Pins.csproj"), "utf8"),
    }),
    ...verifyPackages(pins, packages),
  ];
  if (problems.length === 0 && process.argv.includes("--tool")) {
    // Help exits nonzero in this release, so the output is read and the status is not.
    const wix = (...args) => spawnSync("dotnet", ["tool", "run", "wix", "--", ...args], { cwd: ROOT, encoding: "utf8" }).stdout ?? "";
    problems.push(...toolProblems(pins, wix("--version"), wix("eula", "--help")));
  }
  for (const p of problems) process.stderr.write(`pin check: ${p}\n`);
  if (problems.length > 0) process.exit(1);
  process.stdout.write(`pins verified: ${pins.packages.length} packages at ${pins.wixRelease}\n`);
}
