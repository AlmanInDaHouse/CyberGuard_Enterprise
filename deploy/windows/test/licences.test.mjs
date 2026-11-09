// SPEC-024 Amendment 2026-11-01, point 1: the licence check reads the nuget pins of what the
// setup or the MSI carries and admits them by name, under MS-RL.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { nugetPackages, refused } from "../../../.github/scripts/check-licences.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const pins = () => JSON.parse(readFileSync(join(REPO, "deploy", "windows", "pins", "pins.json"), "utf8"));
const allowlist = () => JSON.parse(readFileSync(join(REPO, ".github", "scripts", "licences-allowed.json"), "utf8"));

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
    const cargo = {
      packages: [
        { id: "cg-agent 0.1.0", name: "cg-agent", version: "0.1.0", license: "Apache-2.0" },
        { id: "serde 1.0.0", name: "serde", version: "1.0.0", license: "MIT" },
      ],
      resolve: {
        nodes: [
          { id: "cg-agent 0.1.0", deps: [{ pkg: "serde 1.0.0", dep_kinds: [{ kind: null, target: null }] }] },
          { id: "serde 1.0.0", deps: [] },
        ],
      },
    };
    writeFileSync(join(dir, "pnpm.json"), JSON.stringify(pnpm));
    writeFileSync(join(dir, "cargo.json"), JSON.stringify(cargo));
    writeFileSync(join(dir, "pins.json"), JSON.stringify({ packages: [{ id: "wix", version: "7.0.1", carried: false }] }));
    const script = join(REPO, ".github", "scripts", "check-licences.mjs");
    const base = [script, "--pnpm", join(dir, "pnpm.json"), "--cargo", join(dir, "cargo.json")];
    const r = spawnSync(process.execPath, [...base, "--nuget", join(dir, "pins.json")], { encoding: "utf8" });
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stderr, /no nuget packages read/);
    const ok = spawnSync(process.execPath, [...base, "--report", join(dir, "report.md")], { encoding: "utf8" });
    assert.equal(ok.status, 0, ok.stdout + ok.stderr);
    assert.ok(readFileSync(join(dir, "report.md"), "utf8").includes("| nuget | WixToolset.Sdk | 7.0.1 | MS-RL |"));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
