#!/usr/bin/env node
// SPEC-024 Amendment 2026-11-14, points 2, 3 and 6: the release's Windows assets. release.yml's
// agent job is to run this on the assets it builds from the tag, before it attests them, and
// package-ci's dry run on the assets it builds at a fixture version (points 1 and 6).
//
//   node .github/scripts/release-windows.mjs check --version X.Y.Z --exe <cg-agent.exe> \
//     --msi <msi> --setup <setup.exe> --setup-exe <cg-setup.exe>
//
// runs every check of point 2 and prints one `PASS <check>` or `FAIL <check>: <reason>` line for
// each, going on past a failure, and exits 1 if any failed. It installs nothing and runs neither
// the MSI nor the setup: it reads the executables' PE images, opens the MSI's database read-only
// through msi.dll, and extracts the setup's and the MSI's payloads with the pinned `wix` tool.
//
//   node .github/scripts/release-windows.mjs archive --out <dir> [--ref <ref>]
//
// builds point 3's wix-<release>-source.tar.gz in <dir>: the pinned WiX commit's whole tree,
// fetched by its ID from WiX's repository with git's object checks on, and the setup's own
// sources from this repository at <ref> (HEAD by default). It reads the archive back and refuses
// it, removed and with the reason named, unless its wix-<release>/ is exactly the pinned tree and
// its LICENSE.TXT is the theme's copy.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const WINDOWS = join(REPO, "deploy", "windows");

/** The checks of point 2, in the order `check` prints them. */
export const CHECKS = [
  "agent --version",
  "agent version resource",
  "agent imports",
  "agent message table",
  "cg-setup.exe version resource",
  "msi product version",
  "msi file table",
  "msi custom actions",
  "setup version resource",
  "setup copyright",
  "setup registered version",
  "setup msi payload",
  "setup cg-setup.exe payloads",
  "msi agent payload",
];

/**
 * The DLLs the agent may import, each one part of every supported Windows (SPEC-001 NFR-004).
 * Beyond these, an API set other than the C runtime's is the system's too. This mirrors
 * SYSTEM_DLLS in agent/cg-agent/tests/package_executable.rs, which a test holds it to.
 */
export const SYSTEM_DLLS = Object.freeze([
  "advapi32.dll",
  "bcrypt.dll",
  "bcryptprimitives.dll",
  "combase.dll",
  "crypt32.dll",
  "kernel32.dll",
  "ntdll.dll",
  "ole32.dll",
  "oleaut32.dll",
  "shell32.dll",
  "tdh.dll",
  "wevtapi.dll",
  "ws2_32.dll",
]);

/** The agent's message table entries (the Amendment 2026-11-09, item 4). */
export const MESSAGE_IDS = Object.freeze([1000, 1001, 1002, 1003]);

/** VS_FF_PRERELEASE, the pre-release flag of a version resource's file flags (point 8). */
export const VS_FF_PRERELEASE = 0x2;

/** WiX's copyright line, as burn.exe states it, which the setup's version resource holds (points 2 and 10). */
export const WIX_COPYRIGHT_LINE = "Copyright (c) .NET Foundation and contributors. All rights reserved.";

/** Git's object checks on the fetch, both settings, so that no configuration turns them off (point 3). */
export const FETCH_CHECKS = Object.freeze(["-c", "transfer.fsckObjects=true", "-c", "fetch.fsckObjects=true"]);

/** WiX's upstream repository, from which point 3 fetches the pinned commit. */
export const WIX_REMOTE = "https://github.com/wixtoolset/wix";

/** The setup's own sources under deploy/windows/ that the archive holds (point 3); a path ending in / is a directory. */
export const SETUP_SOURCES = Object.freeze([
  "bundle/Bundle.wxs",
  "bundle/CyberGuardSetup.wixproj",
  "bundle/packages.lock.json",
  "Directory.Build.props",
  "Directory.Build.targets",
  "global.json",
  "nuget.config",
  "bundle/theme/",
]);

/** The theme's copy of WiX's LICENSE.TXT, which the licence report reads (point 4). */
const THEME_LICENCE = "bundle/theme/LICENSE.TXT";

const RT_MESSAGETABLE = 11;
const RT_VERSION = 16;

// --- PE images --------------------------------------------------------------------------

function u16(buf, at) {
  if (at < 0 || at + 2 > buf.length) throw new Error(`the image ends before offset ${at}`);
  return buf.readUInt16LE(at);
}

function u32(buf, at) {
  if (at < 0 || at + 4 > buf.length) throw new Error(`the image ends before offset ${at}`);
  return buf.readUInt32LE(at);
}

/** A PE image's headers: its data directories and the file offset of a relative virtual address. */
function peImage(buf) {
  if (buf.length < 0x40 || buf.toString("latin1", 0, 2) !== "MZ") throw new Error("not an MZ image");
  const pe = u32(buf, 0x3c);
  if (buf.toString("latin1", pe, pe + 4) !== "PE\0\0") throw new Error("no PE signature");
  const count = u16(buf, pe + 6);
  const optional = pe + 24;
  const magic = u16(buf, optional);
  if (magic !== 0x10b && magic !== 0x20b) throw new Error(`optional header magic ${magic.toString(16)} is neither PE32 nor PE32+`);
  const plus = magic === 0x20b;
  const imageBase = plus ? Number(buf.readBigUInt64LE(optional + 24)) : u32(buf, optional + 28);
  const directories = u32(buf, optional + (plus ? 108 : 92));
  const sectionTable = optional + u16(buf, pe + 20);
  const sections = [];
  for (let i = 0; i < count; i += 1) {
    const s = sectionTable + i * 40;
    sections.push({ va: u32(buf, s + 12), size: Math.max(u32(buf, s + 8), u32(buf, s + 16)), raw: u32(buf, s + 20) });
  }
  return {
    imageBase,
    directory(index) {
      if (index >= directories) return { rva: 0, size: 0 };
      const at = optional + (plus ? 112 : 96) + index * 8;
      return { rva: u32(buf, at), size: u32(buf, at + 4) };
    },
    offset(rva) {
      const s = sections.find((x) => rva >= x.va && rva < x.va + x.size);
      if (!s) throw new Error(`RVA 0x${rva.toString(16)} is in no section`);
      return rva - s.va + s.raw;
    },
  };
}

