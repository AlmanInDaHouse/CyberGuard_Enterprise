// SPEC-024 Amendment 2026-11-14, points 1, 5, 6 and 7: what release.yml itself must hold, read
// from its text. Its six payload assets in SHA256SUMS and in the publish step; each job's
// permissions; each job that pushes or attests waiting for the licence check, directly or through
// another job; no step passing the MSI's test-build define, using an action outside the list it
// may use or one not pinned by its commit, or naming a cache, and Rust installed through rustup
// before the build; the Windows assets made by the script package-ci's dry run runs, into the same
// relative directory, by a step that cannot mint an OIDC token, with the licence report fetched
// outside that directory, and attested after it; the release's notes naming its assets; and the
// runners' images, pinned and the same as the dry runs'. Each check of a workflow's text is also
// seen refusing a copy of release.yml changed for it.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { releaseSteps } from "../../../.github/scripts/release-windows.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const workflow = (name) => readFileSync(join(REPO, ".github", "workflows", name), "utf8");

/** A workflow's jobs: each one's `needs`, `runs-on`, `permissions` and lines. */
function jobs(text) {
  const lines = text.split("\n");
  const start = lines.indexOf("jobs:");
  assert.ok(start >= 0, "the workflow has a jobs: block");
  const found = new Map();
  let current = null;
  let inPermissions = false;
  for (const line of lines.slice(start + 1)) {
    const job = /^ {2}([A-Za-z_][A-Za-z0-9_-]*):\s*$/.exec(line);
    if (job) {
      current = { needs: [], runsOn: null, permissions: [], text: [] };
      found.set(job[1], current);
      inPermissions = false;
      continue;
    }
    if (!current) continue;
    current.text.push(line);
    const needs = /^ {4}needs:\s*(.+)$/.exec(line);
    if (needs) current.needs = needs[1].replace(/[[\]]/g, "").split(",").map((n) => n.trim()).filter(Boolean);
    const runsOn = /^ {4}runs-on:\s*(\S+)\s*$/.exec(line);
    if (runsOn) current.runsOn = runsOn[1];
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

const release = workflow("release.yml");
const releaseJobs = jobs(release);

test("each job of release.yml has the permissions it uses alone", () => {
  assert.match(release, /^permissions: \{\}$/m, "no permission at the workflow's level");
  const expected = {
    version: ["contents: read"],
    licences: ["contents: read"],
    images: ["contents: read", "packages: write", "id-token: write", "attestations: write"],
    agent: ["contents: read", "id-token: write", "attestations: write"],
    bundle: ["contents: read", "id-token: write", "attestations: write"],
    publish: ["contents: write"],
  };
  assert.deepEqual([...releaseJobs.keys()].sort(), Object.keys(expected).sort());
  for (const [name, permissions] of Object.entries(expected)) {
    assert.deepEqual(releaseJobs.get(name).permissions.sort(), [...permissions].sort(), name);
  }
});

test("each job that pushes, attests or publishes waits for the licence check", () => {
  for (const [name, job] of releaseJobs) {
    const acts = job.permissions.some((p) => /^(packages|id-token|attestations): write$|^contents: write$/.test(p));
    if (acts) assert.ok(dependsOn(releaseJobs, name, "licences"), `${name} must wait for the licence check`);
  }
});

test("the agent job makes the Windows assets with the dry run's script, then attests and keeps them", () => {
  const steps = releaseJobs.get("agent").text.join("\n");
  const build = steps.indexOf("node .github/scripts/release-windows.mjs build ");
  const attest = steps.indexOf("uses: actions/attest-build-provenance@");
  const keep = steps.indexOf("uses: actions/upload-artifact@");
  assert.ok(build > 0, "the agent job runs the release script's build");
  assert.match(steps, /--report "\$RUNNER_TEMP\/licences\/licences-\$V\.md"/, "with the licence report");
  assert.ok(attest > build && keep > attest, "it attests after the build, and keeps after the attestation");
  assert.match(steps, /subject-path: assets\/\*/);
  assert.match(steps, /name: licences\n/, "it fetches the licence job's report");
  // The dry run runs the same command.
  assert.match(workflow("package-ci.yml"), /node \.github\/scripts\/release-windows\.mjs build --version "\$CG_FIXTURE_VERSION"/);
});

test("the release publishes six payload assets, each in SHA256SUMS, and SHA256SUMS", () => {
  const publish = releaseJobs.get("publish").text.join("\n");
  for (const asset of [
    '"cyberguard-deploy-$V.tar.gz"',
    '"cg-agent-$V-x86_64-pc-windows-msvc.exe"',
    '"cyberguard-agent-$V-x64-setup.exe"',
    '"cyberguard-agent-$V-x64.msi"',
    '"licences-$V.md"',
    '"${SOURCES[0]}"',
  ]) {
    assert.ok(publish.includes(asset), asset);
  }
  assert.match(publish, /SOURCES=\(wix-\*-source\.tar\.gz\)\n\s+test "\$\{#SOURCES\[@\]\}" -eq 1/, "one source archive");
  assert.match(publish, /sha256sum "\$\{ASSETS\[@\]\}" > SHA256SUMS/);
  assert.match(publish, /"\$\{ASSETS\[@\]\}" SHA256SUMS\s*$/);
  // Six names in the ASSETS array.
  const array = /ASSETS=\(([\s\S]*?)\)\n/.exec(publish)?.[1] ?? "";
  assert.equal(array.match(/"[^"]+"/g)?.length, 6, array);
});

/** The actions a release job may use, each pinned by its commit's SHA. */
const ALLOWED_ACTIONS = Object.freeze([
  "actions/checkout",
  "actions/setup-node",
  "actions/setup-dotnet",
  "actions/download-artifact",
  "actions/upload-artifact",
  "actions/attest-build-provenance",
]);

/** Each `uses:` outside ALLOWED_ACTIONS, or not pinned by a 40-digit commit SHA. */
function actionProblems(text) {
  const out = [];
  for (const m of text.matchAll(/^\s*(?:-\s+)?uses:\s*(\S+)/gm)) {
    const [name, ref] = m[1].split("@");
    if (!ALLOWED_ACTIONS.includes(name)) out.push(`${m[1]} is not an action the release may use`);
    else if (!/^[0-9a-f]{40}$/.test(ref ?? "")) out.push(`${m[1]} is not pinned by a commit SHA`);
  }
  return out;
}

/** Each line whose key names a cache, but `package-manager-cache: false`. */
function cacheProblems(text) {
  return text
    .split("\n")
    .filter((line) => {
      const key = /^\s*(?:-\s+)?([A-Za-z0-9_.-]+)\s*:/.exec(line)?.[1];
      return key !== undefined && /cache/i.test(key) && line.trim() !== "package-manager-cache: false";
    })
    .map((line) => `${line.trim()} names a cache`);
}

/** Rust comes from rust-toolchain.toml through rustup, before the release build runs. */
function rustupProblems(jobText) {
  const rustup = jobText.indexOf("rustup toolchain install");
  const build = jobText.indexOf("node .github/scripts/release-windows.mjs build ");
  if (build === -1) return ["the job does not run the release build"];
  return rustup !== -1 && rustup < build ? [] : ["rustup toolchain install does not run before the release build"];
}

/** A job's steps, `{ name, text }`, each from its `- name:` line to the next one. */
function steps(jobText) {
  const out = [];
  for (const line of jobText.split("\n")) {
    const named = /^ {6}- name:\s*(.+)$/.exec(line);
    if (named) out.push({ name: named[1].trim(), lines: [] });
    out.at(-1)?.lines.push(line);
  }
  return out.map((s) => ({ name: s.name, text: s.lines.join("\n") }));
}

/** A step's own `env:` (its keys at two spaces past it), as written: { NAME: 'value as text' }. */
function stepEnv(stepText) {
  const lines = stepText.split("\n");
  const at = lines.findIndex((l) => /^ {8}env:\s*$/.test(l));
  const env = {};
  if (at === -1) return env;
  for (const line of lines.slice(at + 1)) {
    const entry = /^ {10}([A-Za-z_][A-Za-z0-9_]*):\s*(.*)$/.exec(line);
    if (!entry) break;
    env[entry[1]] = entry[2];
  }
  return env;
}

/** The step that runs the release build may not mint an OIDC token: both request variables blank in its env. */
function oidcProblems(stepText) {
  const env = stepEnv(stepText);
  return ["ACTIONS_ID_TOKEN_REQUEST_URL", "ACTIONS_ID_TOKEN_REQUEST_TOKEN"].filter((v) => env[v] !== '""').map((v) => `${v} is not set blank: ${env[v] ?? "(unset)"}`);
}

/** The download step of the licence report, whose path must be outside assets/, which the attestation covers. */
function licencesPathProblems(jobText) {
  const download = steps(jobText).find((s) => /uses: actions\/download-artifact@/.test(s.text) && /^ {10}name: licences\s*$/m.test(s.text));
  if (!download) return ["no step downloads the licence report"];
  const path = /^ {10}path:\s*(.+)$/m.exec(download.text)?.[1].trim();
  return path === "${{ runner.temp }}/licences" ? [] : [`the licence report is downloaded to ${path ?? "(no path)"}, not \${{ runner.temp }}/licences`];
}

const packageRelease = jobs(workflow("package-ci.yml")).get("release").text.join("\n");
const agentJob = releaseJobs.get("agent").text.join("\n");

test("no release step passes the MSI's test-build define, uses another action, or restores a cache", () => {
  for (const [name, text] of [
    ["release.yml", release],
    ["package-ci.yml's release job", packageRelease],
  ]) {
    assert.ok(!/CgInjectFailure/.test(text), `${name} names the test build's define`);
    assert.deepEqual(actionProblems(text), [], name);
    assert.deepEqual(cacheProblems(text), [], name);
    const nodes = text.match(/uses: actions\/setup-node@/g)?.length ?? 0;
    const off = text.match(/package-manager-cache: false/g)?.length ?? 0;
    assert.equal(off, nodes, `${name}: each Node set up with its cache off`);
  }
  assert.deepEqual(rustupProblems(agentJob), [], "release.yml's agent job");
  assert.deepEqual(rustupProblems(packageRelease), [], "package-ci.yml's release job");
  // The build passes the define empty, which an environment variable of that name cannot override,
  // and checks the pins and the EULA ID before WiX builds anything (release-build.test.mjs holds the rest).
  const plan = releaseSteps({ version: "1.2.3", out: "assets", work: "work", wixRelease: "7.0.1" });
  const msi = plan.steps.find((s) => s.name === "the MSI");
  assert.ok(msi.args.includes("-p:CgInjectFailure="));
  assert.ok(plan.steps.findIndex((s) => s.args?.includes("--tool")) < plan.steps.indexOf(msi));
  assert.ok(!/CgInjectFailure=1/.test(readFileSync(join(REPO, ".github", "scripts", "release-windows.mjs"), "utf8")));
});

test("the checks of the release's actions, caches and Rust refuse a workflow changed for the test", () => {
  const swap = (from, to) => {
    assert.ok(release.includes(from), from);
    return release.replace(from, to);
  };
  const checkout = "uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0";
  const rustCache = swap(checkout, `uses: Swatinem/rust-cache@9d47c6ad4b02e050fd481d890b2ea34778fd09d6 # v2.7.8\n      - name: Checkout\n        ${checkout}`);
  assert.match(actionProblems(rustCache).join(), /Swatinem\/rust-cache@.* is not an action the release may use/);
  const restore = swap(checkout, "uses: actions/cache/restore@5a3ec84eff668545956fd18022155c47e93e2684 # v4.2.3");
  assert.match(actionProblems(restore).join(), /actions\/cache\/restore@/);
  assert.match(actionProblems(swap(checkout, "uses: actions/checkout@v5")).join(), /actions\/checkout@v5 is not pinned/);
  const pnpmCache = swap("          package-manager-cache: false\n", "          cache: pnpm\n");
  assert.match(cacheProblems(pnpmCache).join(), /cache: pnpm names a cache/);
  assert.match(cacheProblems(swap("          package-manager-cache: false\n", "          package-manager-cache: true\n")).join(), /package-manager-cache: true/);
  assert.match(cacheProblems(swap("          global-json-file: deploy/windows/global.json\n", "          global-json-file: deploy/windows/global.json\n          cache: true\n")).join(), /cache: true/);
  const noRustup = jobs(swap("          rustup toolchain install\n          node .github/scripts/release-windows.mjs build", "          node .github/scripts/release-windows.mjs build"));
  assert.match(rustupProblems(noRustup.get("agent").text.join("\n")).join(), /rustup toolchain install does not run/);
});

test("the release build cannot mint an OIDC token, and the licence report is fetched outside assets/", () => {
  const all = steps(agentJob);
  const build = all.find((s) => s.name === "Build, check and archive the Windows assets");
  assert.ok(build, "the agent job's build step");
  assert.deepEqual(oidcProblems(build.text), []);
  assert.match(build.text, /node \.github\/scripts\/release-windows\.mjs build /);
  // The attestation keeps the job's permission: no other step blanks the variables.
  const attest = all.find((s) => /uses: actions\/attest-build-provenance@/.test(s.text));
  assert.ok(attest && all.indexOf(attest) > all.indexOf(build));
  for (const s of all.filter((x) => x !== build)) assert.deepEqual(stepEnv(s.text), {}, s.name);
  assert.deepEqual(licencesPathProblems(agentJob), []);
  // The same checks, on the step and the job changed for the test.
  assert.equal(oidcProblems(build.text.replace(/^ {8}env:\n(?: {10}.*\n)+/m, "")).length, 2);
  assert.match(oidcProblems(build.text.replace('ACTIONS_ID_TOKEN_REQUEST_TOKEN: ""', "ACTIONS_ID_TOKEN_REQUEST_TOKEN: x")).join(), /TOKEN is not set blank: x/);
  assert.match(licencesPathProblems(agentJob.replace("path: ${{ runner.temp }}/licences", "path: assets/licences")).join(), /downloaded to assets\/licences/);
});

test("the release's notes name its six assets and SHA256SUMS, and the dry run builds into the same relative directory", () => {
  const publish = releaseJobs.get("publish").text.join("\n");
  const notes = /--notes "([^"\\]*(?:\\.[^"\\]*)*)"/.exec(publish)?.[1] ?? "";
  for (const asset of ["cyberguard-deploy-$V.tar.gz", "cg-agent-$V-x86_64-pc-windows-msvc.exe", "cyberguard-agent-$V-x64-setup.exe", "cyberguard-agent-$V-x64.msi", "licences-$V.md", "${SOURCES[0]}", "SHA256SUMS"]) {
    assert.ok(notes.includes(asset), `the notes name ${asset}: ${notes}`);
  }
  for (const [name, text] of [
    ["release.yml", agentJob],
    ["package-ci.yml", packageRelease],
  ]) {
    assert.match(text, /node \.github\/scripts\/release-windows\.mjs build --version "\$[A-Z_]+" --out assets /, name);
  }
});

test("the release's runners are pinned images, the same as its dry runs'", () => {
  for (const [name, job] of releaseJobs) {
    assert.equal(job.runsOn, name === "agent" ? "windows-2025" : "ubuntu-24.04", name);
  }
  assert.equal(jobs(workflow("package-ci.yml")).get("release").runsOn, "windows-2025");
  const deploy = jobs(workflow("deploy-ci.yml"));
  assert.equal(deploy.get("release-dry-run").runsOn, "ubuntu-24.04");
  assert.equal(deploy.get("images").runsOn, "ubuntu-24.04");
});

test("package-ci runs on a change to release.yml or to a script the release runs", () => {
  const text = workflow("package-ci.yml");
  const on = text.slice(0, text.indexOf("\njobs:"));
  assert.equal(on.match(/- "\.github\/workflows\/release\.yml"/g)?.length, 2, "push and pull_request");
  assert.equal(on.match(/- "\.github\/scripts\/\*\*"/g)?.length, 2, "push and pull_request");
});
