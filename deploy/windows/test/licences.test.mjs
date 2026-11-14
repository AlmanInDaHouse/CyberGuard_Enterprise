// SPEC-024 Amendment 2026-11-01, point 1: the licence check reads the nuget pins of what the
// setup or the MSI carries and admits them by name, under MS-RL. The Amendment 2026-11-14,
// points 3, 4 and 9: it reads cg-setup's runtime crates as it reads the agent's, its report
// carries the WiX files the setup carries with WiX's LICENSE.TXT, which the theme's directory
// holds byte for byte, and the allowlist's reason reads the engine as the setup carries it.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { cargoPackages, nugetPackages, refused, wixSection } from "../../../.github/scripts/check-licences.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const pins = () => JSON.parse(readFileSync(join(REPO, "deploy", "windows", "pins", "pins.json"), "utf8"));
const allowlist = () => JSON.parse(readFileSync(join(REPO, ".github", "scripts", "licences-allowed.json"), "utf8"));
const THEME = join(REPO, "deploy", "windows", "bundle", "theme");
/** WiX's LICENSE.TXT at the pinned commit 642f1a0f7c857ffa4fc9fa3f9d1433aab895dd29: its blob's ID. */
const WIX_LICENCE_BLOB = "d4d316ef3cbe3b5197dae64e758655c7e62f28ea";
/** The ID git gives `bytes` as a blob, with no filter: SHA-1 over "blob <size>\0" and the bytes. */
const blobId = (bytes) => createHash("sha1").update(`blob ${bytes.length}\0`).update(bytes).digest("hex");

/** cargo metadata's shape: the workspace's two roots, a crate both use and one only cg-setup uses. */
function workspaceMetadata() {
  const pkg = (name, license = "MIT") => ({ id: `${name} 1.0.0`, name, version: "1.0.0", license });
  const normal = [{ kind: null, target: null }];
  return {
    packages: [pkg("cg-agent", "Apache-2.0"), pkg("cg-setup", "Apache-2.0"), pkg("serde"), pkg("serde_derive"), pkg("pem"), pkg("base64"), pkg("tempfile")],
    workspace_members: ["cg-agent 1.0.0", "cg-setup 1.0.0"],
    resolve: {
      nodes: [
        { id: "cg-agent 1.0.0", deps: [{ pkg: "serde 1.0.0", dep_kinds: normal }] },
        {
          id: "cg-setup 1.0.0",
          deps: [
            { pkg: "cg-agent 1.0.0", dep_kinds: normal },
            { pkg: "serde 1.0.0", dep_kinds: normal },
            { pkg: "pem 1.0.0", dep_kinds: normal },
            { pkg: "tempfile 1.0.0", dep_kinds: [{ kind: "dev", target: null }] },
          ],
        },
        { id: "serde 1.0.0", deps: [{ pkg: "serde_derive 1.0.0", dep_kinds: normal }] },
        { id: "serde_derive 1.0.0", deps: [] },
        { id: "pem 1.0.0", deps: [{ pkg: "base64 1.0.0", dep_kinds: normal }] },
        { id: "base64 1.0.0", deps: [] },
        { id: "tempfile 1.0.0", deps: [] },
      ],
    },
  };
}

// SPEC-024 Amendment 2026-11-14, point 3: the theme's directory holds WiX's LICENSE.TXT, the one
// its two MS-RL files' headers name, as the pinned commit holds it (LF, no conversion).
test("the theme's LICENSE.TXT is WiX's at the pinned commit, byte for byte", () => {
  const bytes = readFileSync(join(THEME, "LICENSE.TXT"));
  assert.equal(bytes.length, 3341);
  assert.equal(blobId(bytes), WIX_LICENCE_BLOB);
});

test("each theme file keeps WiX's notice and places its LICENSE.TXT beside it", () => {
  const notice =
    "<!-- Copyright (c) .NET Foundation and contributors. All rights reserved. Licensed under the Microsoft Reciprocal License. See LICENSE.TXT file in the project root for full license information. -->";
  for (const file of ["CyberGuardTheme.xml", "CyberGuardTheme.wxl"]) {
    const lines = readFileSync(join(THEME, file), "utf8").split("\n");
    assert.equal(lines[1], notice, `${file}: WiX's notice, untouched`);
    assert.match(lines[2], /^<!-- .*LICENSE\.TXT.* beside this file.* -->$/, `${file}: the line that places it beside the file`);
  }
});

// The Amendment 2026-11-14, point 4: cg-setup's runtime crates, which the setup carries, are read
// as the agent's are, from the one cargo metadata of the workspace.
test("the crates are cg-agent's and cg-setup's normal dependencies, transitively, once each, without the workspace's own", () => {
  const crates = cargoPackages(workspaceMetadata());
  assert.deepEqual(crates.map((p) => p.name).sort(), ["base64", "pem", "serde", "serde_derive"]);
});

test("a cargo metadata without cg-setup is refused rather than read as the agent's alone", () => {
  const json = workspaceMetadata();
  json.packages = json.packages.filter((p) => p.name !== "cg-setup");
  assert.throws(() => cargoPackages(json), /cg-setup/);
});

