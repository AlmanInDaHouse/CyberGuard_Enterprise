// SPEC-024 Amendment 2026-11-01, point 1: an advisory check of the nuget pins joins
// dependency-audit. It reads nuget.org's vulnerability feed, whose ranges are NuGet's.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { advisories, inRange } from "../pins/audit-pins.mjs";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "..", "pins", "audit-pins.mjs");

test("NuGet version ranges", () => {
  const cases = [
    ["7.0.1", "(, 7.0.2)", true],
    ["7.0.2", "(, 7.0.2)", false],
    ["7.0.2", "(, 7.0.2]", true],
    ["7.0.1", "[4.0.0, 4.0.5)", false],
    ["4.0.0", "[4.0.0, 4.0.5)", true],
    ["4.0.0", "(4.0.0, 4.0.5)", false],
    ["7.0.1", "[7.0.0, )", true],
    ["7.0.1", "[7.0.1]", true],
    ["7.0.1", "[7.0.2]", false],
    ["7.0.1", "(, )", true],
    ["7.0.1", "[7.0.1-rc.1, 7.0.1)", false],
    ["7.0.1", "[7.0.1-rc.1, 7.0.2)", true],
    ["7.0.1", "(, 7.0.1-rc.1)", false],
    ["7.0.1", "[7.0.0.1, 7.0.1.1)", true],
    ["7.10.0", "(, 7.9.0)", false],
  ];
  for (const [version, range, expected] of cases) assert.equal(inRange(version, range), expected, `${version} in ${range}`);
});

test("a range it cannot read throws, so the check fails closed", () => {
  for (const range of ["7.0.1", "[7.0.1", "[a, b)", "", "[7.0.1, 7.0.2, 7.0.3)"]) {
    assert.throws(() => inRange("7.0.1", range), undefined, range);
  }
});

const PINS = {
  packages: [
    { id: "WixToolset.Sdk", version: "7.0.1" },
    { id: "wix", version: "7.0.1" },
  ],
};

test("an advisory that covers a pin is reported, whatever the id's case, and one that does not is not", () => {
  const pages = [
    {
      "wixtoolset.sdk": [
        { url: "https://example.test/GHSA-new", severity: 2, versions: "[7.0.0, 7.0.2)" },
        { url: "https://example.test/GHSA-old", severity: 2, versions: "(, 4.0.5)" },
      ],
    },
    { wix: [{ url: "https://example.test/GHSA-other", severity: 1, versions: "[8.0.0, )" }] },
  ];
  assert.deepEqual(advisories(PINS, pages), [
    { id: "WixToolset.Sdk", version: "7.0.1", url: "https://example.test/GHSA-new", severity: 2, versions: "[7.0.0, 7.0.2)" },
  ]);
});

function runCli(pages) {
  const dir = mkdtempSync(join(tmpdir(), "cg-audit-"));
  try {
    writeFileSync(join(dir, "pages.json"), JSON.stringify(pages));
    return spawnSync(process.execPath, [SCRIPT, "--pages", join(dir, "pages.json")], { encoding: "utf8" });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("the command fails naming the advisory, passes on a clean feed, and fails on an empty one", () => {
  const dirty = runCli([{ wix: [{ url: "https://example.test/GHSA-x", severity: 3, versions: "(, 8.0.0)" }] }]);
  assert.equal(dirty.status, 1, dirty.stdout + dirty.stderr);
  assert.match(dirty.stderr, /wix 7\.0\.1.*GHSA-x/);
  const clean = runCli([{ wix: [{ url: "https://example.test/GHSA-old", severity: 2, versions: "(, 4.0.5)" }] }]);
  assert.equal(clean.status, 0, clean.stdout + clean.stderr);
  const empty = runCli([{}]);
  assert.equal(empty.status, 1, empty.stdout + empty.stderr);
  assert.match(empty.stderr, /no advisories read/);
});
