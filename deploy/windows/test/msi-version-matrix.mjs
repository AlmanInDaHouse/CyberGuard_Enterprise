// SPEC-024 §Data contracts §1: packaging rejects a version outside MSI's limits rather than
// truncating it or inventing one, and a development build is not a release package. This builds
// the MSI project once per case and checks that each refusal is the project's own
// (deploy/windows/Directory.Build.targets, which runs .github/scripts/release-version.mjs) and
// that a version at MSI's limits builds. It needs the pins restored and verified (README.md) and
// the agent's executable; package-ci runs it. From the repository root:
//   node deploy/windows/test/msi-version-matrix.mjs <cg-agent.exe>
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const PROJECT = resolve("deploy/windows/msi/CyberGuardAgent.wixproj");
const agentExe = process.argv[2] ? resolve(process.argv[2]) : undefined;
if (!agentExe || !existsSync(agentExe)) {
  console.error("usage: node deploy/windows/test/msi-version-matrix.mjs <cg-agent.exe>");
  process.exit(2);
}

const NOT_SET = "CG_RELEASE_VERSION is not set";
const REFUSED_FORM = "release version:";
const NO_AGENT = "AgentExe names no file";
const OVERRIDDEN = "is not CG_RELEASE_VERSION";

// [label, CG_RELEASE_VERSION (undefined: absent), AgentExe, the refusal's text or null to build,
//  further MSBuild arguments]
const CASES = [
  ["without the input", undefined, agentExe, NOT_SET],
  ["an empty input", "", agentExe, NOT_SET],
  ["two components", "1.2", agentExe, REFUSED_FORM],
  ["four components", "1.2.3.4", agentExe, REFUSED_FORM],
  ["a v prefix", "v1.2.3", agentExe, REFUSED_FORM],
  ["a leading zero", "01.2.3", agentExe, REFUSED_FORM],
  ["a pre-release suffix", "1.2.3-dev", agentExe, REFUSED_FORM],
  ["a quote", '1.2.3"', agentExe, REFUSED_FORM],
  ["a ProductVersion set otherwise", "1.2.3", agentExe, OVERRIDDEN, ["-p:ProductVersion=9.9.9"]],
  ["major above 255", "256.0.0", agentExe, REFUSED_FORM],
  ["minor above 255", "0.256.0", agentExe, REFUSED_FORM],
  ["patch above 65535", "0.0.65536", agentExe, REFUSED_FORM],
  ["no agent executable", "1.2.3", join(tmpdir(), "absent-cg-agent.exe"), NO_AGENT],
  ["MSI's limits", "255.255.65535", agentExe, null],
];

let failed = 0;
for (const [label, version, exe, refusal, extra = []] of CASES) {
  const out = mkdtempSync(join(tmpdir(), "cg-msi-"));
  const env = { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1" };
  delete env.CG_RELEASE_VERSION;
  if (version !== undefined) env.CG_RELEASE_VERSION = version;
  const r = spawnSync(
    "dotnet",
    ["build", "--no-incremental", PROJECT, "-c", "Release", `-p:AgentExe=${exe}`, `-p:OutputPath=${out}\\`, ...extra],
    { env, encoding: "utf8", cwd: resolve("deploy/windows") },
  );
  const output = `${r.stdout ?? ""}${r.stderr ?? ""}`;
  let ok;
  if (refusal === null) {
    ok = r.status === 0 && existsSync(join(out, `cyberguard-agent-${version}-x64.msi`));
    console.log(`${ok ? "PASS" : "FAIL"} ${label} (${version}) builds: exit ${r.status}`);
  } else {
    ok = r.status !== 0 && output.includes(refusal);
    console.log(`${ok ? "PASS" : "FAIL"} ${label} (${JSON.stringify(version)}) refused: exit ${r.status}`);
  }
  if (!ok) {
    failed += 1;
    console.log(output.split(/\r?\n/).filter((line) => /error|refus|release version/i.test(line)).join("\n"));
  }
  rmSync(out, { recursive: true, force: true });
}
console.log(failed === 0 ? "msi version matrix: all cases passed" : `msi version matrix: ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