test("the report's WiX section names the files the setup carries, their release and source, and MS-RL's text verbatim", () => {
  const licence = readFileSync(join(THEME, "LICENSE.TXT"), "utf8");
  const section = wixSection(pins(), licence);
  assert.match(section, /^## /m, "a section of its own");
  assert.match(section, /Burn's engine.*the setup's own executable.*code unmodified.*version resource the setup's/s);
  assert.match(section, /`wixstdba\.exe`.*`logo\.png`.*unmodified/s);
  assert.match(section, /`thm\.xml` and `thm\.wxl`.*modified from WiX's `HyperlinkTheme` files/s);
  assert.match(section, /WiX 7\.0\.1/);
  assert.match(section, /`642f1a0`/);
  assert.match(section, /MS-RL/);
  assert.match(section, /`wix-7\.0\.1-source\.tar\.gz`/);
  assert.ok(section.includes(`\n\`\`\`text\n${licence}\`\`\`\n`), "the LICENSE.TXT verbatim, fenced");
});

test("the allowlist's WixToolset.Sdk reason reads the engine as the setup carries it", () => {
  const sdk = allowlist().packages.find((p) => p.ecosystem === "nuget" && p.name === "WixToolset.Sdk");
  assert.equal(sdk.licence, "MS-RL");
  assert.match(sdk.reason, /code unmodified/);
  assert.match(sdk.reason, /version resource the setup's/);
  assert.match(sdk.reason, /WiX's copyright line/);
  assert.match(sdk.reason, /SPEC-024 Amendment 2026-11-14/);
});

test("only the pins the setup or the MSI carries are read, each under its licence", () => {
  assert.deepEqual(nugetPackages(pins()).map((p) => `${p.ecosystem}:${p.name}@${p.version}:${p.licence}`).sort(), [
    "nuget:WixToolset.BootstrapperApplications.wixext@7.0.1:MS-RL",
    "nuget:WixToolset.Sdk@7.0.1:MS-RL",
  ]);
});

test("the carried pins are admitted by name, and MS-RL is not admitted in general", () => {
  const list = allowlist();
  const carried = nugetPackages(pins());
  assert.equal(carried.length, 2);
  assert.deepEqual(refused(carried, list), []);
  assert.ok(!list.licences.includes("MS-RL"));
  const named = list.packages.filter((p) => p.ecosystem === "nuget");
  assert.equal(named.length, 2);
  for (const entry of named) assert.ok(entry.reason.length > 0);
});

test("a carried pin the list does not name is refused, naming it", () => {
  const unnamed = { packages: [{ id: "WixToolset.Other.wixext", version: "7.0.1", carried: true, licence: "MS-RL" }] };
  const bad = refused(nugetPackages(unnamed), allowlist());
  assert.equal(bad.length, 1);
  assert.equal(bad[0].name, "WixToolset.Other.wixext");
});

test("a named package under another licence is refused, a permissive one included", () => {
  for (const licence of ["GPL-3.0-only", "MIT"]) {
    const changed = { packages: [{ id: "WixToolset.Sdk", version: "7.0.1", carried: true, licence }] };
    assert.equal(refused(nugetPackages(changed), allowlist()).length, 1, licence);
  }
});

test("a carried pin is admitted by name alone: an unnamed one declaring a permissive licence is refused", () => {
  const unnamed = { packages: [{ id: "WixToolset.Other.wixext", version: "7.0.1", carried: true, licence: "MIT" }] };
  const bad = refused(nugetPackages(unnamed), allowlist());
  assert.equal(bad.length, 1);
  assert.match(bad[0].why, /by name/);
});

test("a pins file with nothing carried fails the command; the repository's pins enter the report", () => {
  const dir = mkdtempSync(join(tmpdir(), "cg-licences-"));
  try {
    const pnpm = { MIT: [{ name: "left-pad", versions: ["1.3.0"], license: "MIT" }] };
    writeFileSync(join(dir, "pnpm.json"), JSON.stringify(pnpm));
    writeFileSync(join(dir, "cargo.json"), JSON.stringify(workspaceMetadata()));
    writeFileSync(join(dir, "pins.json"), JSON.stringify({ packages: [{ id: "wix", version: "7.0.1", carried: false }] }));
    const script = join(REPO, ".github", "scripts", "check-licences.mjs");
    const base = [script, "--pnpm", join(dir, "pnpm.json"), "--cargo", join(dir, "cargo.json")];
    const r = spawnSync(process.execPath, [...base, "--nuget", join(dir, "pins.json")], { encoding: "utf8" });
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stderr, /no nuget packages read/);
    const ok = spawnSync(process.execPath, [...base, "--report", join(dir, "report.md")], { encoding: "utf8" });
    assert.equal(ok.status, 0, ok.stdout + ok.stderr);
    const report = readFileSync(join(dir, "report.md"), "utf8");
    assert.ok(report.includes("| nuget | WixToolset.Sdk | 7.0.1 | MS-RL |"), report);
    assert.ok(report.includes("| crate | pem | 1.0.0 | MIT |"), "a crate only cg-setup uses");
    assert.equal(report.split("| crate | serde | 1.0.0 | MIT |").length, 2, "a crate both use, once");
    assert.doesNotMatch(report, /\| crate \| cg-(agent|setup) \|/);
    // The WiX section, with the theme's LICENSE.TXT verbatim (SPEC-024 Amendment 2026-11-14, point 4).
    assert.match(report, /`wix-7\.0\.1-source\.tar\.gz`/);
    assert.ok(report.includes(readFileSync(join(THEME, "LICENSE.TXT"), "utf8")), "the LICENSE.TXT in the report");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
