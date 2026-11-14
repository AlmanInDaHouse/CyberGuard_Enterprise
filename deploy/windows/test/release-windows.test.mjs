// SPEC-024 Amendment 2026-11-14, points 2, 3 and 6, with its *Tests*: the script the release's
// agent job and package-ci's dry run use to check the Windows assets and to build and check the
// source archive (.github/scripts/release-windows.mjs). Each check is seen refusing a case made
// for it: a copy of a built file changed for the test (a foreign import, another message entry,
// the pre-release flag, another version string, an MSI with a second file, a Binary table or a
// custom action), crafted archives (a converted LICENSE.TXT, a file missing, a file added, a byte
// changed), and a local upstream whose tag or tree is not the pinned one. The readers are run on
// the binaries a build made, given by path:
//   CG_TEST_AGENT_EXE  cg-agent.exe, a release build
//   CG_TEST_SETUP_EXE  cg-setup.exe, a release build
//   CG_TEST_MSI        the MSI built around CG_TEST_AGENT_EXE
//   CG_TEST_SETUP      the setup built around CG_TEST_MSI and CG_TEST_SETUP_EXE
//   CG_TEST_VERSION    the version they were built at (1.2.3 when unset)
// A test whose files are not given skips, unless CG_REQUIRE_RELEASE_FILES=1, when it fails.
// Nothing here installs or runs the MSI or the setup; the MSI is opened through msi.dll.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { gunzipSync, gzipSync } from "node:zlib";
import {
  CHECKS,
  FETCH_CHECKS,
  SETUP_SOURCES,
  SYSTEM_DLLS,
  WIX_COPYRIGHT_LINE,
  WIX_REMOTE,
  agentVersionProblems,
  blobId,
  buildArchive,
  foreignImports,
  gitEnvironment,
  importProblems,
  messageIdProblems,
  msiCustomActionProblems,
  msiFileProblems,
  msiVersionProblems,
  parseManifest,
  payloadProblems,
  readImportEntries,
  readImports,
  readMessageIds,
  readMsi,
  readResources,
  readTarGz,
  readVersionInfo,
  registeredVersionProblems,
  setupCopyrightProblems,
  setupExeVersionProblems,
  setupVersionProblems,
  treeId,
  verifyArchive,
  versionOutputProblems,
} from "../../../.github/scripts/release-windows.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const SCRIPT = join(REPO, ".github", "scripts", "release-windows.mjs");
const VERSION = process.env.CG_TEST_VERSION ?? "1.2.3";
const REQUIRE = process.env.CG_REQUIRE_RELEASE_FILES === "1";

/** A test of the files a build made: it skips when they are not given, or fails if they must be. */
function releaseTest(name, vars, fn) {
  const missing = vars.filter((v) => !process.env[v] || !existsSync(process.env[v]));
  const skip = missing.length > 0 && !REQUIRE ? `${missing.join(", ")} not given` : false;
  test(name, { skip }, () => {
    assert.deepEqual(missing, [], `CG_REQUIRE_RELEASE_FILES=1 and ${missing.join(", ")} not given`);
    return fn(...vars.map((v) => process.env[v]));
  });
}

