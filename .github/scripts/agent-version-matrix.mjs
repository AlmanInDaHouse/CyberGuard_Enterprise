// SPEC-024 install_ac_014 (debt #131): the agent's version is fixed at build time by the
// build-only input CG_RELEASE_VERSION. This runs, in one target directory: a build given a
// malformed or empty input, which must fail; then the version tests without the input, with
// a fixture version unlike Cargo's, with that fixture changed, and without the input again,
// so a value a rebuild left stale fails them. Each step is one cargo invocation: the input it
// sets reaches the build and, at run time, the tests' expected value
// (agent/cg-agent/tests/common/mod.rs, expected_agent_version). rust-ci runs it on Linux and
// on Windows. From the repository root:
//   node .github/scripts/agent-version-matrix.mjs
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const TESTS = ["--test", "install_ac_014_version", "--test", "enroll_ac_003_request_contents"];
const MALFORMED = ["", "v7.8.9", "7.8", "07.8.9"];
const FIXTURES = ["7.8.9", "7.8.10"];
const BUILD_ERROR = "CG_RELEASE_VERSION must be X.Y.Z";

const manifest = readFileSync("agent/cg-agent/Cargo.toml", "utf8");
const cargoVersion = /^\[package\][^[]*?^version = "([^"]+)"/ms.exec(manifest)?.[1];
if (cargoVersion === undefined) {
  throw new Error("no [package] version in agent/cg-agent/Cargo.toml");
}
for (const v of FIXTURES) {
  if (v === cargoVersion) {
    throw new Error(`fixture ${v} equals Cargo's version; install_ac_014 needs one unlike it`);
  }
}

/** Run cargo with the input set to `value`, or absent when `value` is undefined. */
function cargo(args, value, capture) {
  const env = { ...process.env };
  delete env.CG_RELEASE_VERSION;
  if (value !== undefined) {
    env.CG_RELEASE_VERSION = value;
  }
  const r = spawnSync("cargo", args, {
    env,
    encoding: "utf8",
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
  if (r.error) {
    throw r.error;
  }
  return r;
}

let failed = 0;
for (const value of MALFORMED) {
  const r = cargo(["build", "--locked", "-p", "cg-agent", "--lib"], value, true);
  const refused = r.status !== 0 && r.stderr.includes(BUILD_ERROR);
  console.log(
    `${refused ? "PASS" : "FAIL"} build with CG_RELEASE_VERSION=${JSON.stringify(value)} ${refused ? "refused" : `exit ${r.status}`}`,
  );
  if (!refused) {
    failed += 1;
    process.stderr.write(r.stderr);
  }
}

for (const value of [undefined, ...FIXTURES, undefined]) {
  const label = value === undefined ? "without the input" : `CG_RELEASE_VERSION=${value}`;
  console.log(`== version tests ${label}`);
  const r = cargo(["test", "--locked", "--no-fail-fast", "-p", "cg-agent", ...TESTS], value, false);
  console.log(`${r.status === 0 ? "PASS" : "FAIL"} version tests ${label}`);
  if (r.status !== 0) {
    failed += 1;
  }
}

if (failed > 0) {
  console.error(`agent version matrix: ${failed} step(s) failed`);
  process.exit(1);
}
console.log("agent version matrix: all steps passed");
