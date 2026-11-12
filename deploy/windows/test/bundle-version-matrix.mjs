// SPEC-024 §Data contracts §1 and its Amendment 2026-11-12 (second), point 1: the setup is built
// only at a release version, carrying the MSI and cg-setup.exe of that version. This builds the
// setup project once per case and checks that each refusal is the project's own
// (deploy/windows/Directory.Build.targets) and that the setup builds from packages of its version.
// It builds the MSIs it needs from the agent's executable, at the version cg-setup.exe was built
// with and at another. It needs the pins restored and verified (README.md); package-ci runs it.
// Building installs nothing. From the repository root:
//   node deploy/windows/test/bundle-version-matrix.mjs <cg-agent.exe> <cg-setup.exe> <version>
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const [agentArg, setupArg, version] = process.argv.slice(2);
const agentExe = agentArg ? resolve(agentArg) : undefined;
const setupExe = setupArg ? resolve(setupArg) : undefined;
if (!agentExe || !existsSync(agentExe) || !setupExe || !existsSync(setupExe) || !/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("usage: node deploy/windows/test/bundle-version-matrix.mjs <cg-agent.exe> <cg-setup.exe> <version>");
  process.exit(2);
}
const [major, minor, patch] = version.split(".").map(Number);
const other = `${major}.${minor}.${patch + 1}`;

const CWD = resolve("deploy/windows");
const scratch = mkdtempSync(join(tmpdir(), "cg-bundle-"));

function build(project, release, properties, extra = []) {
  const env = { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1" };
  delete env.CG_RELEASE_VERSION;
  if (release !== undefined) env.CG_RELEASE_VERSION = release;
  const out = mkdtempSync(join(scratch, "out-"));
  const r = spawnSync(
    "dotnet",
    ["build", "--no-incremental", project, "-c", "Release", ...properties, `-p:OutputPath=${out}\\`, ...extra],
    { env, encoding: "utf8", cwd: CWD },
  );
  return { status: r.status, output: `${r.stdout ?? ""}${r.stderr ?? ""}`, out };
}

// The MSIs, at the setup's version and at another.
const msis = {};
for (const v of [version, other]) {
  const r = build("msi/CyberGuardAgent.wixproj", v, [`-p:AgentExe=${agentExe}`]);
  msis[v] = join(r.out, `cyberguard-agent-${v}-x64.msi`);
  if (r.status !== 0 || !existsSync(msis[v])) {
    console.log(r.output);
    console.error(`the MSI at ${v} did not build`);
    process.exit(1);
  }
}

const NOT_SET = "CG_RELEASE_VERSION is not set";
const REFUSED_FORM = "release version:";
const OVERRIDDEN = "is not CG_RELEASE_VERSION";
const NO_MSI = "Msi names no file";
const NO_SETUP = "SetupExe names no file";
const MSI_VERSION = `Msi is version ${other}`;
const SETUP_VERSION = `SetupExe is version ${version}`;
const absent = join(tmpdir(), "absent-cg-package.bin");

// [label, CG_RELEASE_VERSION (undefined: absent), Msi, SetupExe, the refusal's text or null to
//  build, further MSBuild arguments]
const CASES = [
  ["without the input", undefined, msis[version], setupExe, NOT_SET],
  ["an empty input", "", msis[version], setupExe, NOT_SET],
  ["a v prefix", `v${version}`, msis[version], setupExe, REFUSED_FORM],
  ["a quote", `${version}"`, msis[version], setupExe, REFUSED_FORM],
  ["a ProductVersion set otherwise", version, msis[version], setupExe, OVERRIDDEN, ["-p:ProductVersion=9.9.9"]],
  ["no MSI", version, absent, setupExe, NO_MSI],
  ["no cg-setup.exe", version, msis[version], absent, NO_SETUP],
  ["an MSI of another version", version, msis[other], setupExe, MSI_VERSION],
  ["a cg-setup.exe of another version", other, msis[other], setupExe, SETUP_VERSION],
  ["the packages of its version", version, msis[version], setupExe, null],
];

let failed = 0;
for (const [label, release, msi, setup, refusal, extra = []] of CASES) {
  const r = build("bundle/CyberGuardSetup.wixproj", release, [`-p:Msi=${msi}`, `-p:SetupExe=${setup}`], extra);
  const built = existsSync(join(r.out, `cyberguard-agent-${release}-x64-setup.exe`));
  let ok;
  if (refusal === null) {
    ok = r.status === 0 && built;
    console.log(`${ok ? "PASS" : "FAIL"} ${label} (${release}) builds: exit ${r.status}`);
  } else {
    ok = r.status !== 0 && r.output.includes(refusal) && !built;
    console.log(`${ok ? "PASS" : "FAIL"} ${label} (${JSON.stringify(release)}) refused: exit ${r.status}`);
  }
  if (!ok) {
    failed += 1;
    console.log(r.output.split(/\r?\n/).filter((line) => /error|refus|release version|version/i.test(line)).join("\n"));
  }
}
rmSync(scratch, { recursive: true, force: true });
console.log(failed === 0 ? "bundle version matrix: all cases passed" : `bundle version matrix: ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