function cString(buf, at) {
  const end = buf.indexOf(0, at);
  if (end === -1) throw new Error(`no NUL after offset ${at}`);
  return buf.toString("latin1", at, end);
}

/**
 * The DLL names of the import and delay-import tables, each with its file offset, read as
 * package_executable.rs reads them.
 */
export function readImportEntries(buf) {
  const image = peImage(buf);
  const out = [];
  // [directory index, descriptor size, offset of its Name field]
  for (const [index, size, nameField] of [
    [1, 20, 12],
    [13, 32, 4],
  ]) {
    const { rva } = image.directory(index);
    if (rva === 0) continue;
    for (let at = image.offset(rva); ; at += size) {
      if (at + size > buf.length) throw new Error("an import table runs past the image");
      if (buf.subarray(at, at + size).every((b) => b === 0)) break;
      let name = u32(buf, at + nameField);
      // A delay-load descriptor whose attributes lack bit 0 holds virtual addresses, not RVAs.
      if (index === 13 && (u32(buf, at) & 1) === 0) name -= image.imageBase;
      const offset = image.offset(name);
      out.push({ name: cString(buf, offset), offset });
    }
  }
  return out;
}

/** The DLL names the image imports, delay-loaded ones included. */
export function readImports(buf) {
  return readImportEntries(buf).map((e) => e.name);
}

/** The resources of a PE image: type, name and language (an ID, or a string), and the file offset and size of each one's data. */
export function readResources(buf) {
  const image = peImage(buf);
  const { rva } = image.directory(2);
  if (rva === 0) return [];
  const base = image.offset(rva);
  const out = [];
  const seen = new Set();
  const walk = (at, path) => {
    if (seen.has(at)) throw new Error("the resource directory loops");
    seen.add(at);
    const entries = u16(buf, at + 12) + u16(buf, at + 14);
    for (let i = 0; i < entries; i += 1) {
      const entry = at + 16 + i * 8;
      const nameField = u32(buf, entry);
      const dataField = u32(buf, entry + 4);
      let id = nameField & 0xffff;
      if (nameField & 0x80000000) {
        const s = base + (nameField & 0x7fffffff);
        id = buf.toString("utf16le", s + 2, s + 2 + u16(buf, s) * 2);
      }
      const subdirectory = (dataField & 0x80000000) !== 0;
      if (subdirectory !== path.length < 2) throw new Error("the resource directory is not type, name and language");
      if (subdirectory) {
        walk(base + (dataField & 0x7fffffff), [...path, id]);
      } else {
        const leaf = base + dataField;
        const size = u32(buf, leaf + 4);
        const offset = image.offset(u32(buf, leaf));
        if (offset + size > buf.length) throw new Error("a resource runs past the image");
        out.push({ type: path[0], name: path[1], language: id, offset, size });
      }
    }
  };
  walk(base, []);
  return out;
}

/** One block of a version resource: its key, its value's place, and its children. */
function versionBlock(buf, at, base) {
  const align = (p) => p + ((4 - ((p - base) % 4)) % 4);
  const length = u16(buf, at);
  const valueLength = u16(buf, at + 2);
  const type = u16(buf, at + 4);
  if (length < 6) throw new Error("a version block is shorter than its header");
  const end = at + length;
  let p = at + 6;
  let key = "";
  for (; p + 2 <= end && u16(buf, p) !== 0; p += 2) key += String.fromCharCode(u16(buf, p));
  p = align(p + 2);
  const valueAt = p;
  // wType 1 counts a text value in characters, wType 0 a binary one in bytes; WiX's binder writes
  // a string's value as wType 0. Its text is read up to its NUL, within the value and the block.
  p = Math.min(end, valueAt + (type === 1 ? valueLength * 2 : valueLength));
  let q = valueAt;
  while (q + 2 <= p && u16(buf, q) !== 0) q += 2;
  const text = buf.toString("utf16le", valueAt, q);
  const children = [];
  for (p = align(p); p + 6 <= end; ) {
    const child = versionBlock(buf, p, base);
    if (child.length === 0) break;
    children.push(child);
    p = align(p + child.length);
  }
  return { key, length, valueAt, valueLength, text, children };
}

/**
 * The version resource: the numeric file and product versions as [a, b, c, d], the file flags and
 * their mask, the file offset of VS_FIXEDFILEINFO, and the strings of its first string table (all
 * of them under stringTables, by language and code page).
 */
export function readVersionInfo(buf) {
  const found = readResources(buf).filter((r) => r.type === RT_VERSION);
  if (found.length !== 1) throw new Error(`the image holds ${found.length} version resources, not one`);
  const { offset } = found[0];
  const root = versionBlock(buf, offset, offset);
  if (root.key !== "VS_VERSION_INFO") throw new Error(`the version resource's key is ${JSON.stringify(root.key)}`);
  const fixed = root.valueAt;
  if (root.valueLength < 52 || u32(buf, fixed) !== 0xfeef04bd) throw new Error("the version resource has no VS_FIXEDFILEINFO");
  const version = (ms, ls) => [u32(buf, ms) >>> 16, u32(buf, ms) & 0xffff, u32(buf, ls) >>> 16, u32(buf, ls) & 0xffff];
  const stringTables = {};
  for (const info of root.children.filter((c) => c.key === "StringFileInfo")) {
    for (const table of info.children) {
      stringTables[table.key] = Object.fromEntries(table.children.map((s) => [s.key, s.text ?? ""]));
    }
  }
  return {
    fileVersion: version(fixed + 8, fixed + 12),
    productVersion: version(fixed + 16, fixed + 20),
    fileFlagsMask: u32(buf, fixed + 24),
    fileFlags: u32(buf, fixed + 28),
    fixedOffset: fixed,
    strings: Object.values(stringTables)[0] ?? {},
    stringTables,
  };
}

