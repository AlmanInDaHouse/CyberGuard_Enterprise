// SPEC-024 install_ac_015 and its Amendment 2026-11-01, point 3 (debt #139): the
// release refuses an absent, malformed or out-of-range version before anything is pushed,
// attested or published. The check's accepted and refused tags, through its module and its
// command line, and release.yml's order: its first job runs the check with `contents: read`
// alone, compares the printed version with the tag's, and every other job depends on it,
// with no condition or tolerated failure that would let one run past a failed check.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import { releaseVersion } from "../../../.github/scripts/release-version.mjs";

const REPO = fileURLToPath(new URL("../../../", import.meta.url));
const SCRIPT = join(REPO, ".github", "scripts", "release-version.mjs");

const ACCEPTED = [
  ["v0.0.0", "0.0.0"],
  ["v1.2.3", "1.2.3"],
  ["v10.20.30", "10.20.30"],
  ["v255.255.65535", "255.255.65535"],
];

const REFUSED = [
  "",
  "v",
  "1.2.3",
  "v1.2",
  "v1.2.3.4",
  "v01.2.3",
  "v1.02.3",
  "v1.2.03",
  "v1.2.3-rc.1",
  "v1.2.3+build",
  "v 1.2.3",
  "v1.2.3 ",
  "V1.2.3",
  "v-1.2.3",
  "v256.0.0",
  "v0.256.0",
  "v0.0.65536",
  "v300.0.0",
  "v99999999999999999999.0.0",
];

describe("the release's version check (SPEC-024 Amendment 2026-11-01, point 3; #139)", () => {
  test("accepts v<X>.<Y>.<Z> within MSI's limits and returns X.Y.Z", () => {
    for (const [tag, version] of ACCEPTED) {
      assert.equal(releaseVersion(tag), version, tag);
    }
  });

  test("refuses every other tag, the out-of-range ones included", () => {
    for (const tag of REFUSED) {
      assert.ok(releaseVersion(tag) instanceof Error, `${JSON.stringify(tag)} must be refused`);
    }
  });

  test("its command line prints the version, or exits 1 with the reason", () => {
    const none = spawnSync(process.execPath, [SCRIPT], { encoding: "utf8" });
    assert.equal(none.status, 1, "no tag at all");
    const good = spawnSync(process.execPath, [SCRIPT, "v1.2.3"], { encoding: "utf8" });
    assert.equal(good.status, 0, good.stderr);
    assert.equal(good.stdout.trim(), "1.2.3");
    for (const tag of ["v300.0.0", "v01.2.3", ""]) {
      const bad = spawnSync(process.execPath, [SCRIPT, tag], { encoding: "utf8" });
      assert.equal(bad.status, 1, `${JSON.stringify(tag)}: ${bad.stdout}`);
      assert.match(bad.stderr, /^release version: /, tag);
      assert.equal(bad.stdout, "", tag);
    }
  });
});

/** release.yml's jobs: each one's `needs` and its `permissions` lines. */
function jobs() {
  const text = readFileSync(join(REPO, ".github", "workflows", "release.yml"), "utf8");
  const lines = text.split("\n");
  const start = lines.indexOf("jobs:");
  assert.ok(start >= 0, "release.yml has a jobs: block");
  const found = new Map();
  let current = null;
  let inPermissions = false;
  for (const line of lines.slice(start + 1)) {
    const job = /^ {2}([A-Za-z_][A-Za-z0-9_-]*):\s*$/.exec(line);
    if (job) {
      current = { needs: [], permissions: [], text: [] };
      found.set(job[1], current);
      inPermissions = false;
      continue;
    }
    if (!current) continue;
    current.text.push(line);
    const needs = /^ {4}needs:\s*(.+)$/.exec(line);
    if (needs) {
      current.needs = needs[1]
        .replace(/[[\]]/g, "")
        .split(",")
        .map((n) => n.trim())
        .filter(Boolean);
    }
    if (/^ {4}permissions:\s*$/.test(line)) {
      inPermissions = true;
      continue;
    }
    if (inPermissions) {
      const permission = /^ {6}([a-z-]+):\s*([a-z]+)\s*$/.exec(line);
      if (permission) {
        current.permissions.push(`${permission[1]}: ${permission[2]}`);
        continue;
      }
      inPermissions = false;
    }
  }
  return found;
}

function dependsOn(all, name, target, seen = new Set()) {
  if (seen.has(name)) return false;
  seen.add(name);
  const job = all.get(name);
  return job.needs.includes(target) || job.needs.some((n) => dependsOn(all, n, target, seen));
}

describe("release.yml checks the tag before any other job (#139)", () => {
  test("its version job needs no other job and has contents: read alone", () => {
    const all = jobs();
    const version = all.get("version");
    assert.ok(version, "release.yml has a version job");
    assert.deepEqual(version.needs, []);
    assert.deepEqual(version.permissions, ["contents: read"]);
    const run = version.text.map((l) => l.trim());
    const at = run.indexOf("set -eu");
    assert.ok(at > 0 && run[at - 1] === "run: |", "the check runs in a strict shell");
    assert.equal(
      run[at + 1],
      'test "$(node .github/scripts/release-version.mjs "$GITHUB_REF_NAME")" = "${GITHUB_REF_NAME#v}"',
      "the job compares the printed version with the tag's",
    );
  });

  test("no job runs past a failed check: no condition, no tolerated failure", () => {
    for (const [name, job] of jobs()) {
      for (const line of job.text) {
        assert.ok(!/^\s*(-\s+)?if:/.test(line), `${name} has a condition: ${line}`);
        assert.ok(!/continue-on-error/.test(line), `${name} tolerates a failure: ${line}`);
      }
    }
  });

  test("every other job depends on it, directly or through another job", () => {
    const all = jobs();
    assert.ok(all.size > 1, "release.yml has other jobs");
    for (const name of all.keys()) {
      if (name === "version") continue;
      assert.ok(dependsOn(all, name, "version"), `${name} must wait for the version check`);
    }
  });
});
