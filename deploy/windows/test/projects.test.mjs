// SPEC-024 Amendment 2026-11-01, point 1: the WiX projects accept only the EULA Manuel accepted,
// each restores against a committed lock file in locked mode, and a package a project references
// is one pins.json pins at its exact version. The lock file covers PackageReference items alone;
// the MSBuild SDK, from global.json, is verified by pins/verify-pins.mjs before any build.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(ROOT, p), "utf8");
const pins = JSON.parse(read("pins/pins.json"));

/** The .wixproj files under deploy/windows, relative to it. */
function projects() {
  const out = [];
  const walk = (dir) => {
    for (const entry of readdirSync(join(ROOT, dir), { withFileTypes: true })) {
      if (entry.isDirectory() && !["bin", "obj", "node_modules"].includes(entry.name)) walk(join(dir, entry.name));
      else if (entry.isFile() && entry.name.endsWith(".wixproj")) out.push(join(dir, entry.name));
    }
  };
  walk(".");
  return out;
}

test("the WiX projects' properties accept the pinned EULA and lock their restore", () => {
  const props = read("Directory.Build.props");
  const group = /<PropertyGroup Condition=" '\$\(MSBuildProjectExtension\)' == '\.wixproj' ">([\s\S]*?)<\/PropertyGroup>/.exec(props);
  assert.ok(group, "Directory.Build.props sets its properties for .wixproj projects alone");
  const value = (name) => new RegExp(`<${name}>([^<]*)</${name}>`).exec(group[1])?.[1];
  assert.equal(value("AcceptEula"), pins.eulaId);
  assert.equal(value("RestorePackagesWithLockFile"), "true");
  assert.equal(value("RestoreLockedMode"), "true");
  assert.equal(value("TreatWarningsAsErrors"), "true");
  assert.equal(value("ProductVersion"), "$(CG_RELEASE_VERSION)");
  assert.equal(props.match(/<PropertyGroup\b/g).length, 1, "no property outside that group");
});

test("each WiX project has a committed lock file and references only pinned packages", () => {
  const found = projects();
  assert.ok(found.length > 0, "a WiX project exists");
  const tracked = new Set(
    execFileSync("git", ["ls-files", "--", "."], { cwd: ROOT, encoding: "utf8" })
      .split("\n")
      .filter(Boolean)
      .map((p) => p.replaceAll("\\", "/")),
  );
  const pinned = new Map(pins.packages.map((p) => [p.id, p.version]));
  for (const project of found) {
    const lock = relative(ROOT, join(ROOT, dirname(project), "packages.lock.json")).replaceAll("\\", "/");
    assert.ok(tracked.has(lock), `${lock} is committed`);
    const text = read(project);
    const references = [...text.matchAll(/<PackageReference\s+Include="([^"]+)"\s+Version="([^"]+)"\s*\/>/g)];
    assert.equal(
      references.length,
      text.match(/<PackageReference\b/g)?.length ?? 0,
      `${project}: each PackageReference has the form this check reads`,
    );
    const locked = JSON.parse(read(lock)).dependencies;
    for (const [, id, version] of references) {
      assert.ok(pinned.has(id), `${project} references ${id}, which pins.json does not pin`);
      assert.equal(version, `[${pinned.get(id)}]`, `${project} takes ${id} at its exact pin`);
    }
    for (const framework of Object.values(locked)) {
      for (const [id, entry] of Object.entries(framework)) {
        assert.equal(entry.resolved, pinned.get(id), `${lock} resolves ${id} at its pin`);
      }
    }
  }
});

test("the MSI's targets check the version and the agent before WiX compiles", () => {
  const targets = read("Directory.Build.targets");
  assert.match(targets, /<Target Name="CheckProductVersion"\s+BeforeTargets="CoreCompile"\s+Condition=" '\$\(MSBuildProjectExtension\)' == '\.wixproj' ">/);
  assert.match(targets, /\.github\\scripts\\release-version\.mjs/);
  assert.match(targets, /Exists\('\$\(AgentExe\)'\)/);
});
