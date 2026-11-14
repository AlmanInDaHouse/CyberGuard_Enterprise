// SPEC-024 Amendment 2026-11-01, point 1: the pins agree with each other, each restored nuget
// package is checked against its recorded hash before use, and the tool is the pinned release
// under the EULA Manuel accepted. package-ci.yml runs the verifier over the real restore.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { consistency, toolProblems, verifyPackages } from "../pins/verify-pins.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(ROOT, p), "utf8");
const realFiles = () => ({
  pins: JSON.parse(read("pins/pins.json")),
  globalJson: JSON.parse(read("global.json")),
  toolManifest: JSON.parse(read(".config/dotnet-tools.json")),
  pinsProject: read("pins/Pins.csproj"),
});
const sha512 = (bytes) => createHash("sha512").update(bytes).digest("base64");

test("the repository's pins agree across global.json, the tool manifest and the download project", () => {
  const files = realFiles();
  assert.deepEqual(consistency(files), []);
  assert.equal(files.globalJson.sdk.rollForward, "disable");
  assert.equal(files.pins.eulaId, "wix7");
  // SPEC-024 Amendment 2026-11-14: the tag v7.0.1's commit and its tree, as read upstream.
  assert.equal(files.pins.wixCommit, "642f1a0f7c857ffa4fc9fa3f9d1433aab895dd29");
  assert.equal(files.pins.wixTree, "7ebe172bb319de1070dbac860425531ac04379c0");
});

test("a drift between the pin files is named", () => {
  const cases = [
    ["sdk version", (f) => (f.globalJson.sdk.version = "10.0.404"), /global\.json.*10\.0\.404/],
    ["sdk roll forward", (f) => (f.globalJson.sdk.rollForward = "latestPatch"), /rollForward/],
    ["msbuild sdk", (f) => (f.globalJson["msbuild-sdks"]["WixToolset.Sdk"] = "7.0.0"), /WixToolset\.Sdk.*7\.0\.0/],
    ["tool version", (f) => (f.toolManifest.tools.wix.version = "7.0.0"), /wix.*7\.0\.0/],
    [
      "a floating download",
      (f) => (f.pinsProject = f.pinsProject.replace('Include="wix" Version="[7.0.1]"', 'Include="wix" Version="7.0.1"')),
      /wix.*exact/,
    ],
    [
      "a package nothing downloads",
      (f) => (f.pinsProject = f.pinsProject.replace(/<PackageDownload Include="WixToolset\.Sdk"[^>]*>/, "")),
      /WixToolset\.Sdk/,
    ],
    [
      "a download nothing pins",
      (f) => (f.pinsProject = f.pinsProject.replace("</ItemGroup>", '<PackageDownload Include="Other" Version="[1.0.0]" /></ItemGroup>')),
      /Other/,
    ],
    [
      "a download written with a closing tag",
      (f) => (f.pinsProject = f.pinsProject.replace('<PackageDownload Include="wix" Version="[7.0.1]" />', '<PackageDownload Include="wix" Version="[7.0.1]"></PackageDownload>')),
      /PackageDownload/,
    ],
    [
      "a package reference outside the pins",
      (f) => (f.pinsProject = f.pinsProject.replace("</ItemGroup>", '<PackageReference Include="Other" Version="1.*" /></ItemGroup>')),
      /PackageReference/,
    ],
    ["a package off the release", (f) => (f.pins.packages[1].version = "7.0.0"), /WixToolset\.Sdk.*7\.0\.0/],
    ["a hash of the wrong length", (f) => (f.pins.packages[0].sha512 = "AAAA"), /wix.*SHA-512/],
    ["another EULA", (f) => (f.pins.eulaId = "wix8"), /wix8/],
    ["a carried package with no licence", (f) => delete f.pins.packages[1].licence, /WixToolset\.Sdk.*licen/],
    ["a package listed twice", (f) => f.pins.packages.push({ ...f.pins.packages[0] }), /wix.*twice/],
    // SPEC-024 Amendment 2026-11-14, point 3: the commit and its tree, by their full IDs, and
    // the informational version naming that commit.
    ["no commit", (f) => delete f.pins.wixCommit, /wixCommit/],
    ["a commit in upper case", (f) => (f.pins.wixCommit = f.pins.wixCommit.toUpperCase()), /wixCommit.*40 lower-case hex/],
    ["an abbreviated commit", (f) => (f.pins.wixCommit = f.pins.wixCommit.slice(0, 12)), /wixCommit.*40 lower-case hex/],
    ["no tree", (f) => delete f.pins.wixTree, /wixTree/],
    ["a tree of 39 digits", (f) => (f.pins.wixTree = f.pins.wixTree.slice(1)), /wixTree.*40 lower-case hex/],
    [
      "an informational version naming another commit",
      (f) => (f.pins.wixInformationalVersion = "7.0.1+642f1a1"),
      /wixInformationalVersion 7\.0\.1\+642f1a1 does not name wixCommit/,
    ],
    [
      "a commit the informational version does not name",
      (f) => (f.pins.wixCommit = `a${f.pins.wixCommit.slice(1)}`),
      /wixInformationalVersion 7\.0\.1\+642f1a0 does not name wixCommit a42f1a0/,
    ],
    ["an informational version naming no commit", (f) => (f.pins.wixInformationalVersion = "7.0.1"), /wixInformationalVersion 7\.0\.1 does not name/],
  ];
  for (const [name, change, expected] of cases) {
    const files = realFiles();
    change(files);
    const problems = consistency(files);
    assert.ok(
      problems.some((p) => expected.test(p)),
      `${name}: ${JSON.stringify(problems)}`,
    );
  }
});