/** The IDs of the entries of the image's message tables, in every language, sorted. */
export function readMessageIds(buf) {
  const ids = new Set();
  for (const r of readResources(buf).filter((x) => x.type === RT_MESSAGETABLE)) {
    const blocks = u32(buf, r.offset);
    if (4 + blocks * 12 > r.size) throw new Error("a message table's blocks run past it");
    for (let i = 0; i < blocks; i += 1) {
      const low = u32(buf, r.offset + 4 + i * 12);
      const high = u32(buf, r.offset + 8 + i * 12);
      if (high < low || high - low > 0xffff) throw new Error(`a message block runs from ${low} to ${high}`);
      for (let id = low; id <= high; id += 1) ids.add(id);
    }
  }
  return [...ids].sort((a, b) => a - b);
}

// --- Point 2's checks, on what the readers read -----------------------------------------

const quoted = (v) => (v === undefined ? "(none)" : JSON.stringify(v));

/** The imports outside SYSTEM_DLLS and the API sets other than the C runtime's. */
export function foreignImports(names) {
  return names.filter((name) => {
    const n = name.toLowerCase();
    const apiSet = n.startsWith("api-ms-win-") && !n.startsWith("api-ms-win-crt-");
    return !apiSet && !SYSTEM_DLLS.includes(n);
  });
}

export function importProblems(names) {
  if (names.length === 0) return ["no import was read"];
  const foreign = foreignImports(names);
  return foreign.length === 0 ? [] : [`cg-agent.exe needs a DLL the endpoint may not have: ${foreign.join(", ")}`];
}

export function versionOutputProblems(output, version) {
  const printed = output.replace(/(\r?\n)+$/, "");
  return printed === `cg-agent ${version}` ? [] : [`--version printed ${quoted(printed)}, not "cg-agent ${version}"`];
}

function prereleaseProblems(info) {
  return info.fileFlags & VS_FF_PRERELEASE ? [`the pre-release flag (VS_FF_PRERELEASE) is set in its file flags 0x${info.fileFlags.toString(16)}`] : [];
}

function stringProblems(info, keys, version) {
  return keys.filter((k) => info.strings[k] !== version).map((k) => `${k} ${quoted(info.strings[k])}, not "${version}"`);
}

/** The agent's version resource: X.Y.Z.0 in numbers, X.Y.Z in strings, and no pre-release flag. */
export function agentVersionProblems(info, version) {
  const numeric = `${version}.0`;
  const out = [];
  for (const [what, v] of [
    ["file", info.fileVersion],
    ["product", info.productVersion],
  ]) {
    if (v.join(".") !== numeric) out.push(`numeric ${what} version ${v.join(".")}, not ${numeric}`);
  }
  return [...out, ...stringProblems(info, ["FileVersion", "ProductVersion"], version), ...prereleaseProblems(info)];
}

/** cg-setup.exe's ProductVersion string, and no pre-release flag. */
export function setupExeVersionProblems(info, version) {
  return [...stringProblems(info, ["ProductVersion"], version), ...prereleaseProblems(info)];
}

/** The setup's ProductVersion string. */
export function setupVersionProblems(info, version) {
  return stringProblems(info, ["ProductVersion"], version);
}

/** The setup's LegalCopyright holds WiX's line (point 10). */
export function setupCopyrightProblems(info) {
  const c = info.strings.LegalCopyright;
  return c?.includes(WIX_COPYRIGHT_LINE) ? [] : [`LegalCopyright ${quoted(c)} lacks WiX's line ${quoted(WIX_COPYRIGHT_LINE)}`];
}

/** The message table holds entries 1000 to 1003, and neither 999 nor 1004 (point 2). */
export function messageIdProblems(ids) {
  const out = MESSAGE_IDS.filter((id) => !ids.includes(id)).map((id) => `entry ${id} is missing`);
  for (const id of [999, 1004].filter((x) => ids.includes(x))) out.push(`entry ${id} is in the table, beside 1000 to 1003`);
  return out;
}

// --- The MSI, through msi.dll -------------------------------------------------------------

// Its database opened read-only (msiOpenDatabaseModeReadOnly, 0); one tab-separated line a row.
const MSI_SCRIPT = `
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
function Call($o, $m, $a) { $o.GetType().InvokeMember($m, 'InvokeMethod', $null, $o, $a) }
$installer = New-Object -ComObject WindowsInstaller.Installer
$db = Call $installer 'OpenDatabase' @($env:CG_MSI, 0)
function Rows($tag, $sql) {
  $view = Call $db 'OpenView' @($sql)
  [void](Call $view 'Execute' $null)
  while ($null -ne ($r = Call $view 'Fetch' $null)) {
    $n = [int]$r.GetType().InvokeMember('FieldCount', 'GetProperty', $null, $r, $null)
    $fields = @($tag)
    for ($k = 1; $k -le $n; $k++) { $fields += [string]$r.GetType().InvokeMember('StringData', 'GetProperty', $null, $r, [object[]]@([int]$k)) }
    [Console]::Out.WriteLine(($fields -join "\`t"))
  }
  [void](Call $view 'Close' $null)
}
Rows 'table' 'SELECT \`Name\` FROM \`_Tables\`'
Rows 'version' 'SELECT \`Value\` FROM \`Property\` WHERE \`Property\` = ''ProductVersion'''
Rows 'file' 'SELECT \`File\`, \`FileName\` FROM \`File\`'
$tables = @()
$view = Call $db 'OpenView' @('SELECT \`Name\` FROM \`_Tables\`')
[void](Call $view 'Execute' $null)
while ($null -ne ($r = Call $view 'Fetch' $null)) { $tables += [string]$r.GetType().InvokeMember('StringData', 'GetProperty', $null, $r, [object[]]@([int]1)) }
[void](Call $view 'Close' $null)
if ($tables -contains 'CustomAction') { Rows 'action' 'SELECT \`Action\` FROM \`CustomAction\`' }
if ($tables -contains 'Binary') { Rows 'binary' 'SELECT \`Name\` FROM \`Binary\`' }
[Console]::Out.WriteLine('end')
`;

