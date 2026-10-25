// SPEC-023 deploy_ac_019's licence half (§Operational §12, debt #112): the licence checker
// fails, naming the package, on a fixture with a licence outside the allowlist, and reads the
// SPDX expressions as the allowlist says. That it passes on the repository is deploy-ci.yml's
// step and dependency-audit.yml's licences job, which run it over the installed dependencies.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { admitted, cargoPackages, pnpmPackages, refused } from "../../../.github/scripts/check-licences.mjs";
import { REPO, run } from "./lib.mjs";

const ALLOWED = new Set(["MIT", "Apache-2.0", "BSD-3-Clause"]);

test("an SPDX expression: OR admits one side, AND both, WITH its base, X/Y as OR; Unknown and a malformed one are not admitted", () => {
  assert.equal(admitted("MIT", ALLOWED), true);
  assert.equal(admitted("GPL-3.0-only", ALLOWED), false);
  assert.equal(admitted("MIT OR GPL-3.0-only", ALLOWED), true);
  assert.equal(admitted("MIT AND GPL-3.0-only", ALLOWED), false);
  assert.equal(admitted("(MIT OR Apache-2.0) AND BSD-3-Clause", ALLOWED), true);
  assert.equal(admitted("(MIT OR GPL-2.0-only) AND LGPL-3.0-or-later", ALLOWED), false);
  assert.equal(admitted("Apache-2.0 WITH LLVM-exception OR GPL-2.0-only", ALLOWED), true);
  assert.equal(admitted("MIT/Apache-2.0", ALLOWED), true);
  assert.equal(admitted("Unknown", ALLOWED), false);
  assert.throws(() => admitted("(MIT OR Apache-2.0", ALLOWED));
  assert.throws(() => admitted("MIT WITH )", ALLOWED));
});

test("the agent's crates are its normal dependencies, transitively, without its development ones", () => {
  const pkg = (name) => ({ id: `${name} 1.0.0`, name, version: "1.0.0", license: "MIT" });
  const normal = [{ kind: null, target: null }];
  const json = {
    packages: [pkg("cg-agent"), pkg("serde"), pkg("serde_derive"), pkg("bollard")],
    resolve: {
      nodes: [
        { id: "cg-agent 1.0.0", deps: [{ pkg: "serde 1.0.0", dep_kinds: normal }, { pkg: "bollard 1.0.0", dep_kinds: [{ kind: "dev", target: null }] }] },
        { id: "serde 1.0.0", deps: [{ pkg: "serde_derive 1.0.0", dep_kinds: normal }] },
        { id: "serde_derive 1.0.0", deps: [] },
        { id: "bollard 1.0.0", deps: [] },
      ],
    },
  };
  assert.deepEqual(cargoPackages(json).map((p) => p.name).sort(), ["serde", "serde_derive"]);
});

test("a package the allowlist names is admitted under its licence too, and under no other outside the list", () => {
  const allowlist = { licences: ["MIT"], packages: [{ ecosystem: "npm", name: "caniuse-lite", licence: "CC-BY-4.0", reason: "data" }] };
  const pkg = (licence) => ({ ecosystem: "npm", name: "caniuse-lite", version: "1.0.0", licence });
  assert.deepEqual(refused([pkg("CC-BY-4.0"), pkg("MIT")], allowlist), []);
  const changed = refused([pkg("GPL-3.0-only")], allowlist);
  assert.equal(changed.length, 1);
  assert.equal(changed[0].name, "caniuse-lite");
});

test("the checker fails on a fixture with a licence outside the list, naming the package, and writes its report", () => {
  const dir = mkdtempSync(join(tmpdir(), "cg-licences-"));
  try {
    const pnpm = {
      MIT: [{ name: "left-pad", versions: ["1.3.0"], license: "MIT" }],
      "GPL-3.0-only": [{ name: "copyleft-thing", versions: ["2.0.0"], license: "GPL-3.0-only" }],
    };
    const cargo = {
      packages: [
        { id: "cg-agent 0.1.0", name: "cg-agent", version: "0.1.0", license: "Apache-2.0" },
        { id: "agpl-crate 0.3.0", name: "agpl-crate", version: "0.3.0", license: "AGPL-3.0-only" },
      ],
      resolve: { nodes: [{ id: "cg-agent 0.1.0", deps: [{ pkg: "agpl-crate 0.3.0", dep_kinds: [{ kind: null, target: null }] }] }, { id: "agpl-crate 0.3.0", deps: [] }] },
    };
    writeFileSync(join(dir, "pnpm.json"), JSON.stringify(pnpm));
    writeFileSync(join(dir, "cargo.json"), JSON.stringify(cargo));
    const r = run(process.execPath, [join(REPO, ".github", "scripts", "check-licences.mjs"), "--pnpm", join(dir, "pnpm.json"), "--cargo", join(dir, "cargo.json"), "--report", join(dir, "report.md")]);
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stderr, /licence not admitted: npm copyleft-thing@2\.0\.0: GPL-3\.0-only/);
    assert.match(r.stderr, /licence not admitted: crate agpl-crate@0\.3\.0: AGPL-3\.0-only/);
    assert.doesNotMatch(r.stderr, /left-pad/);
    // An input that yields no package of an ecosystem fails rather than passes.
    writeFileSync(join(dir, "none.json"), "{}");
    const none = run(process.execPath, [join(REPO, ".github", "scripts", "check-licences.mjs"), "--pnpm", join(dir, "none.json"), "--cargo", join(dir, "cargo.json")]);
    assert.equal(none.status, 1);
    assert.match(none.stderr, /no npm packages read/);
    const report = readFileSync(join(dir, "report.md"), "utf8");
    assert.ok(report.includes("| npm | left-pad | 1.3.0 | MIT |"), report);
    assert.ok(report.includes("| crate | agpl-crate | 0.3.0 | AGPL-3.0-only |"), report);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
