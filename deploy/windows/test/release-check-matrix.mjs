// SPEC-024 Amendment 2026-11-14, points 2 and 6, with its *Tests*: the release's check passes the
// assets of one release and refuses each wrong set, naming the check that refuses it. This builds
// the MSI and the setup at <version> from the executables given, and the wrong cases a build makes:
// a test-build MSI (a custom action), a setup around a second build of the MSI, an MSI around
// another executable, a setup of another version (around a copy of cg-setup.exe whose version
// strings were changed for the test), and, when one is given, a development executable. Then it
// runs `.github/scripts/release-windows.mjs check` on each set. It needs the pins restored and
// verified (README.md); building installs nothing, and the check runs neither the MSI nor the
// setup. From the repository root:
//   node deploy/windows/test/release-check-matrix.mjs <cg-agent.exe> <cg-setup.exe> <version> [<development cg-agent.exe>]
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const SCRIPT = resolve(".github/scripts/release-windows.mjs");
const { CHECKS, readResources } = await import(pathToFileURL(SCRIPT).href);
if (CHECKS.length !== 14) {
  console.error(`the script names ${CHECKS.length} checks, not point 2's 14`);
  process.exit(1);
}

const [agentArg, setupArg, version, devArg] = process.argv.slice(2);
const agentExe = agentArg ? resolve(agentArg) : undefined;
const setupExe = setupArg ? resolve(setupArg) : undefined;
const devExe = devArg ? resolve(devArg) : undefined;
if (
  !agentExe ||
  !existsSync(agentExe) ||
  !setupExe ||
  !existsSync(setupExe) ||
  !/^\d+\.\d+\.\d+$/.test(version ?? "") ||
  (devExe !== undefined && !existsSync(devExe))
) {
  console.error("usage: node deploy/windows/test/release-check-matrix.mjs <cg-agent.exe> <cg-setup.exe> <version> [<development cg-agent.exe>]");
  process.exit(2);
}
// Another version of the same length, so that a version string can be changed in place.
const [major, minor, patch] = version.split(".").map(Number);
const otherPatch = String(patch + 1).length === String(patch).length ? patch + 1 : patch - 1;
const other = `${major}.${minor}.${otherPatch}`;

const CWD = resolve("deploy/windows");
const scratch = mkdtempSync(join(tmpdir(), "cg-release-matrix-"));