test("the .NET CLI's telemetry is off: the command refuses to run with it on, and CI sets it", () => {
  const env = { ...process.env };
  delete env.DOTNET_CLI_TELEMETRY_OPTOUT;
  const r = spawnSync(process.execPath, [join(ROOT, "pins", "verify-pins.mjs")], { encoding: "utf8", env });
  assert.equal(r.status, 1, r.stdout + r.stderr);
  assert.match(r.stderr, /DOTNET_CLI_TELEMETRY_OPTOUT/);
  const workflow = readFileSync(join(ROOT, "..", "..", ".github", "workflows", "package-ci.yml"), "utf8");
  assert.match(workflow, /^ {2}DOTNET_CLI_TELEMETRY_OPTOUT: "1"$/m);
  assert.match(workflow, /^env:$/m);
});

function withPackages(fn) {
  const dir = mkdtempSync(join(tmpdir(), "cg-pins-"));
  try {
    const bytes = Buffer.from("a package's bytes");
    const file = join(dir, "wixtoolset.sdk", "7.0.1", "wixtoolset.sdk.7.0.1.nupkg");
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, bytes);
    fn({ dir, file, bytes, pins: { packages: [{ id: "WixToolset.Sdk", version: "7.0.1", sha512: sha512(bytes) }] } });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("a retained package with its recorded hash passes, under NuGet's lower-case layout", () => {
  withPackages(({ dir, pins }) => assert.deepEqual(verifyPackages(pins, dir), []));
});

test("one changed byte in a retained package fails, naming the package", () => {
  withPackages(({ dir, file, bytes, pins }) => {
    const tampered = Buffer.from(bytes);
    tampered[0] ^= 1;
    writeFileSync(file, tampered);
    const problems = verifyPackages(pins, dir);
    assert.equal(problems.length, 1);
    assert.match(problems[0], /WixToolset\.Sdk 7\.0\.1.*hash/);
  });
});

test("a package that was not retained fails rather than passes", () => {
  withPackages(({ dir, file, pins }) => {
    rmSync(file);
    assert.match(verifyPackages(pins, dir)[0] ?? "", /WixToolset\.Sdk 7\.0\.1.*not found/);
  });
});

test("no package listed is a failure, not a pass", () => {
  withPackages(({ dir }) => assert.ok(verifyPackages({ packages: [] }, dir).length > 0));
});

const TOOL = { wixInformationalVersion: "7.0.1+642f1a0", eulaId: "wix7" };

test("the tool must be the pinned build and ask for the EULA Manuel accepted", () => {
  const help = "Commands:\n  accept            Accept the WiX OSMF EULA. This versions eulaId = wix7\n";
  assert.deepEqual(toolProblems(TOOL, "7.0.1+642f1a0\n", help), []);
  assert.match(toolProblems(TOOL, "7.0.2+abc1234\n", help)[0] ?? "", /7\.0\.2\+abc1234/);
  assert.match(toolProblems(TOOL, "7.0.1+642f1a0\n", help.replace("wix7", "wix8"))[0] ?? "", /EULA/);
  assert.match(toolProblems(TOOL, "7.0.1+642f1a0\n", "no such text")[0] ?? "", /EULA/);
});

test("the command exits 1 and names the packages when the packages folder is empty", () => {
  const dir = mkdtempSync(join(tmpdir(), "cg-pins-empty-"));
  try {
    const env = { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1" };
    const r = spawnSync(process.execPath, [join(ROOT, "pins", "verify-pins.mjs"), "--packages", dir], { encoding: "utf8", env });
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stderr, /wix 7\.0\.1/);
    assert.match(r.stderr, /WixToolset\.BootstrapperApplications\.wixext 7\.0\.1/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