/**
 * PowerShell 7 where it is installed: Windows PowerShell 5.1 fails as the runner's Administrator
 * (CLAUDE.md, *Windows CI job*), so on a GitHub runner its absence is a failure, not a fallback.
 */
function powershell() {
  if (spawnSync("pwsh", ["-NoProfile", "-NonInteractive", "-Command", "exit 0"]).status === 0) return "pwsh";
  if (process.env.GITHUB_ACTIONS === "true") throw new Error("pwsh is not on this runner: the MSI is read through PowerShell 7 alone");
  return "powershell";
}

/** The MSI's tables, ProductVersion, File rows, custom actions and Binary rows, read through msi.dll. */
export function readMsi(path) {
  const r = spawnSync(powershell(), ["-NoProfile", "-NonInteractive", "-Command", MSI_SCRIPT], {
    encoding: "utf8",
    env: { ...process.env, CG_MSI: resolve(path) },
  });
  const lines = (r.stdout ?? "").split(/\r?\n/).filter((l) => l !== "");
  if (r.status !== 0 || lines.at(-1) !== "end") {
    throw new Error(`msi.dll did not read ${path}: exit ${r.status}: ${(r.stderr ?? "").trim().split(/\r?\n/)[0] ?? ""}`);
  }
  const facts = { tables: [], productVersion: [], files: [], customActions: [], binaries: [] };
  for (const line of lines.slice(0, -1)) {
    const [tag, ...fields] = line.split("\t");
    if (tag === "table") facts.tables.push(fields[0]);
    else if (tag === "version") facts.productVersion.push(fields[0]);
    else if (tag === "file") facts.files.push({ key: fields[0], name: fields[1] });
    else if (tag === "action") facts.customActions.push(fields[0]);
    else if (tag === "binary") facts.binaries.push(fields[0]);
  }
  return facts;
}

export function msiVersionProblems(facts, version) {
  if (facts.productVersion.length !== 1) return [`the Property table holds ${facts.productVersion.length} ProductVersion rows, not one`];
  return facts.productVersion[0] === version ? [] : [`ProductVersion ${facts.productVersion[0]}, not ${version}`];
}

/** The File table holds one file, the agent's executable (a FileName is short|long, or one name). */
export function msiFileProblems(facts) {
  const longName = (f) => f.name.split("|").at(-1);
  if (facts.files.length !== 1) {
    return [`the File table holds ${facts.files.length} files, not one: ${facts.files.map((f) => `${f.key} (${f.name})`).join(", ")}`];
  }
  const name = longName(facts.files[0]);
  return name === "cg-agent.exe" ? [] : [`the File table's one file is ${name}, not cg-agent.exe`];
}

/** No Binary table, and the CustomAction table absent or empty (#158). */
export function msiCustomActionProblems(facts) {
  const out = [];
  if (facts.tables.includes("Binary")) out.push(`the database has a Binary table${facts.binaries.length > 0 ? ` (${facts.binaries.join(", ")})` : ""}`);
  if (facts.customActions.length > 0) out.push(`the CustomAction table holds ${facts.customActions.join(", ")}`);
  return out;
}

// --- The setup's manifest and payloads -------------------------------------------------------

function attributes(text) {
  const entities = { quot: '"', amp: "&", lt: "<", gt: ">", apos: "'" };
  const out = {};
  for (const m of text.matchAll(/([A-Za-z_][\w.-]*)="([^"]*)"/g)) {
    out[m[1]] = m[2].replace(/&(quot|amp|lt|gt|apos);/g, (_, e) => entities[e]);
  }
  return out;
}

/** Burn's manifest.xml: the versions its Registration elements register, and its payloads. */
export function parseManifest(xml) {
  const elements = (name) => [...xml.matchAll(new RegExp(`<${name}\\s([^>]*?)/?>`, "g"))].map((m) => attributes(m[1]));
  return {
    registrations: elements("Registration").map((a) => ({ version: a.Version })),
    payloads: elements("Payload").map((a) => ({ id: a.Id, filePath: a.FilePath, sourcePath: a.SourcePath, container: a.Container })),
  };
}

export function registeredVersionProblems(manifest, version) {
  if (manifest.registrations.length !== 1) return [`the manifest has ${manifest.registrations.length} Registration elements, not one`];
  const registered = manifest.registrations[0].version;
  return registered === version ? [] : [`the manifest registers ${registered}, not ${version}`];
}

/**
 * The payloads the setup's container carries, by their bytes: its one MSI is the MSI asset, and
 * each cg-setup.exe is the one built. `digestOf(payload)` is the SHA-256 of the extracted file,
 * or undefined when it was not extracted.
 */
export function payloadProblems(manifest, digestOf, { msi, setupExe }) {
  const contained = manifest.payloads.filter((p) => p.container);
  const base = (p) => p.filePath.split(/[\\/]/).at(-1).toLowerCase();
  const compare = (payloads, digest, what) =>
    payloads.flatMap((p) => {
      const actual = digestOf(p);
      if (actual === undefined) return [`the payload ${p.filePath} was not extracted`];
      return actual === digest ? [] : [`the payload ${p.filePath} is not ${what}`];
    });
  const msis = contained.filter((p) => base(p).endsWith(".msi"));
  const setups = contained.filter((p) => base(p) === "cg-setup.exe");
  return {
    msi: msis.length !== 1 ? [`the setup carries ${msis.length} MSI payloads, not one`] : compare(msis, msi, "the MSI asset"),
    setupExe: setups.length === 0 ? ["the setup carries no cg-setup.exe"] : compare(setups, setupExe, "cg-setup.exe"),
  };
}

