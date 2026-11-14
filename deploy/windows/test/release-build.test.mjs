// SPEC-024 Amendment 2026-11-14, point 1: the build release.yml's agent job and package-ci's dry
// run make the Windows assets with (.github/scripts/release-windows.mjs build), read as data and by
// its guards, without building anything: its steps and their order, the absolute paths MSBuild is
// given whatever the directories' form, the environment its steps run in, and what it refuses
// before it builds (an output or work directory not empty, a licence report without WiX's
// LICENSE.TXT, a change where the build reads the repository) and after (an output directory that
// is not the four assets).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  BUILD_INPUTS,
  SCRUBBED_VARIABLES,
  assetNames,
  buildEnvironment,
  buildRelease,
  cleanTreeProblems,
  emptyDirectoryProblems,
  outputProblems,
  preflightProblems,
  releaseSteps,
  repositoryProblems,
  reportProblems,
  statusPaths,
} from "../../../.github/scripts/release-windows.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const LICENCE = readFileSync(join(REPO, "deploy", "windows", "bundle", "theme", "LICENSE.TXT"), "utf8");

function withTemp(fn) {
  const dir = mkdtempSync(join(tmpdir(), "cg-release-build-test-"));
  try {
    return fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** The plan for relative directories, as release.yml gives `--out assets`. */
const plan = () => releaseSteps({ version: "1.2.3", out: "assets", work: join("work", "release"), wixRelease: "7.0.1" });
const runs = (p) => p.steps.filter((s) => !s.copy);
const isWixBuild = (s) => s.command === "dotnet" && s.args[0] === "build" && s.args.some((a) => /\.wixproj$/.test(a));

test("every path the build gives dotnet, node and the copies is absolute, though out and work are relative", () => {
  const p = plan();
  assert.ok(isAbsolute(p.out) && isAbsolute(p.work));
  assert.equal(relative(process.cwd(), p.out), "assets");
  const paths = [];
  for (const s of p.steps) {
    if (s.copy) {
      paths.push(s.copy.from, s.copy.to);
      continue;
    }
    assert.ok(isAbsolute(s.cwd), `${s.name}'s directory ${s.cwd}`);
    for (const a of s.args) {
      // A property's value, or an argument that names a file: whatever has a separator or a project's extension.
      const value = /^-p:[^=]+=/.test(a) ? a.slice(a.indexOf("=") + 1) : a;
      if (/[\\/]|\.(cs|wix)proj$|\.mjs$/.test(value)) paths.push(value);
    }
  }
  for (const path of paths) assert.ok(isAbsolute(path), `${path} is not absolute`);
  // Each property MSBuild resolves against a project's directory is among them.
  const properties = runs(p)
    .flatMap((s) => s.args)
    .filter((a) => /^-p:(AgentExe|Msi|SetupExe|OutputPath)=/.test(a));
  assert.equal(properties.length, 5, JSON.stringify(properties));
  for (const a of properties) assert.ok(isAbsolute(a.slice(a.indexOf("=") + 1)), a);
  assert.ok(properties.includes(`-p:AgentExe=${join(p.out, "cg-agent-1.2.3-x86_64-pc-windows-msvc.exe")}`));
  assert.ok(properties.includes(`-p:Msi=${join(p.out, "cyberguard-agent-1.2.3-x64.msi")}`));
  assert.ok(properties.includes(`-p:SetupExe=${join(p.work, "cg-setup.exe")}`));
});

test("the four pins checks run before WiX builds anything", () => {
  const steps = runs(plan());
  const pinChecks = [
    ["dotnet", "restore"],
    [process.execPath, "verify-pins.mjs"],
    ["dotnet", "tool"],
    [process.execPath, "verify-pins.mjs", "--tool"],
  ].map(([command, first, flag]) =>
    steps.findIndex((s) => s.command === command && s.args[0].endsWith(first) && (flag === undefined ? !s.args.includes("--tool") : s.args.includes(flag))),
  );
  assert.ok(pinChecks.every((i) => i >= 0), JSON.stringify(pinChecks));
  assert.deepEqual(steps.slice(0, 4).map((s) => s.name), ["the pinned packages", "the packages' hashes", "the wix tool", "the tool's build and EULA ID"]);
  const firstWix = steps.findIndex(isWixBuild);
  assert.ok(firstWix > 0, "a WiX project is built");
  assert.ok(Math.max(...pinChecks) < firstWix, `the pins checks ${pinChecks} before the WiX build ${firstWix}`);
});

test("cargo builds the agent alone, whose executable is copied at once, and then cg-setup", () => {
  const p = plan();
  const cargo = p.steps.map((s, i) => [s, i]).filter(([s]) => s.command === "cargo");
  assert.deepEqual(
    cargo.map(([s]) => s.args),
    [
      ["build", "--release", "--locked", "-p", "cg-agent"],
      ["build", "--release", "--locked", "-p", "cg-setup"],
    ],
  );
  for (const [s] of cargo) assert.deepEqual(s.env, { CG_RELEASE_VERSION: "1.2.3" });
  const [[, agentAt], [, setupAt]] = cargo;
  const agentCopy = p.steps.findIndex((s) => s.copy?.to === p.exe);
  const setupCopy = p.steps.findIndex((s) => s.copy?.to === p.setupExe);
  assert.equal(agentCopy, agentAt + 1, "the agent's executable is copied right after its build");
  assert.ok(agentCopy < setupAt, "and before cg-setup is built");
  assert.ok(p.steps[agentCopy].copy.from.endsWith(join("target", "release", "cg-agent.exe")));
  assert.ok(setupCopy > setupAt);
  assert.ok(p.steps[setupCopy].copy.from.endsWith(join("target", "release", "cg-setup.exe")));
});

test("the MSI is built with the test build's define empty, around the agent's asset", () => {
  const steps = runs(plan());
  const msi = steps.find((s) => isWixBuild(s) && s.args.some((a) => a.endsWith("CyberGuardAgent.wixproj")));
  assert.ok(msi);
  assert.deepEqual(
    msi.args.filter((a) => /CgInjectFailure/i.test(a)),
    ["-p:CgInjectFailure="],
  );
  assert.ok(!steps.some((s) => s !== msi && s.args.some((a) => /CgInjectFailure/i.test(a))));
  const setup = steps.find((s) => isWixBuild(s) && s.args.some((a) => a.endsWith("CyberGuardSetup.wixproj")));
  assert.ok(steps.indexOf(setup) > steps.indexOf(msi));
  assert.deepEqual(Object.values(plan().names), Object.values(assetNames("1.2.3", "7.0.1")));
});

test("no build step inherits a variable that would change what it builds", () => {
  assert.deepEqual(
    [...SCRUBBED_VARIABLES],
    [
      "RUSTFLAGS",
      "CARGO_ENCODED_RUSTFLAGS",
      "CARGO_BUILD_RUSTFLAGS",
      "RUSTC_WRAPPER",
      "RUSTC_WORKSPACE_WRAPPER",
      "CARGO_BUILD_RUSTC_WRAPPER",
      "CARGO_TARGET_DIR",
      "CARGO_BUILD_TARGET",
      "CARGO_BUILD_TARGET_DIR",
      "CgInjectFailure",
      "CustomBeforeMicrosoftCommonTargets",
      "CustomAfterMicrosoftCommonTargets",
      "NUGET_PACKAGES",
    ],
  );
  const parent = { Path: "C:\\bin", CARGO_HOME: "C:\\cargo", DOTNET_NOLOGO: "0" };
  for (const v of SCRUBBED_VARIABLES) parent[v] = "set";
  // Windows reads names without regard to case, and MSBuild takes a variable as a property so.
  Object.assign(parent, { rustflags: "-C target-feature=-crt-static", cginjectfailure: "1", Nuget_Packages: "C:\\elsewhere" });
  const env = buildEnvironment(parent, { CG_RELEASE_VERSION: "1.2.3" });
  assert.deepEqual(env, { Path: "C:\\bin", CARGO_HOME: "C:\\cargo", DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1", CG_RELEASE_VERSION: "1.2.3" });
});

test("the build's guards: empty directories before, the four assets after, the report's licence, a clean checkout", () => {
  assert.deepEqual(emptyDirectoryProblems("the output directory", "C:\\a", []), []);
  assert.deepEqual(emptyDirectoryProblems("the output directory", "C:\\a", ["old.msi", "cg-agent.exe"]), ["the output directory C:\\a is not empty: cg-agent.exe, old.msi"]);
  const names = assetNames("1.2.3", "7.0.1");
  const four = Object.values(names);
  assert.deepEqual(outputProblems("C:\\a", [...four].reverse(), names), []);
  assert.deepEqual(outputProblems("C:\\a", [...four, "SHA256SUMS"], names), ["C:\\a holds SHA256SUMS beside the four assets"]);
  assert.deepEqual(outputProblems("C:\\a", four.slice(1), names), [`C:\\a lacks ${four[0]}`]);
  assert.deepEqual(reportProblems("r.md", `# Licences\n\n${LICENCE}\n`, LICENCE), []);
  assert.deepEqual(reportProblems("r.md", `# Licences\r\n\r\n${LICENCE.replaceAll("\n", "\r\n")}`, LICENCE), []);
  assert.match(reportProblems("C:\\t\\licences-1.2.3.md", "# Licences\n\nMS-RL, in part.\n", LICENCE).join(), /licence report C:\\t\\licences-1\.2\.3\.md does not carry WiX's LICENSE\.TXT/);
  assert.match(reportProblems("r.md", LICENCE.slice(0, -40), LICENCE).join(), /r\.md/);

  assert.deepEqual(statusPaths(""), []);
  assert.deepEqual(statusPaths(" M agent/cg-agent/src/main.rs\n?? deploy/windows/extra.props\r\nR  Cargo.toml -> Cargo2.toml\n\n"), [
    "agent/cg-agent/src/main.rs",
    "deploy/windows/extra.props",
    "Cargo.toml -> Cargo2.toml",
  ]);
  assert.deepEqual(cleanTreeProblems(""), []);
  assert.deepEqual(cleanTreeProblems("?? .cargo/config.local.toml\n"), ["the repository is not clean where the build reads it: .cargo/config.local.toml"]);
  assert.deepEqual([...BUILD_INPUTS], ["deploy/windows", "agent", "Cargo.toml", "Cargo.lock", ".cargo", "rust-toolchain.toml"]);
});

function git(cwd, args) {
  const r = spawnSync("git", ["-c", "user.name=cg-test", "-c", "user.email=cg-test@example.invalid", "-c", "commit.gpgsign=false", ...args], { cwd, encoding: "utf8" });
  assert.equal(r.status, 0, `git ${args.join(" ")}: ${r.stderr}`);
  return r.stdout;
}

test("a change, a staged file or an untracked one where the build reads the repository is refused, elsewhere not", () =>
  withTemp((dir) => {
    git(dir, ["init", "-q"]);
    for (const f of ["agent/cg-agent/src/main.rs", "deploy/windows/msi/Package.wxs", "Cargo.toml", "Cargo.lock", ".cargo/config.toml", "rust-toolchain.toml", "README.md"]) {
      mkdirSync(join(dir, dirname(f)), { recursive: true });
      writeFileSync(join(dir, f), `${f}\n`);
    }
    git(dir, ["add", "-A"]);
    git(dir, ["commit", "-q", "-m", "fixture"]);
    assert.deepEqual(repositoryProblems(dir), []);
    // Outside the build's inputs: an output directory at the root, a document.
    mkdirSync(join(dir, "assets"));
    writeFileSync(join(dir, "assets", "cg-agent.exe"), "x");
    writeFileSync(join(dir, "README.md"), "changed\n");
    assert.deepEqual(repositoryProblems(dir), []);
    const cases = [
      ["a changed source", "agent/cg-agent/src/main.rs", "changed\n"],
      ["an untracked file deep under deploy/windows", "deploy/windows/msi/sub/Extra.wxs", "<Wix />\n"],
      ["an untracked cargo configuration", ".cargo/config.local.toml", "[build]\n"],
      ["a changed toolchain", "rust-toolchain.toml", "[toolchain]\n"],
    ];
    for (const [name, file, text] of cases) {
      const path = join(dir, file);
      const before = (() => {
        try {
          return readFileSync(path);
        } catch {
          return undefined;
        }
      })();
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, text);
      assert.deepEqual(repositoryProblems(dir), [`the repository is not clean where the build reads it: ${file}`], name);
      if (before === undefined) rmSync(path);
      else writeFileSync(path, before);
    }
    writeFileSync(join(dir, "Cargo.lock"), "staged\n");
    git(dir, ["add", "Cargo.lock"]);
    assert.match(repositoryProblems(dir).join(), /Cargo\.lock/);
  }));

/** A clean repository of the build's inputs, for the preflight's own git status. */
function cleanRepo(dir) {
  mkdirSync(dir, { recursive: true });
  git(dir, ["init", "-q"]);
  writeFileSync(join(dir, "Cargo.toml"), "[workspace]\n");
  git(dir, ["add", "-A"]);
  git(dir, ["commit", "-q", "-m", "fixture"]);
  return dir;
}

test("the preflight refuses an output or a work directory not empty, a report without WiX's licence, a change in the inputs", () =>
  withTemp((dir) => {
    const repo = cleanRepo(join(dir, "repo"));
    const out = join(dir, "out");
    const work = join(dir, "work");
    const good = join(dir, "good.md");
    const bad = join(dir, "licences-1.2.3.md");
    writeFileSync(good, `# Licences\n\n${LICENCE}`);
    writeFileSync(bad, "# Licences\n");
    // Absent directories are made, and empty ones pass.
    assert.deepEqual(preflightProblems({ out, work, report: good, repo }), []);
    assert.deepEqual(preflightProblems({ out, work, repo }), []);
    writeFileSync(join(out, "leftover.msi"), "x");
    assert.deepEqual(preflightProblems({ out, work, report: good, repo }), [`the output directory ${out} is not empty: leftover.msi`]);
    rmSync(join(out, "leftover.msi"));
    writeFileSync(join(work, "cg-setup.exe"), "x");
    assert.deepEqual(preflightProblems({ out, work, report: good, repo }), [`the work directory ${work} is not empty: cg-setup.exe`]);
    rmSync(join(work, "cg-setup.exe"));
    assert.deepEqual(preflightProblems({ out, work, report: bad, repo }), [
      `the licence report ${bad} does not carry WiX's LICENSE.TXT (deploy/windows/bundle/theme/LICENSE.TXT)`,
    ]);
    writeFileSync(join(repo, "Cargo.toml"), "[workspace]\nmembers = []\n");
    assert.deepEqual(preflightProblems({ out, work, report: good, repo }), ["the repository is not clean where the build reads it: Cargo.toml"]);
  }));

test("build runs the preflight before it builds anything, every problem named", () =>
  withTemp((dir) => {
    // Each guard alone would stop the build, so none of it runs even if one of them did not.
    const out = join(dir, "out");
    const work = join(dir, "work");
    mkdirSync(out);
    mkdirSync(work);
    writeFileSync(join(out, "leftover.msi"), "x");
    writeFileSync(join(work, "cg-setup.exe"), "x");
    const report = join(dir, "licences-1.2.3.md");
    writeFileSync(report, "# Licences\n");
    const refused = (() => {
      try {
        buildRelease({ version: "1.2.3", out, work, report });
      } catch (e) {
        return String(e);
      }
      assert.fail("build did not refuse");
    })();
    assert.match(refused, /the output directory .*out is not empty: leftover\.msi/);
    assert.match(refused, /the work directory .*work is not empty: cg-setup\.exe/);
    assert.match(refused, /the licence report .*licences-1\.2\.3\.md does not carry WiX's LICENSE\.TXT/);
  }));