function build(project, release, properties, extra = []) {
  const env = { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1" };
  env.CG_RELEASE_VERSION = release;
  const out = mkdtempSync(join(scratch, "out-"));
  const r = spawnSync(
    "dotnet",
    ["build", "--no-incremental", project, "-c", "Release", ...properties, `-p:OutputPath=${out}\\`, ...extra],
    { env, encoding: "utf8", cwd: CWD },
  );
  const file = join(out, project.startsWith("msi/") ? `cyberguard-agent-${release}-x64.msi` : `cyberguard-agent-${release}-x64-setup.exe`);
  if (r.status !== 0 || !existsSync(file)) {
    console.log(`${r.stdout ?? ""}${r.stderr ?? ""}`.split(/\r?\n/).filter((l) => /error/i.test(l)).join("\n"));
    console.error(`${project} at ${release} (${properties.join(" ")} ${extra.join(" ")}) did not build`);
    rmSync(scratch, { recursive: true, force: true });
    process.exit(1);
  }
  return file;
}
const msi = (release, exe, extra = []) => build("msi/CyberGuardAgent.wixproj", release, [`-p:AgentExe=${exe}`], extra);
const setup = (release, msiFile, setupFile) => build("bundle/CyberGuardSetup.wixproj", release, [`-p:Msi=${msiFile}`, `-p:SetupExe=${setupFile}`]);

/** A copy of cg-setup.exe whose FileVersion and ProductVersion strings read `to`: a file changed for the test. */
function setupExeAt(to) {
  const buf = Buffer.from(readFileSync(setupExe));
  const [r] = readResources(buf).filter((x) => x.type === 16);
  const region = buf.subarray(r.offset, r.offset + r.size);
  for (const key of ["FileVersion", "ProductVersion"]) {
    const at = region.indexOf(Buffer.from(`${key}\0`, "utf16le"));
    const value = at === -1 ? -1 : region.indexOf(Buffer.from(version, "utf16le"), at);
    if (value === -1) {
      console.error(`cg-setup.exe's ${key} does not read ${version}`);
      process.exit(1);
    }
    Buffer.from(to, "utf16le").copy(region, value);
  }
  const file = join(mkdtempSync(join(scratch, "exe-")), "cg-setup.exe");
  writeFileSync(file, buf);
  return file;
}

console.log(`building the MSIs and setups at ${version} (and ${other})`);
const right = msi(version, agentExe);
const rightSetup = setup(version, right, setupExe);
const testBuild = msi(version, agentExe, ["-p:CgInjectFailure=1"]);
const secondBuild = msi(version, agentExe);
const aroundOther = msi(version, devExe ?? setupExe);
const otherMsi = msi(other, agentExe);

// [label, --version, --exe, --msi, --setup, --setup-exe, the checks that must fail (none: all pass)]
const CASES = [
  ["the release's assets", version, agentExe, right, rightSetup, setupExe, []],
  ["a test-build MSI (a custom action)", version, agentExe, testBuild, setup(version, testBuild, setupExe), setupExe, ["msi custom actions"]],
  ["a setup around a second build of the MSI", version, agentExe, right, setup(version, secondBuild, setupExe), setupExe, ["setup msi payload"]],
  [
    `an MSI around another executable (${devExe ? "a development build" : "cg-setup.exe"})`,
    version,
    agentExe,
    aroundOther,
    setup(version, aroundOther, setupExe),
    setupExe,
    ["msi agent payload"],
  ],
  [
    `a setup of another version (${other})`,
    version,
    agentExe,
    right,
    setup(other, otherMsi, setupExeAt(other)),
    setupExe,
    ["setup version resource", "setup registered version", "setup msi payload", "setup cg-setup.exe payloads"],
  ],
  [
    `assets of ${version} checked as ${other}`,
    other,
    agentExe,
    right,
    rightSetup,
    setupExe,
    ["agent --version", "agent version resource", "cg-setup.exe version resource", "msi product version", "setup version resource", "setup registered version"],
  ],
  ...(devExe ? [["a development executable", version, devExe, right, rightSetup, setupExe, ["agent --version", "agent version resource", "msi agent payload"]]] : []),
];

// A FAIL line that must also say why: the development executable's version resource fails for its
// pre-release flag (point 8), not for its version alone.
const REASONS = { "a development executable": { "agent version resource": /pre-release/ } };

let failed = 0;
for (const [label, v, exe, msiFile, setupFile, setupExeFile, expected] of CASES) {
  const r = spawnSync(process.execPath, [SCRIPT, "check", "--version", v, "--exe", exe, "--msi", msiFile, "--setup", setupFile, "--setup-exe", setupExeFile], {
    encoding: "utf8",
  });
  const lines = (r.stdout ?? "").split(/\r?\n/).filter((l) => /^(PASS|FAIL) /.test(l));
  // Each of the 14 checks once, in CHECKS's order, PASS unless it is one the case expects to fail.
  const want = CHECKS.map((c) => `${expected.includes(c) ? "FAIL" : "PASS"} ${c}`);
  const got = lines.map((l) => /^((?:PASS|FAIL) [^:]+)/.exec(l)[1]);
  const problems = expected.filter((c) => !CHECKS.includes(c)).map((c) => `${c} is not a check`);
  if (got.join("\n") !== want.join("\n")) problems.push(`its lines read ${JSON.stringify(got)}, not ${JSON.stringify(want)}`);
  if (r.status !== (expected.length === 0 ? 0 : 1)) problems.push(`exit ${r.status}`);
  for (const [name, reason] of Object.entries(REASONS[label] ?? {})) {
    const line = lines.find((l) => l.startsWith(`FAIL ${name}:`));
    if (!line || !reason.test(line)) problems.push(`its ${name} line does not match ${reason}: ${line ?? "(none)"}`);
  }
  const ok = problems.length === 0;
  console.log(
    expected.length === 0
      ? `${ok ? "PASS" : "FAIL"} ${label} passes all ${CHECKS.length} checks: exit ${r.status}`
      : `${ok ? "PASS" : "FAIL"} ${label} refused by exactly ${expected.join(", ")}: exit ${r.status}`,
  );
  for (const p of problems) console.log(`  ${p}`);
  if (!ok) {
    failed += 1;
    console.log(`${r.stdout ?? ""}${r.stderr ?? ""}`.split(/\r?\n/).filter((l) => l.startsWith("FAIL ") || /error/i.test(l)).map((l) => `  ${l}`).join("\n"));
  }
}
rmSync(scratch, { recursive: true, force: true });
console.log(failed === 0 ? "release check matrix: all cases passed" : `release check matrix: ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