// --- The check -------------------------------------------------------------------------------

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function pins() {
  return JSON.parse(readFileSync(join(WINDOWS, "pins", "pins.json"), "utf8"));
}

/** The pinned `wix` tool, from deploy/windows/, under the EULA Manuel accepted. */
function wix(args) {
  const r = spawnSync("dotnet", ["tool", "run", "wix", "--", ...args, "-acceptEula", pins().eulaId], {
    cwd: WINDOWS,
    encoding: "utf8",
    env: { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1" },
  });
  if (r.status !== 0) {
    const said = `${r.stdout ?? ""}${r.stderr ?? ""}`.trim().split(/\r?\n/).filter((l) => /error/i.test(l)).join(" | ");
    throw new Error(`wix ${args.slice(0, 2).join(" ")} exited ${r.status}${said ? `: ${said}` : ""}`);
  }
}

/** Every check of point 2 on the assets, each one's problems listed; none stops the others. */
export function check({ version, exe, msi, setup, setupExe }) {
  const results = [];
  const run = (name, fn) => {
    let problems;
    try {
      problems = fn();
    } catch (e) {
      problems = [e instanceof Error ? e.message : String(e)];
    }
    results.push({ check: name, problems });
  };
  const once = (fn) => {
    let value;
    let error;
    let done = false;
    return () => {
      if (!done) {
        done = true;
        try {
          value = fn();
        } catch (e) {
          error = e;
        }
      }
      if (error) throw error;
      return value;
    };
  };

  const agent = once(() => readFileSync(exe));
  run("agent --version", () => {
    const r = spawnSync(exe, ["--version"], { encoding: "utf8", timeout: 60_000 });
    if (r.error) throw r.error;
    return [...(r.status === 0 ? [] : [`--version exited ${r.status}`]), ...versionOutputProblems(r.stdout ?? "", version)];
  });
  run("agent version resource", () => agentVersionProblems(readVersionInfo(agent()), version));
  run("agent imports", () => importProblems(readImports(agent())));
  run("agent message table", () => messageIdProblems(readMessageIds(agent())));
  run("cg-setup.exe version resource", () => setupExeVersionProblems(readVersionInfo(readFileSync(setupExe)), version));

  const facts = once(() => readMsi(msi));
  run("msi product version", () => msiVersionProblems(facts(), version));
  run("msi file table", () => msiFileProblems(facts()));
  run("msi custom actions", () => msiCustomActionProblems(facts()));

  const setupInfo = once(() => readVersionInfo(readFileSync(setup)));
  run("setup version resource", () => setupVersionProblems(setupInfo(), version));
  run("setup copyright", () => setupCopyrightProblems(setupInfo()));

  const scratch = mkdtempSync(join(tmpdir(), "cg-release-check-"));
  try {
    const extracted = once(() => {
      wix(["burn", "extract", resolve(setup), "-o", join(scratch, "c"), "-oba", join(scratch, "ba")]);
      return parseManifest(readFileSync(join(scratch, "ba", "manifest.xml"), "utf8"));
    });
    // Each payload where the manifest places it, its FilePath under its container's folder.
    const digestOf = (p) => {
      for (const name of [p.filePath, p.sourcePath]) {
        const file = join(scratch, "c", p.container, ...name.split(/[\\/]/));
        if (existsSync(file)) return sha256(readFileSync(file));
      }
      return undefined;
    };
    const payloads = once(() => payloadProblems(extracted(), digestOf, { msi: sha256(readFileSync(msi)), setupExe: sha256(readFileSync(setupExe)) }));
    run("setup registered version", () => registeredVersionProblems(extracted(), version));
    run("setup msi payload", () => payloads().msi);
    run("setup cg-setup.exe payloads", () => payloads().setupExe);

    run("msi agent payload", () => {
      const files = facts().files;
      if (files.length !== 1) return [`the File table holds ${files.length} files, so the agent's is not known`];
      wix(["msi", "decompile", resolve(msi), "-x", join(scratch, "m"), "-o", join(scratch, "m.wxs")]);
      const file = join(scratch, "m", "File", files[0].key);
      if (!existsSync(file)) return [`the MSI's file ${files[0].key} was not extracted`];
      return sha256(readFileSync(file)) === sha256(agent()) ? [] : [`the MSI's ${files[0].key} is not the raw executable`];
    });
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  return results;
}

// --- The source archive (point 3) ------------------------------------------------------------

/** A git blob's ID: the SHA-1 of "blob <length>\0" and the bytes. */
export function blobId(bytes) {
  return createHash("sha1").update(`blob ${bytes.length}\0`).update(bytes).digest("hex");
}

/** The ID of the git tree that holds `entries` ({ mode, id, path }), its subtrees written as git writes them. */
export function treeId(entries) {
  const root = new Map();
  for (const e of entries) {
    const parts = e.path.split("/");
    let dir = root;
    for (const part of parts.slice(0, -1)) {
      if (!dir.has(part)) dir.set(part, new Map());
      dir = dir.get(part);
      if (!(dir instanceof Map)) throw new Error(`${e.path} is under a file`);
    }
    dir.set(parts.at(-1), e);
  }
  const hash = (dir) => {
    // git sorts a subtree's name as if it ended with "/".
    const names = [...dir.keys()].sort((a, b) => {
      const ka = Buffer.from(dir.get(a) instanceof Map ? `${a}/` : a);
      const kb = Buffer.from(dir.get(b) instanceof Map ? `${b}/` : b);
      return Buffer.compare(ka, kb);
    });
    const body = Buffer.concat(
      names.map((name) => {
        const v = dir.get(name);
        const [mode, id] = v instanceof Map ? ["40000", hash(v)] : [v.mode, v.id];
        return Buffer.concat([Buffer.from(`${mode} ${name}\0`), Buffer.from(id, "hex")]);
      }),
    );
    return createHash("sha1").update(`tree ${body.length}\0`).update(body).digest("hex");
  };
  return hash(root);
}

function octal(h, at, length) {
  const field = h.subarray(at, at + length);
  if (field[0] & 0x80) {
    // base-256, for a size past octal's eleven digits
    let v = 0;
    for (let i = 1; i < field.length; i += 1) v = v * 256 + field[i];
    return v;
  }
  const s = field.toString("latin1").replace(/\0.*$/s, "").trim();
  return s === "" ? 0 : Number.parseInt(s, 8);
}

function paxRecords(data) {
  const out = {};
  for (let at = 0; at < data.length; ) {
    const space = data.indexOf(0x20, at);
    const length = Number(data.toString("latin1", at, space));
    if (space === -1 || !Number.isInteger(length) || length <= 0) throw new Error("a malformed pax record");
    const record = data.toString("utf8", space + 1, at + length - 1);
    const eq = record.indexOf("=");
    out[record.slice(0, eq)] = record.slice(eq + 1);
    at += length;
  }
  return out;
}

/**
 * The entries of a tar.gz as git archive writes it (ustar headers, pax extended headers):
 * { path, type: file | directory | symlink | the type flag, mode, bytes, linkname }.
 */
export function readTarGz(gz) {
  const tar = gunzipSync(gz);
  const out = [];
  let pax = {};
  for (let at = 0; ; ) {
    if (at + 512 > tar.length) throw new Error("the archive is truncated: no end-of-archive blocks");
    const h = tar.subarray(at, at + 512);
    if (h.every((b) => b === 0)) break;
    let sum = 0;
    for (let i = 0; i < 512; i += 1) sum += i >= 148 && i < 156 ? 0x20 : h[i];
    if (sum !== octal(h, 148, 8)) throw new Error(`a header's checksum does not match at offset ${at}`);
    const flag = String.fromCharCode(h[156]);
    const size = pax.size !== undefined && flag !== "x" && flag !== "g" ? Number(pax.size) : octal(h, 124, 12);
    const start = at + 512;
    if (start + size > tar.length) throw new Error("the archive is truncated inside an entry");
    const data = tar.subarray(start, start + size);
    at = start + Math.ceil(size / 512) * 512;
    if (flag === "x") {
      pax = paxRecords(data);
      continue;
    }
    if (flag === "g") continue; // git writes the commit's ID there, which a tree has none of
    const name = h.toString("utf8", 0, 100).replace(/\0.*$/s, "");
    const prefix = h.toString("latin1", 257, 262) === "ustar" ? h.toString("utf8", 345, 500).replace(/\0.*$/s, "") : "";
    const path = (pax.path ?? (prefix ? `${prefix}/${name}` : name)).replace(/\/$/, "");
    const linkname = pax.linkpath ?? h.toString("utf8", 157, 257).replace(/\0.*$/s, "");
    const type = flag === "0" || flag === "\0" ? "file" : flag === "5" ? "directory" : flag === "2" ? "symlink" : `type ${flag}`;
    out.push({ path, type, mode: octal(h, 100, 8), bytes: Buffer.from(data), linkname });
    pax = {};
  }
  return out;
}

/**
 * What point 3 asks of the archive: its wix-<release>/ holds exactly the pinned tree's files, each
 * the blob the tree holds, with its mode, so that the tree's ID recomputes; its cyberguard-setup/
 * holds the setup's sources; and WiX's LICENSE.TXT, the theme's copy in the archive and the
 * repository's copy (`licence`) are the same bytes. `tree` is `git ls-tree -r` of the pinned tree;
 * `setupFiles` are { path under deploy/windows/, bytes }.
 */
export function verifyArchive(entries, { release, wixTree, tree, setupFiles, licence }) {
  const wixPrefix = `wix-${release}/`;
  const setupPrefix = "cyberguard-setup/deploy/windows/";
  const out = [];
  const files = new Map();
  for (const e of entries) {
    if (e.type === "directory") continue;
    if (!e.path.startsWith(wixPrefix) && !e.path.startsWith(setupPrefix)) {
      out.push(`${e.path} is outside ${wixPrefix} and cyberguard-setup/`);
      continue;
    }
    if (e.type !== "file" && e.type !== "symlink") out.push(`${e.path} is a tar ${e.type}, neither a file nor a link`);
    if (files.has(e.path)) out.push(`${e.path} is in the archive twice`);
    files.set(e.path, e);
  }

  const archived = [];
  const named = new Set();
  for (const t of tree) {
    const path = `${wixPrefix}${t.path}`;
    named.add(path);
    if (t.type !== "blob") {
      out.push(`${path} is a ${t.type} in the pinned tree, whose content the archive cannot hold`);
      continue;
    }
    const e = files.get(path);
    if (!e) {
      out.push(`the archive lacks ${path}`);
      continue;
    }
    const content = e.type === "symlink" ? Buffer.from(e.linkname, "utf8") : e.bytes;
    const mode = e.type === "symlink" ? "120000" : e.mode & 0o111 ? "100755" : "100644";
    const id = blobId(content);
    if (id !== t.id) out.push(`${path} is not the blob ${t.id} the pinned tree holds`);
    if (mode !== t.mode) out.push(`${path} has mode ${mode}, not the tree's ${t.mode}`);
    archived.push({ mode, id, path: t.path });
  }
  for (const path of files.keys()) {
    if (path.startsWith(wixPrefix) && !named.has(path)) {
      out.push(`${path} is not in the pinned tree`);
      const e = files.get(path);
      const content = e.type === "symlink" ? Buffer.from(e.linkname, "utf8") : e.bytes;
      archived.push({ mode: e.type === "symlink" ? "120000" : e.mode & 0o111 ? "100755" : "100644", id: blobId(content), path: path.slice(wixPrefix.length) });
    }
  }
  const recomputed = archived.length > 0 ? treeId(archived) : "(nothing)";
  if (recomputed !== wixTree) out.push(`the archive's ${wixPrefix} recomputes to tree ${recomputed}, not the pinned ${wixTree}`);

  const setupNamed = new Set();
  for (const f of setupFiles) {
    const path = `${setupPrefix}${f.path}`;
    setupNamed.add(path);
    const e = files.get(path);
    if (!e) out.push(`the archive lacks ${path}`);
    else if (!e.bytes.equals(f.bytes)) out.push(`${path} is not the repository's file`);
  }
  for (const path of files.keys()) {
    if (path.startsWith(setupPrefix) && !setupNamed.has(path)) out.push(`${path} is not one of the setup's sources`);
  }

  for (const path of [`${wixPrefix}LICENSE.TXT`, `${setupPrefix}${THEME_LICENCE}`]) {
    const e = files.get(path);
    if (!e) out.push(`the archive lacks ${path}`);
    else if (!e.bytes.equals(licence)) out.push(`${path} is not byte-identical to the repository's deploy/windows/${THEME_LICENCE}`);
  }
  return [...new Set(out)];
}

/** The variables a call of git may set itself: the index and work tree of the archive's tree. */
const GIT_OWN = Object.freeze(["GIT_INDEX_FILE", "GIT_WORK_TREE"]);

/**
 * The environment git runs in for the archive (point 3): `parent` without any GIT_* variable, so
 * that none it inherits (GIT_CONFIG_PARAMETERS, GIT_CONFIG_COUNT, GIT_ALTERNATE_OBJECT_DIRECTORIES,
 * GIT_DIR, ...) reaches it; no system configuration and no global one (the null device, which Git
 * for Windows reads as such too), so that no url.insteadOf, fsck or attribute setting of the host
 * changes the fetch or the archive; then the variables the call sets itself, GIT_OWN alone.
 */
export function gitEnvironment(parent, own = {}) {
  const extra = Object.keys(own).filter((k) => !GIT_OWN.includes(k));
  if (extra.length > 0) throw new Error(`git's environment takes ${GIT_OWN.join(" and ")} alone, not ${extra.join(", ")}`);
  const env = Object.fromEntries(Object.entries(parent).filter(([k]) => !/^GIT_/i.test(k)));
  return { ...env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null", ...own };
}

function git(args, { cwd, input, env, gitDir } = {}) {
  const r = spawnSync("git", [...(gitDir ? ["--git-dir", gitDir] : []), ...args], {
    cwd,
    input,
    env: gitEnvironment(process.env, env),
    maxBuffer: 1024 * 1024 * 1024,
  });
  if (r.error) throw r.error;
  if (r.status !== 0) throw new Error(`git ${args.join(" ")} exited ${r.status}: ${r.stderr.toString("utf8").trim()}`);
  return r.stdout;
}

/** `git ls-tree -r -z`'s entries: { mode, type, id, path }. */
function lsTree(gitDir, treeish, paths = []) {
  return git(["ls-tree", "-r", "-z", "--full-tree", treeish, "--", ...paths], { gitDir })
    .toString("utf8")
    .split("\0")
    .filter((l) => l !== "")
    .map((l) => {
      const tab = l.indexOf("\t");
      const [mode, type, id] = l.slice(0, tab).split(" ");
      return { mode, type, id, path: l.slice(tab + 1) };
    });
}

/**
 * Builds point 3's archive in `out` and verifies it, or throws naming the reason and leaves no
 * archive. `repo` is this repository's git directory (or working tree), `ref` the release's tag;
 * `remote` is WiX's repository, another only in tests.
 */
export function buildArchive({ repo = REPO, ref = "HEAD", out, remote = WIX_REMOTE }) {
  const repoDir = git(["rev-parse", "--absolute-git-dir"], { cwd: repo }).toString("utf8").trim();
  const pinned = JSON.parse(git(["cat-file", "blob", `${ref}:deploy/windows/pins/pins.json`], { gitDir: repoDir }).toString("utf8"));
  const { wixRelease: release, wixCommit: commit, wixTree: tree } = pinned;
  if (!/^\d+\.\d+\.\d+$/.test(String(release))) throw new Error(`pins.json's wixRelease ${release} is not X.Y.Z`);
  for (const [key, id] of [
    ["wixCommit", commit],
    ["wixTree", tree],
  ]) {
    if (!/^[0-9a-f]{40}$/.test(String(id))) throw new Error(`pins.json's ${key} ${id} is not 40 lower-case hex digits`);
  }

  // The setup's sources at the ref, read before anything is fetched.
  const sources = [];
  for (const source of SETUP_SOURCES) {
    const found = lsTree(repoDir, ref, [`deploy/windows/${source}`]);
    if (found.length === 0) throw new Error(`the repository has no deploy/windows/${source} at ${ref}`);
    for (const f of found) {
      if (f.type !== "blob" || (f.mode !== "100644" && f.mode !== "100755")) throw new Error(`${f.path} at ${ref} is a ${f.mode} ${f.type}, not a file`);
      sources.push({ ...f, path: f.path.slice("deploy/windows/".length), bytes: git(["cat-file", "blob", f.id], { gitDir: repoDir }) });
    }
  }
  const licence = sources.find((s) => s.path === THEME_LICENCE)?.bytes;
  if (!licence) throw new Error(`the repository has no deploy/windows/${THEME_LICENCE} at ${ref}`);

  // The tag must name the pinned commit: an annotated tag's peeled line, else its own.
  const tagRef = `refs/tags/v${release}`;
  const advertised = new Map(
    git(["ls-remote", remote, tagRef, `${tagRef}^{}`])
      .toString("utf8")
      .split(/\r?\n/)
      .filter((l) => l !== "")
      .map((l) => l.split("\t").reverse()),
  );
  const named = advertised.get(`${tagRef}^{}`) ?? advertised.get(tagRef);
  if (!named) throw new Error(`the tag v${release} is not at ${remote}`);
  if (named !== commit) throw new Error(`the tag v${release} at ${remote} names ${named}, not the pinned ${commit}`);

  const scratch = mkdtempSync(join(tmpdir(), "cg-release-archive-"));
  const archive = join(resolve(out), `wix-${release}-source.tar.gz`);
  try {
    const wixDir = join(scratch, "wix.git");
    git(["init", "--bare", "-q", wixDir]);
    // No attribute converts or leaves out a file: these override any .gitattributes in the tree.
    mkdirSync(join(wixDir, "info"), { recursive: true });
    writeFileSync(join(wixDir, "info", "attributes"), "* -text -eol -crlf -ident -filter -diff -export-ignore -export-subst -working-tree-encoding\n");
    git([...FETCH_CHECKS, "fetch", "--quiet", "--no-tags", "--depth", "1", remote, commit], { gitDir: wixDir });
    const kind = git(["cat-file", "-t", commit], { gitDir: wixDir }).toString("utf8").trim();
    if (kind !== "commit") throw new Error(`${commit} is a ${kind}, not a commit`);
    const actualTree = git(["rev-parse", `${commit}^{tree}`], { gitDir: wixDir }).toString("utf8").trim();
    if (actualTree !== tree) throw new Error(`the commit ${commit}'s tree is ${actualTree}, not the pinned ${tree}`);

    // One tree: the commit's under wix-<release>/, the setup's sources under cyberguard-setup/,
    // built in an index of its own. read-tree asks for a work tree, an empty one it never writes.
    const index = { GIT_INDEX_FILE: join(scratch, "index"), GIT_WORK_TREE: join(scratch, "empty") };
    mkdirSync(index.GIT_WORK_TREE);
    git(["read-tree", `--prefix=wix-${release}/`, commit], { gitDir: wixDir, env: index });
    for (const s of sources) {
      const id = git(["hash-object", "-w", "--no-filters", "--stdin"], { gitDir: wixDir, input: s.bytes }).toString("utf8").trim();
      if (id !== s.id) throw new Error(`deploy/windows/${s.path} hashed to ${id}, not its blob ${s.id}`);
      git(["update-index", "--add", "--cacheinfo", `${s.mode},${id},cyberguard-setup/deploy/windows/${s.path}`], { gitDir: wixDir, env: index });
    }
    const combined = git(["write-tree"], { gitDir: wixDir, env: index }).toString("utf8").trim();

    mkdirSync(dirname(archive), { recursive: true });
    git(["-c", "core.autocrlf=false", "-c", "core.eol=lf", "archive", "--format=tar.gz", "-o", archive, combined], { gitDir: wixDir });

    const pinnedTree = lsTree(wixDir, tree);
    const problems = verifyArchive(readTarGz(readFileSync(archive)), {
      release,
      wixTree: tree,
      tree: pinnedTree,
      setupFiles: sources.map((s) => ({ path: s.path, bytes: s.bytes })),
      licence,
    });
    if (problems.length > 0) throw new Error(`the archive is refused: ${problems.join("; ")}`);
    return { archive, release, commit, tree, wixFiles: pinnedTree.length, setupFiles: sources.length, size: statSync(archive).size };
  } catch (e) {
    rmSync(archive, { force: true });
    throw e;
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

// --- The command --------------------------------------------------------------------------

function arg(name) {
  const i = process.argv.indexOf(name);
  return i === -1 ? undefined : process.argv[i + 1];
}

// Run as a command, by whatever path: the loader resolves links in its own URL.
if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  const command = process.argv[2];
  if (command === "check") {
    const options = { version: arg("--version"), exe: arg("--exe"), msi: arg("--msi"), setup: arg("--setup"), setupExe: arg("--setup-exe") };
    if (!/^\d+\.\d+\.\d+$/.test(options.version ?? "") || Object.values(options).some((v) => !v)) {
      process.stderr.write("usage: release-windows.mjs check --version X.Y.Z --exe <cg-agent.exe> --msi <msi> --setup <setup.exe> --setup-exe <cg-setup.exe>\n");
      process.exit(2);
    }
    const results = check(options);
    for (const r of results) process.stdout.write(r.problems.length === 0 ? `PASS ${r.check}\n` : `FAIL ${r.check}: ${r.problems.join("; ")}\n`);
    const failed = results.filter((r) => r.problems.length > 0).length;
    process.stdout.write(failed === 0 ? `release check: all ${results.length} checks passed\n` : `release check: ${failed} of ${results.length} checks failed\n`);
    process.exit(failed === 0 ? 0 : 1);
  } else if (command === "archive") {
    const out = arg("--out");
    if (!out) {
      process.stderr.write("usage: release-windows.mjs archive --out <dir> [--ref <ref>]\n");
      process.exit(2);
    }
    try {
      const r = buildArchive({ ref: arg("--ref") ?? "HEAD", out });
      const digest = sha256(readFileSync(r.archive));
      process.stdout.write(
        `archive: ${r.archive}: wix-${r.release}/ is the tree ${r.tree} of ${r.commit} (${r.wixFiles} files), with ${r.setupFiles} setup sources; ${r.size} bytes, sha256 ${digest}\n`,
      );
    } catch (e) {
      process.stderr.write(`archive refused: ${e instanceof Error ? e.message : String(e)}\n`);
      process.exit(1);
    }
  } else {
    process.stderr.write("usage: release-windows.mjs check ... | archive --out <dir> [--ref <ref>]\n");
    process.exit(2);
  }
}