function withTemp(fn) {
  const dir = mkdtempSync(join(tmpdir(), "cg-release-test-"));
  try {
    return fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// --- The import allowlist and the problems, on values made for them -----------------------

test("the import allowlist is package_executable.rs's SYSTEM_DLLS", () => {
  const rust = readFileSync(join(REPO, "agent", "cg-agent", "tests", "package_executable.rs"), "utf8");
  const block = /const SYSTEM_DLLS: \[&str; (\d+)\] = \[([^\]]*)\];/.exec(rust);
  assert.ok(block, "package_executable.rs declares SYSTEM_DLLS");
  const names = [...block[2].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  assert.equal(names.length, Number(block[1]));
  assert.deepEqual([...SYSTEM_DLLS], names);
});

test("an import outside the system DLLs and the API sets other than the C runtime's is foreign", () => {
  assert.deepEqual(foreignImports(["KERNEL32.dll", "ntdll.dll", "api-ms-win-core-synch-l1-2-0.dll", "WS2_32.dll"]), []);
  assert.deepEqual(foreignImports(["kernel32.dll", "api-ms-win-crt-runtime-l1-1-0.dll", "VCRUNTIME140.dll", "foreign.dll"]), [
    "api-ms-win-crt-runtime-l1-1-0.dll",
    "VCRUNTIME140.dll",
    "foreign.dll",
  ]);
  assert.deepEqual(importProblems(["kernel32.dll", "advapi32.dll"]), []);
  assert.match(importProblems(["kernel32.dll", "foreign.dll"])[0] ?? "", /foreign\.dll/);
  assert.match(importProblems([])[0] ?? "", /no import/);
});

/**
 * A minimal PE32+ image whose only import is a delay-loaded one, made for the test: a DOS header
 * with e_lfanew, "PE\0\0", a COFF header with one section and a 240-byte optional header (magic
 * 0x20B, 16 data directories, the import directory, 1, empty), and in its one section (RVA 0x1000,
 * file offset 0x200) the delay-import descriptor directory 13 points at, its name at RVA 0x1100.
 * With grAttrs 1 the descriptor holds RVAs; with 0, virtual addresses, the image base added.
 */
function delayImportImage(name, { grAttrs = 1, imageBase = 0x140000000n } = {}) {
  const buf = Buffer.alloc(0x400);
  buf.write("MZ", 0, "latin1");
  buf.writeUInt32LE(0x40, 0x3c);
  const pe = 0x40;
  buf.write("PE\0\0", pe, "latin1");
  buf.writeUInt16LE(0x8664, pe + 4);
  buf.writeUInt16LE(1, pe + 6);
  buf.writeUInt16LE(240, pe + 20);
  const optional = pe + 24;
  buf.writeUInt16LE(0x20b, optional);
  buf.writeBigUInt64LE(imageBase, optional + 24);
  buf.writeUInt32LE(16, optional + 108);
  buf.writeUInt32LE(0x1000, optional + 112 + 13 * 8);
  buf.writeUInt32LE(64, optional + 112 + 13 * 8 + 4);
  const section = optional + 240;
  buf.write(".didat", section, "latin1");
  buf.writeUInt32LE(0x200, section + 8);
  buf.writeUInt32LE(0x1000, section + 12);
  buf.writeUInt32LE(0x200, section + 16);
  buf.writeUInt32LE(0x200, section + 20);
  // The descriptor at RVA 0x1000, the all-zero one that ends the table after it.
  buf.writeUInt32LE(grAttrs, 0x200);
  buf.writeUInt32LE(grAttrs & 1 ? 0x1100 : Number(imageBase) + 0x1100, 0x204);
  buf.write(`${name}\0`, 0x300, "latin1");
  return buf;
}

test("a delay-loaded import is read, and a foreign one is refused", () => {
  const image = delayImportImage("foreign.dll");
  assert.deepEqual(readImports(image), ["foreign.dll"]);
  assert.deepEqual(readImportEntries(image), [{ name: "foreign.dll", offset: 0x300 }]);
  assert.match(importProblems(readImports(image)).join(), /foreign\.dll/);
  // A descriptor of virtual addresses (grAttrs bit 0 clear): the image base is taken off its name's.
  const va = delayImportImage("foreign.dll", { grAttrs: 0, imageBase: 0x400000n });
  assert.deepEqual(readImports(va), ["foreign.dll"]);
  assert.match(importProblems(readImports(va)).join(), /foreign\.dll/);
  // A system DLL delay-loaded is not foreign.
  assert.deepEqual(importProblems(readImports(delayImportImage("wevtapi.dll"))), []);
});

const INFO = {
  fileVersion: [1, 2, 3, 0],
  productVersion: [1, 2, 3, 0],
  fileFlags: 0,
  fileFlagsMask: 0x3f,
  strings: { FileVersion: "1.2.3", ProductVersion: "1.2.3", LegalCopyright: "Copyright (c) CyberGuard" },
};

test("the agent's version resource is X.Y.Z.0 and X.Y.Z, without the pre-release flag", () => {
  assert.deepEqual(agentVersionProblems(INFO, "1.2.3"), []);
  const cases = [
    ["another numeric file version", { fileVersion: [1, 2, 4, 0] }, /file version 1\.2\.4\.0/],
    ["a fourth numeric field", { productVersion: [1, 2, 3, 1] }, /product version 1\.2\.3\.1/],
    ["another FileVersion string", { strings: { ...INFO.strings, FileVersion: "1.2.3-dev" } }, /FileVersion "1\.2\.3-dev"/],
    ["no ProductVersion string", { strings: { FileVersion: "1.2.3" } }, /ProductVersion/],
    ["the pre-release flag", { fileFlags: 0x2 }, /pre-release/],
  ];
  for (const [name, change, expected] of cases) {
    const problems = agentVersionProblems({ ...INFO, ...change }, "1.2.3");
    assert.ok(problems.some((p) => expected.test(p)), `${name}: ${JSON.stringify(problems)}`);
  }
});

test("cg-setup.exe's ProductVersion string is X.Y.Z, without the pre-release flag", () => {
  assert.deepEqual(setupExeVersionProblems(INFO, "1.2.3"), []);
  assert.match(setupExeVersionProblems(INFO, "1.2.4")[0] ?? "", /ProductVersion "1\.2\.3"/);
  assert.match(setupExeVersionProblems({ ...INFO, fileFlags: 0x3 }, "1.2.3")[0] ?? "", /pre-release/);
});

test("the setup's ProductVersion string is X.Y.Z and its copyright holds WiX's line", () => {
  assert.deepEqual(setupVersionProblems(INFO, "1.2.3"), []);
  assert.match(setupVersionProblems(INFO, "1.2.4")[0] ?? "", /ProductVersion "1\.2\.3"/);
  const point10 = `Copyright (c) CyberGuard. Burn engine: ${WIX_COPYRIGHT_LINE}`;
  assert.deepEqual(setupCopyrightProblems({ strings: { LegalCopyright: point10 } }), []);
  assert.match(setupCopyrightProblems(INFO)[0] ?? "", /LegalCopyright "Copyright \(c\) CyberGuard"/);
  assert.match(setupCopyrightProblems({ strings: {} })[0] ?? "", /LegalCopyright/);
});

test("the message table holds entries 1000 to 1003, and neither 999 nor 1004", () => {
  assert.deepEqual(messageIdProblems([1000, 1001, 1002, 1003]), []);
  // Point 2 names those two neighbours; an entry it does not name is not its refusal.
  assert.deepEqual(messageIdProblems([1000, 1001, 1002, 1003, 2000]), []);
  assert.match(messageIdProblems([999, 1000, 1001, 1002, 1003]).join(), /999/);
  assert.match(messageIdProblems([1000, 1001, 1002, 1003, 1004]).join(), /1004/);
  assert.match(messageIdProblems([1000, 1001, 1003]).join(), /1002/);
  assert.match(messageIdProblems([]).join(), /1000/);
});

test("the agent's --version prints cg-agent X.Y.Z", () => {
  assert.deepEqual(versionOutputProblems("cg-agent 1.2.3\r\n", "1.2.3"), []);
  assert.match(versionOutputProblems("cg-agent 1.2.3-dev\n", "1.2.3")[0] ?? "", /cg-agent 1\.2\.3-dev/);
  assert.match(versionOutputProblems("", "1.2.3")[0] ?? "", /cg-agent 1\.2\.3/);
});

const MSI = {
  tables: ["_Validation", "Component", "File", "Property"],
  productVersion: ["1.2.3"],
  files: [{ key: "cg_agent_exe", name: "cg-agent.exe" }],
  customActions: [],
  binaries: [],
};

test("the MSI is version X.Y.Z, holds the agent's file alone, and no custom action", () => {
  assert.deepEqual(msiVersionProblems(MSI, "1.2.3"), []);
  assert.deepEqual(msiFileProblems(MSI), []);
  assert.deepEqual(msiCustomActionProblems(MSI), []);
  assert.deepEqual(msiFileProblems({ ...MSI, files: [{ key: "a", name: "CG-AGE~1.EXE|cg-agent.exe" }] }), []);
  assert.deepEqual(msiCustomActionProblems({ ...MSI, tables: [...MSI.tables, "CustomAction"] }), []);
  assert.match(msiVersionProblems(MSI, "1.2.4")[0] ?? "", /ProductVersion 1\.2\.3/);
  assert.match(msiVersionProblems({ ...MSI, productVersion: [] }, "1.2.3")[0] ?? "", /ProductVersion/);
  assert.match(msiFileProblems({ ...MSI, files: [...MSI.files, { key: "b", name: "other.dll" }] })[0] ?? "", /2 files/);
  assert.match(msiFileProblems({ ...MSI, files: [{ key: "a", name: "cg-setup.exe" }] })[0] ?? "", /cg-setup\.exe/);
  assert.match(msiFileProblems({ ...MSI, files: [] })[0] ?? "", /0 files/);
  assert.match(msiCustomActionProblems({ ...MSI, tables: [...MSI.tables, "Binary"] }).join(), /Binary table/);
  assert.match(
    msiCustomActionProblems({ ...MSI, tables: [...MSI.tables, "CustomAction"], customActions: ["CgInjectedFailure"] }).join(),
    /CgInjectedFailure/,
  );
});

const MANIFEST = `﻿<?xml version="1.0" encoding="utf-8"?><BurnManifest xmlns="http://wixtoolset.org/schemas/v4/2008/Burn"><UX PrimaryPayloadId="Ba"><Payload Id="u" FilePath="thm.xml" SourcePath="u0" /><Payload Id="Ba" FilePath="wixstdba.exe" SourcePath="u3" /></UX><Container Id="WixAttachedContainer" FilePath="s.exe" Attached="yes" /><Payload Id="Preflight" FilePath="preflight\\cg-setup.exe" FileSize="3" Packaging="embedded" SourcePath="a0" Container="WixAttachedContainer" /><Payload Id="Agent" FilePath="cyberguard-agent-1.2.3-x64.msi" Packaging="embedded" SourcePath="a1" Container="WixAttachedContainer" /><Payload Id="Configure" FilePath="configure\\cg-setup.exe" Packaging="embedded" SourcePath="a2" Container="WixAttachedContainer" /><Registration Code="{X}" ExecutableName="s.exe" Version="1.2.3"><Arp DisplayName="CyberGuard &amp; Agent" DisplayVersion="1.2.3" /></Registration></BurnManifest>`;

test("the manifest's registered version and its payloads are read", () => {
  const manifest = parseManifest(MANIFEST);
  assert.deepEqual(
    manifest.registrations.map((r) => r.version),
    ["1.2.3"],
  );
  assert.deepEqual(
    manifest.payloads.map((p) => [p.id, p.filePath, p.sourcePath, p.container ?? null]),
    [
      ["u", "thm.xml", "u0", null],
      ["Ba", "wixstdba.exe", "u3", null],
      ["Preflight", "preflight\\cg-setup.exe", "a0", "WixAttachedContainer"],
      ["Agent", "cyberguard-agent-1.2.3-x64.msi", "a1", "WixAttachedContainer"],
      ["Configure", "configure\\cg-setup.exe", "a2", "WixAttachedContainer"],
    ],
  );
  assert.deepEqual(registeredVersionProblems(manifest, "1.2.3"), []);
  assert.match(registeredVersionProblems(manifest, "1.2.4")[0] ?? "", /registers 1\.2\.3/);
  assert.match(registeredVersionProblems(parseManifest(MANIFEST.replace(/<Registration[^]*<\/Registration>/, "")), "1.2.3")[0] ?? "", /Registration/);
  const twice = MANIFEST.replace("</BurnManifest>", '<Registration Version="1.2.3" /></BurnManifest>');
  assert.match(registeredVersionProblems(parseManifest(twice), "1.2.3")[0] ?? "", /2 Registration/);
});

test("the payloads the setup carries are compared with the MSI and cg-setup.exe by their bytes", () => {
  const manifest = parseManifest(MANIFEST);
  const digests = { Preflight: "s", Agent: "m", Configure: "s" };
  const digest = (p) => digests[p.id];
  assert.deepEqual(payloadProblems(manifest, digest, { msi: "m", setupExe: "s" }), { msi: [], setupExe: [] });
  const msiOther = payloadProblems(manifest, digest, { msi: "M", setupExe: "s" });
  assert.match(msiOther.msi[0] ?? "", /cyberguard-agent-1\.2\.3-x64\.msi.*not the MSI/);
  assert.deepEqual(msiOther.setupExe, []);
  digests.Configure = "S";
  const setupOther = payloadProblems(manifest, digest, { msi: "m", setupExe: "s" });
  assert.deepEqual(setupOther.msi, []);
  assert.match(setupOther.setupExe.join(), /configure\\cg-setup\.exe.*not cg-setup\.exe/);
  digests.Agent = undefined;
  assert.match(payloadProblems(manifest, digest, { msi: "m", setupExe: "s" }).msi.join(), /not extracted/);
  const noSetup = parseManifest(MANIFEST.replace(/<Payload Id="(Preflight|Configure)"[^>]*\/>/g, ""));
  assert.match(payloadProblems(noSetup, digest, { msi: "m", setupExe: "s" }).setupExe.join(), /no cg-setup\.exe/);
  const twoMsis = parseManifest(MANIFEST.replace("</BurnManifest>", '<Payload Id="X" FilePath="x.msi" SourcePath="a3" Container="WixAttachedContainer" /></BurnManifest>'));
  assert.match(payloadProblems(twoMsis, () => "m", { msi: "m", setupExe: "s" }).msi.join(), /2 MSI payloads/);
});

// --- The readers on the binaries a build made, and copies changed for the test -------------

/** The bytes of a resource, by type, with their file offset. */
function resource(buf, type) {
  const found = readResources(buf).filter((r) => r.type === type);
  assert.equal(found.length, 1, `one resource of type ${type}`);
  return found[0];
}

/** Writes `text` as UTF-16 over the value that follows the version resource's `key` string. */
function patchVersionString(buf, key, from, to) {
  assert.equal(from.length, to.length);
  const r = resource(buf, 16);
  const region = buf.subarray(r.offset, r.offset + r.size);
  const at = region.indexOf(Buffer.from(`${key}\0`, "utf16le"));
  assert.ok(at >= 0, `the version resource holds ${key}`);
  const value = region.indexOf(Buffer.from(from, "utf16le"), at);
  assert.ok(value >= 0, `${key}'s value holds ${from}`);
  Buffer.from(to, "utf16le").copy(region, value);
}

releaseTest("the agent's imports, version resource and message table are read from its executable", ["CG_TEST_AGENT_EXE"], (exe) => {
  const buf = readFileSync(exe);
  const names = readImports(buf).map((n) => n.toLowerCase());
  assert.ok(names.includes("kernel32.dll"), `the import table was read: ${names}`);
  assert.deepEqual(foreignImports(names), []);
  assert.deepEqual(importProblems(names), []);
  assert.deepEqual(readMessageIds(buf), [1000, 1001, 1002, 1003]);
  const info = readVersionInfo(buf);
  assert.deepEqual(info.fileVersion, [...VERSION.split(".").map(Number), 0]);
  assert.deepEqual(info.productVersion, [...VERSION.split(".").map(Number), 0]);
  assert.equal(info.strings.FileVersion, VERSION);
  assert.equal(info.strings.ProductVersion, VERSION);
  assert.equal(info.fileFlags & 0x2, 0);
  assert.deepEqual(agentVersionProblems(info, VERSION), []);
});

releaseTest("a foreign import in a copy of the agent's executable fails the imports check", ["CG_TEST_AGENT_EXE"], (exe) => {
  const buf = Buffer.from(readFileSync(exe));
  const entry = readImportEntries(buf).find((e) => e.name.length >= "foreign.dll".length);
  assert.ok(entry, "an import name long enough to overwrite");
  buf.fill(0, entry.offset, entry.offset + entry.name.length);
  buf.write("foreign.dll", entry.offset, "latin1");
  assert.ok(readImports(buf).includes("foreign.dll"));
  assert.match(importProblems(readImports(buf)).join(), /foreign\.dll/);
});

releaseTest("another message entry in a copy of the agent's executable fails the message check", ["CG_TEST_AGENT_EXE"], (exe) => {
  const original = readFileSync(exe);
  const table = resource(original, 11);
  const blocks = original.readUInt32LE(table.offset);
  for (const [field, from, to] of [
    [0, 1000, 999],
    [4, 1003, 1004],
  ]) {
    const buf = Buffer.from(original);
    let patched = false;
    for (let i = 0; i < blocks; i += 1) {
      const at = table.offset + 4 + i * 12 + field;
      if (buf.readUInt32LE(at) === from) {
        buf.writeUInt32LE(to, at);
        patched = true;
      }
    }
    assert.ok(patched, `a block ${field === 0 ? "starts" : "ends"} at ${from}`);
    assert.ok(readMessageIds(buf).includes(to));
    assert.match(messageIdProblems(readMessageIds(buf)).join(), new RegExp(String(to)));
  }
});

releaseTest("the pre-release flag or another version string in a copy of the agent fails its version check", ["CG_TEST_AGENT_EXE"], (exe) => {
  const original = readFileSync(exe);
  const flagged = Buffer.from(original);
  const { fixedOffset, fileFlags } = readVersionInfo(flagged);
  flagged.writeUInt32LE(fileFlags | 0x2, fixedOffset + 28);
  assert.equal(readVersionInfo(flagged).fileFlags & 0x2, 0x2);
  assert.match(agentVersionProblems(readVersionInfo(flagged), VERSION).join(), /pre-release/);

  const other = VERSION.replace(/\d$/, (d) => String((Number(d) + 1) % 10));
  for (const key of ["FileVersion", "ProductVersion"]) {
    const buf = Buffer.from(original);
    patchVersionString(buf, key, VERSION, other);
    assert.equal(readVersionInfo(buf).strings[key], other);
    assert.match(agentVersionProblems(readVersionInfo(buf), VERSION).join(), new RegExp(`${key} "${other.replaceAll(".", "\\.")}"`));
  }
});

releaseTest("cg-setup.exe's version resource is read, and a copy with another version or the flag fails", ["CG_TEST_SETUP_EXE"], (exe) => {
  const original = readFileSync(exe);
  const info = readVersionInfo(original);
  assert.equal(info.strings.ProductVersion, VERSION);
  assert.deepEqual(setupExeVersionProblems(info, VERSION), []);
  const flagged = Buffer.from(original);
  flagged.writeUInt32LE(info.fileFlags | 0x2, info.fixedOffset + 28);
  assert.match(setupExeVersionProblems(readVersionInfo(flagged), VERSION).join(), /pre-release/);
  const other = Buffer.from(original);
  patchVersionString(other, "ProductVersion", VERSION, VERSION.replace(/\d$/, (d) => String((Number(d) + 1) % 10)));
  assert.match(setupExeVersionProblems(readVersionInfo(other), VERSION).join(), /ProductVersion/);
});

releaseTest("the setup's version resource is read: its ProductVersion and its copyright, and a copy without WiX's line fails", ["CG_TEST_SETUP"], (setup) => {
  const original = readFileSync(setup);
  const info = readVersionInfo(original);
  assert.equal(info.strings.ProductVersion, VERSION);
  assert.deepEqual(setupVersionProblems(info, VERSION), []);
  assert.deepEqual(setupCopyrightProblems(info), [], info.strings.LegalCopyright);
  // The same resource with one letter of WiX's line changed, as a setup that dropped it.
  const changed = Buffer.from(original);
  const at = changed.indexOf(Buffer.from(".NET Foundation", "utf16le"));
  assert.ok(at > 0, "WiX's line is in the setup's file");
  changed.write("M", at + 2, "utf16le");
  assert.match(setupCopyrightProblems(readVersionInfo(changed)).join(), /lacks WiX's line/);
});

/** Runs SQL on a copy of an MSI through msi.dll, the database opened in transaction mode. */
function changeMsi(path, statements) {
  const script = [
    "$ErrorActionPreference = 'Stop'",
    "$i = New-Object -ComObject WindowsInstaller.Installer",
    "$db = $i.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $i, @($env:CG_MSI, 1))",
    "foreach ($sql in ($env:CG_SQL -split \"`n\")) {",
    "  $v = $db.GetType().InvokeMember('OpenView', 'InvokeMethod', $null, $db, @($sql))",
    "  [void]$v.GetType().InvokeMember('Execute', 'InvokeMethod', $null, $v, $null)",
    "  [void]$v.GetType().InvokeMember('Close', 'InvokeMethod', $null, $v, $null)",
    "}",
    "[void]$db.GetType().InvokeMember('Commit', 'InvokeMethod', $null, $db, $null)",
  ].join("\n");
  const shell = spawnSync("pwsh", ["-NoProfile", "-Command", "exit 0"]).status === 0 ? "pwsh" : "powershell";
  const r = spawnSync(shell, ["-NoProfile", "-NonInteractive", "-Command", script], {
    encoding: "utf8",
    env: { ...process.env, CG_MSI: path, CG_SQL: statements.join("\n") },
  });
  assert.equal(r.status, 0, r.stdout + r.stderr);
}

releaseTest("the MSI's database is read, and a copy with a second file, a Binary table or a custom action fails", ["CG_TEST_MSI"], (msi) => {
  const facts = readMsi(msi);
  assert.deepEqual(facts.productVersion, [VERSION]);
  assert.deepEqual(msiVersionProblems(facts, VERSION), []);
  assert.deepEqual(msiFileProblems(facts), []);
  assert.deepEqual(msiCustomActionProblems(facts), []);
  assert.ok(!facts.tables.includes("Binary") && facts.customActions.length === 0);
  withTemp((dir) => {
    const cases = [
      [
        "a second file",
        ["INSERT INTO `File` (`File`, `Component_`, `FileName`, `FileSize`, `Attributes`, `Sequence`) VALUES ('extra', 'AgentExe', 'extra.dll', 1, 0, 2)"],
        msiFileProblems,
        /2 files/,
      ],
      ["a Binary table", ["CREATE TABLE `Binary` (`Name` CHAR(72) NOT NULL, `Data` OBJECT NOT NULL PRIMARY KEY `Name`)"], msiCustomActionProblems, /Binary table/],
      [
        "a custom action",
        [
          "CREATE TABLE `CustomAction` (`Action` CHAR(72) NOT NULL, `Type` SHORT NOT NULL, `Source` CHAR(72), `Target` CHAR(255) PRIMARY KEY `Action`)",
          "INSERT INTO `CustomAction` (`Action`, `Type`, `Source`, `Target`) VALUES ('Injected', 34, 'AgentFolder', 'cmd.exe')",
        ],
        msiCustomActionProblems,
        /Injected/,
      ],
    ];
    for (const [name, statements, problems, expected] of cases) {
      const copy = join(dir, `${name.replaceAll(" ", "-")}.msi`);
      copyFileSync(msi, copy);
      changeMsi(copy, statements);
      assert.match(problems(readMsi(copy)).join(), expected, name);
    }
  });
});

releaseTest(
  "check reads every asset, reports each check once, and goes on past a failure",
  ["CG_TEST_AGENT_EXE", "CG_TEST_SETUP_EXE", "CG_TEST_MSI", "CG_TEST_SETUP"],
  (exe, setupExe, msi, setup) =>
    withTemp((dir) => {
      // A changed message entry leaves the copy runnable, so its --version is still read.
      const changed = Buffer.from(readFileSync(exe));
      const table = resource(changed, 11);
      const blocks = changed.readUInt32LE(table.offset);
      for (let i = 0; i < blocks; i += 1) {
        if (changed.readUInt32LE(table.offset + 4 + i * 12) === 1000) changed.writeUInt32LE(999, table.offset + 4 + i * 12);
      }
      const copy = join(dir, "cg-agent.exe");
      writeFileSync(copy, changed);
      const args = ["--version", VERSION, "--msi", msi, "--setup", setup, "--setup-exe", setupExe];
      const r = spawnSync(process.execPath, [SCRIPT, "check", "--exe", copy, ...args], { encoding: "utf8" });
      const lines = r.stdout.split(/\r?\n/).filter((l) => /^(PASS|FAIL) /.test(l));
      assert.deepEqual(
        lines.map((l) => /^(?:PASS|FAIL) ([^:]+)/.exec(l)[1]),
        [...CHECKS],
        r.stdout + r.stderr,
      );
      assert.equal(r.status, 1, r.stdout + r.stderr);
      assert.ok(lines.some((l) => l.startsWith("FAIL agent message table:") && l.includes("999")), r.stdout);
      // Its payload is no longer the MSI's file, and every check that reads another asset passes.
      assert.ok(lines.some((l) => l.startsWith("FAIL msi agent payload:")), r.stdout);
      for (const name of ["agent --version", "agent version resource", "agent imports", "cg-setup.exe version resource", "msi product version", "msi file table", "msi custom actions", "setup version resource", "setup copyright", "setup registered version", "setup msi payload", "setup cg-setup.exe payloads"]) {
        assert.ok(lines.includes(`PASS ${name}`), `${name}: ${r.stdout}`);
      }
    }),
);

test("check names its usage and exits 2 when an asset is not given", () => {
  const r = spawnSync(process.execPath, [SCRIPT, "check", "--version", "1.2.3"], { encoding: "utf8" });
  assert.equal(r.status, 2, r.stdout + r.stderr);
  assert.match(r.stderr, /usage/);
});

// --- The source archive ------------------------------------------------------------------

/** A tar.gz as git archive writes one: ustar headers, a pax header for a long path. */
function tarGz(entries) {
  const blocks = [];
  const pad = (b) => Buffer.concat([b, Buffer.alloc((512 - (b.length % 512)) % 512)]);
  const header = (name, size, type, mode, linkname = "") => {
    const h = Buffer.alloc(512);
    h.write(name, 0, 100, "utf8");
    h.write(`${mode.toString(8).padStart(7, "0")}\0`, 100, "latin1");
    h.write("0000000\0", 108, "latin1");
    h.write("0000000\0", 116, "latin1");
    h.write(`${size.toString(8).padStart(11, "0")}\0`, 124, "latin1");
    h.write("00000000000\0", 136, "latin1");
    h.write("        ", 148, "latin1");
    h.write(type, 156, "latin1");
    h.write(linkname, 157, 100, "utf8");
    h.write("ustar\0" + "00", 257, "latin1");
    let sum = 0;
    for (const b of h) sum += b;
    h.write(`${sum.toString(8).padStart(6, "0")}\0 `, 148, "latin1");
    return h;
  };
  const record = (key, value) => {
    const body = ` ${key}=${value}\n`;
    let length = Buffer.byteLength(body);
    length += String(length + String(length).length).length;
    return Buffer.from(`${length}${body}`, "utf8");
  };
  for (const e of entries) {
    let name = e.path;
    if (Buffer.byteLength(name) > 100) {
      const pax = record("path", e.path);
      blocks.push(header("pax_header", pax.length, "x", 0o644), pad(pax));
      name = e.path.slice(0, 100);
    }
    const bytes = e.bytes ?? Buffer.alloc(0);
    const type = e.type === "directory" ? "5" : e.type === "symlink" ? "2" : "0";
    blocks.push(header(name, type === "0" ? bytes.length : 0, type, e.mode ?? (type === "5" ? 0o775 : 0o664), e.linkname ?? ""));
    if (type === "0") blocks.push(pad(bytes));
  }
  blocks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(blocks));
}

const LICENCE = Buffer.from("Copyright (c) .NET Foundation and contributors.\n\nMicrosoft Reciprocal License (MS-RL)\n\nThis license governs use.\n");
const LONG = `src/test/data/${"deep/".repeat(20)}licence.txt`;
const WIX_FILES = [
  { path: "LICENSE.TXT", bytes: LICENCE },
  { path: "OSMFEULA.txt", bytes: Buffer.from("OSMF EULA\n") },
  { path: "src/a.txt", bytes: Buffer.from("a\nb\n") },
  { path: "build.sh", bytes: Buffer.from("#!/bin/sh\n"), mode: "100755" },
  { path: LONG, bytes: Buffer.from("a test's licence\n") },
];
const SETUP_FILES = [
  { path: "bundle/Bundle.wxs", bytes: Buffer.from("<Wix />\n") },
  { path: "bundle/theme/LICENSE.TXT", bytes: LICENCE },
];

function expected(files = WIX_FILES) {
  const tree = files.map((f) => ({ mode: f.mode ?? "100644", type: "blob", id: blobId(f.bytes), path: f.path }));
  return { release: "7.0.1", wixTree: treeId(tree), tree, setupFiles: SETUP_FILES, licence: LICENCE };
}

function crafted({ wix = WIX_FILES, setup = SETUP_FILES } = {}) {
  const entries = [{ path: "wix-7.0.1/", type: "directory" }];
  for (const f of wix) entries.push({ path: `wix-7.0.1/${f.path}`, bytes: f.bytes, mode: f.mode === "100755" ? 0o775 : 0o664 });
  for (const f of setup) entries.push({ path: `cyberguard-setup/deploy/windows/${f.path}`, bytes: f.bytes });
  return tarGz(entries);
}

test("the archive's reader reads ustar and pax entries, and its verifier passes the archive made for it", () => {
  const entries = readTarGz(crafted());
  const files = entries.filter((e) => e.type === "file").map((e) => e.path);
  assert.ok(files.includes(`wix-7.0.1/${LONG}`), `the pax path was read: ${files}`);
  assert.equal(entries.find((e) => e.path === "wix-7.0.1/build.sh").mode & 0o111, 0o111 & 0o775);
  assert.deepEqual(verifyArchive(entries, expected()), []);
});

test("an archive whose LICENSE.TXT had its line endings converted is refused", () => {
  const crlf = Buffer.from(LICENCE.toString("latin1").replaceAll("\n", "\r\n"), "latin1");
  const wix = WIX_FILES.map((f) => (f.path === "LICENSE.TXT" ? { ...f, bytes: crlf } : f));
  const problems = verifyArchive(readTarGz(crafted({ wix })), expected());
  assert.ok(problems.some((p) => /wix-7\.0\.1\/LICENSE\.TXT is not the blob/.test(p)), JSON.stringify(problems));
  assert.ok(problems.some((p) => /LICENSE\.TXT is not byte-identical/.test(p)), JSON.stringify(problems));
  assert.ok(problems.some((p) => /recomputes to tree/.test(p)), JSON.stringify(problems));
});

test("an archive that lacks a file of the tree, or holds one more, or one changed byte, is refused", () => {
  const cases = [
    ["a file missing", { wix: WIX_FILES.filter((f) => f.path !== "OSMFEULA.txt") }, /lacks wix-7\.0\.1\/OSMFEULA\.txt/],
    ["a test data file missing", { wix: WIX_FILES.filter((f) => f.path !== LONG) }, /lacks wix-7\.0\.1\/src\/test\/data/],
    ["a file added", { wix: [...WIX_FILES, { path: "extra.txt", bytes: Buffer.from("x") }] }, /wix-7\.0\.1\/extra\.txt is not in the pinned tree/],
    [
      "a byte changed",
      { wix: WIX_FILES.map((f) => (f.path === "src/a.txt" ? { ...f, bytes: Buffer.from("a\nc\n") } : f)) },
      /wix-7\.0\.1\/src\/a\.txt is not the blob/,
    ],
    [
      "an executable bit lost",
      { wix: WIX_FILES.map((f) => (f.path === "build.sh" ? { ...f, mode: "100644" } : f)) },
      /wix-7\.0\.1\/build\.sh.*mode/,
    ],
    ["a setup file missing", { setup: SETUP_FILES.slice(1) }, /lacks cyberguard-setup\/deploy\/windows\/bundle\/Bundle\.wxs/],
    [
      "the theme's licence changed",
      { setup: [SETUP_FILES[0], { ...SETUP_FILES[1], bytes: Buffer.from("other") }] },
      /cyberguard-setup\/deploy\/windows\/bundle\/theme\/LICENSE\.TXT/,
    ],
  ];
  for (const [name, change, pattern] of cases) {
    const problems = verifyArchive(readTarGz(crafted(change)), expected());
    assert.ok(problems.some((p) => pattern.test(p)), `${name}: ${JSON.stringify(problems)}`);
  }
  const outside = tarGz([{ path: "elsewhere.txt", bytes: Buffer.from("x") }]);
  assert.ok(verifyArchive(readTarGz(outside), expected()).some((p) => /elsewhere\.txt is outside/.test(p)));
});

test("a damaged archive is refused by its reader", () => {
  const tar = gunzipSync(crafted());
  const damaged = Buffer.from(tar);
  damaged[600] ^= 1;
  assert.throws(() => readTarGz(gzipSync(damaged)), /checksum/);
  assert.throws(() => readTarGz(gzipSync(tar.subarray(0, 1024))), /truncated/);
});

// --- git: the tree ID, and the archive built from a local upstream -----------------------

const GIT_IDENTITY = ["-c", "user.name=cg-test", "-c", "user.email=cg-test@example.invalid", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"];

function git(gitDir, args, { input, env } = {}) {
  const r = spawnSync("git", [...GIT_IDENTITY, "--git-dir", gitDir, ...args], { input, env: { ...process.env, ...env }, maxBuffer: 64 * 1024 * 1024 });
  assert.equal(r.status, 0, `git ${args.join(" ")}: ${r.stderr}`);
  return r.stdout.toString("utf8").trim();
}

/** A bare repository's commit of `files`, made from objects alone, with no checkout. */
function commitFiles(gitDir, files, parent) {
  const index = join(dirname(gitDir), `index-${Math.random().toString(16).slice(2)}`);
  for (const f of files) {
    const id = git(gitDir, ["hash-object", "-w", "--no-filters", "--stdin"], { input: f.bytes });
    git(gitDir, ["update-index", "--add", "--cacheinfo", `${f.mode ?? "100644"},${id},${f.path}`], { env: { GIT_INDEX_FILE: index } });
  }
  const tree = git(gitDir, ["write-tree"], { env: { GIT_INDEX_FILE: index } });
  rmSync(index, { force: true });
  const commit = git(gitDir, ["commit-tree", tree, ...(parent ? ["-p", parent] : []), "-m", "fixture"]);
  return { tree, commit };
}

function bare(dir, name) {
  const path = join(dir, name);
  spawnSync("git", ["init", "--bare", "-q", path]);
  return path;
}

test("treeId recomputes the ID git writes for the same files", () =>
  withTemp((dir) => {
    const repo = bare(dir, "t.git");
    const files = [
      { path: "a.b", bytes: Buffer.from("1") },
      { path: "a/b", bytes: Buffer.from("2") },
      { path: "a-c", bytes: Buffer.from("3"), mode: "100755" },
      { path: "z/y/x.txt", bytes: Buffer.from("4\r\n") },
      { path: "link", bytes: Buffer.from("a/b"), mode: "120000" },
    ];
    const { tree } = commitFiles(repo, files);
    const entries = files.map((f) => ({ mode: f.mode ?? "100644", type: "blob", id: blobId(f.bytes), path: f.path }));
    assert.equal(treeId(entries), tree);
    assert.equal(blobId(Buffer.from("1")), git(repo, ["hash-object", "--stdin"], { input: Buffer.from("1") }));
  }));

/** A local upstream holding WiX-like files under an annotated tag v7.0.1, and a repository pinning it. */
function fixture(dir, { pinTree, tagOther = false, themeLicence = LICENCE } = {}) {
  mkdirSync(dir, { recursive: true });
  const upstream = bare(dir, "upstream.git");
  const files = [
    ...WIX_FILES,
    // git archive would convert, or leave out, these but for the attributes the archive overrides.
    { path: ".gitattributes", bytes: Buffer.from("* text eol=crlf\nsrc/ignored.txt export-ignore\nsrc/subst.txt export-subst\n") },
    { path: "src/ignored.txt", bytes: Buffer.from("left out by export-ignore\n") },
    { path: "src/subst.txt", bytes: Buffer.from("$Format:%H$\n") },
    { path: "src/crlf.txt", bytes: Buffer.from("a\r\nb\r\n") },
    { path: "src/link", bytes: Buffer.from("a.txt"), mode: "120000" },
  ];
  const pinned = commitFiles(upstream, files);
  const tagged = tagOther ? commitFiles(upstream, [...files, { path: "later.txt", bytes: Buffer.from("later") }], pinned.commit) : pinned;
  git(upstream, ["tag", "-a", "-m", "v7.0.1", "v7.0.1", tagged.commit]);
  git(upstream, ["update-ref", "refs/heads/main", tagged.commit]);

  const repo = bare(dir, "repo.git");
  const pins = { wixRelease: "7.0.1", wixInformationalVersion: `7.0.1+${pinned.commit.slice(0, 7)}`, wixCommit: pinned.commit, wixTree: pinTree ?? pinned.tree };
  const setup = [
    { path: "deploy/windows/pins/pins.json", bytes: Buffer.from(JSON.stringify(pins)) },
    { path: "deploy/windows/bundle/Bundle.wxs", bytes: Buffer.from("<Wix />\n") },
    { path: "deploy/windows/bundle/CyberGuardSetup.wixproj", bytes: Buffer.from("<Project />\n") },
    { path: "deploy/windows/bundle/packages.lock.json", bytes: Buffer.from("{}\n") },
    { path: "deploy/windows/Directory.Build.props", bytes: Buffer.from("<Project />\n") },
    { path: "deploy/windows/Directory.Build.targets", bytes: Buffer.from("<Project />\n") },
    { path: "deploy/windows/global.json", bytes: Buffer.from("{}\n") },
    { path: "deploy/windows/nuget.config", bytes: Buffer.from("<configuration />\n") },
    { path: "deploy/windows/bundle/theme/CyberGuardTheme.xml", bytes: Buffer.from("<Theme />\n") },
    { path: "deploy/windows/bundle/theme/CyberGuardTheme.wxl", bytes: Buffer.from("<WixLocalization />\n") },
    { path: "deploy/windows/msi/Package.wxs", bytes: Buffer.from("not the setup's\n") },
    ...(themeLicence ? [{ path: "deploy/windows/bundle/theme/LICENSE.TXT", bytes: themeLicence }] : []),
  ];
  const { commit } = commitFiles(repo, setup);
  return { repo, ref: commit, remote: pathToFileURL(upstream).href, pinned };
}

test("the archive holds the pinned tree's files as committed and the setup's sources, and is verified", () =>
  withTemp((dir) => {
    const { repo, ref, remote, pinned } = fixture(dir);
    const out = join(dir, "out");
    const result = buildArchive({ repo, ref, out, remote });
    assert.equal(result.archive, join(out, "wix-7.0.1-source.tar.gz"));
    const entries = readTarGz(readFileSync(result.archive));
    const file = (p) => entries.find((e) => e.path === p && e.type !== "directory");
    assert.deepEqual(file("wix-7.0.1/src/crlf.txt").bytes, Buffer.from("a\r\nb\r\n"));
    assert.deepEqual(file("wix-7.0.1/src/a.txt").bytes, Buffer.from("a\nb\n"));
    assert.deepEqual(file("wix-7.0.1/src/subst.txt").bytes, Buffer.from("$Format:%H$\n"));
    assert.ok(file("wix-7.0.1/src/ignored.txt"), "an export-ignore file is archived");
    assert.equal(file("wix-7.0.1/src/link").type, "symlink");
    assert.ok(file(`wix-7.0.1/${LONG}`));
    assert.deepEqual(file("cyberguard-setup/deploy/windows/bundle/theme/LICENSE.TXT").bytes, LICENCE);
    assert.ok(file("cyberguard-setup/deploy/windows/nuget.config"));
    assert.equal(file("cyberguard-setup/deploy/windows/msi/Package.wxs"), undefined);
    assert.equal(file("cyberguard-setup/deploy/windows/pins/pins.json"), undefined);
    assert.equal(result.wixFiles, WIX_FILES.length + 5);
    assert.equal(result.tree, pinned.tree);
  }));

test("no archive is built for a pinned tree the commit does not have, or a tag naming another commit", () =>
  withTemp((dir) => {
    const wrongTree = fixture(join(dir, "tree"), { pinTree: "0123456789abcdef0123456789abcdef01234567" });
    const out = join(dir, "out");
    // The commit's own tree check, before any archive is made: not the archive's recomputed tree.
    assert.throws(() => buildArchive({ ...wrongTree, out }), /^Error: the commit [0-9a-f]{40}'s tree is [0-9a-f]{40}, not the pinned 0123456789abcdef0123456789abcdef01234567$/);
    assert.ok(!existsSync(join(out, "wix-7.0.1-source.tar.gz")));
    const otherTag = fixture(join(dir, "tag"), { tagOther: true });
    assert.throws(() => buildArchive({ ...otherTag, out }), /tag v7\.0\.1 .*names [0-9a-f]{40}, not the pinned/);
    assert.ok(!existsSync(join(out, "wix-7.0.1-source.tar.gz")));
  }));

test("no archive is left when the theme's LICENSE.TXT is not WiX's, or is absent", () =>
  withTemp((dir) => {
    const out = join(dir, "out");
    const changed = fixture(join(dir, "changed"), { themeLicence: Buffer.from(LICENCE.toString().replaceAll("\n", "\r\n")) });
    assert.throws(() => buildArchive({ ...changed, out }), /LICENSE\.TXT is not byte-identical/);
    assert.ok(!existsSync(join(out, "wix-7.0.1-source.tar.gz")));
    const absent = fixture(join(dir, "absent"), { themeLicence: null });
    assert.throws(() => buildArchive({ ...absent, out }), /bundle\/theme\/LICENSE\.TXT/);
    assert.ok(!existsSync(join(out, "wix-7.0.1-source.tar.gz")));
  }));

test("git runs with no inherited GIT_* variable and no system or global configuration", () => {
  const parent = {
    PATH: "p",
    GIT_CONFIG_PARAMETERS: "'url.https://example.invalid/.insteadof'='https://github.com/'",
    GIT_CONFIG_COUNT: "1",
    GIT_CONFIG_KEY_0: "transfer.fsckObjects",
    GIT_CONFIG_VALUE_0: "false",
    GIT_ALTERNATE_OBJECT_DIRECTORIES: "/elsewhere/objects",
    GIT_OBJECT_DIRECTORY: "/elsewhere/objects",
    Git_Dir: "/other.git",
    GIT_INDEX_FILE: "/inherited/index",
    GIT_CONFIG_GLOBAL: "/home/.gitconfig",
    GIT_CONFIG_NOSYSTEM: "0",
  };
  const quiet = { PATH: "p", GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null" };
  assert.deepEqual(gitEnvironment(parent), quiet);
  const own = { GIT_INDEX_FILE: "/scratch/index", GIT_WORK_TREE: "/scratch/empty" };
  assert.deepEqual(gitEnvironment(parent, own), { ...quiet, ...own });
  assert.throws(() => gitEnvironment(parent, { GIT_DIR: "/x" }), /GIT_DIR/);
});

test("the archive is built as pinned with a GIT_CONFIG_PARAMETERS, an alternate object directory and a global configuration in the environment", () =>
  withTemp((dir) => {
    const upstream = fixture(join(dir, "f"));
    const global = join(dir, "global.gitconfig");
    // Each one, if it reached git, would end it: a malformed parameter, a malformed file.
    writeFileSync(global, "[core\nnot a configuration line\n");
    const set = { GIT_CONFIG_PARAMETERS: "not a parameter", GIT_ALTERNATE_OBJECT_DIRECTORIES: join(dir, "missing", "objects"), GIT_CONFIG_GLOBAL: global };
    const saved = Object.fromEntries(Object.keys(set).map((k) => [k, process.env[k]]));
    Object.assign(process.env, set);
    try {
      const result = buildArchive({ ...upstream, out: join(dir, "out") });
      assert.equal(result.tree, upstream.pinned.tree);
      assert.ok(existsSync(result.archive));
    } finally {
      for (const [k, v] of Object.entries(saved)) {
        if (v === undefined) delete process.env[k];
        else process.env[k] = v;
      }
    }
  }));

test("archive names its usage and exits 2 without --out", () => {
  const r = spawnSync(process.execPath, [SCRIPT, "archive"], { encoding: "utf8" });
  assert.equal(r.status, 2, r.stdout + r.stderr);
  assert.match(r.stderr, /usage/);
});

test("the setup's sources the archive takes are point 3's list, and the fetch checks its objects", () => {
  assert.deepEqual([...SETUP_SOURCES], ["bundle/Bundle.wxs", "bundle/CyberGuardSetup.wixproj", "bundle/packages.lock.json", "Directory.Build.props", "Directory.Build.targets", "global.json", "nuget.config", "bundle/theme/"]);
  assert.equal(WIX_REMOTE, "https://github.com/wixtoolset/wix");
  assert.deepEqual([...FETCH_CHECKS], ["-c", "transfer.fsckObjects=true", "-c", "fetch.fsckObjects=true"]);
  assert.equal(WIX_COPYRIGHT_LINE, "Copyright (c) .NET Foundation and contributors. All rights reserved.");
});
