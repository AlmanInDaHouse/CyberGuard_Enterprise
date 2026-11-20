// SPEC-023 deploy_ac_021: execute the server's actual runbook blocks. Fixtures supply
// release files, images and operator input; no command is replaced by a success stub.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomBytes, X509Certificate } from "node:crypto";
import { cpSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { IS_ROOT, REPO, copyBundle, freePort, makeDashboardTls, ok, run, setEnv, strongPassword, totp } from "./lib.mjs";

const FILE = join(REPO, "docs/operations/install.md");
const IDS = ["versions", "verify", "unpack", "setup-template", "routes", "setup-secrets", "start", "diagnostics", "addresses", "forensic-key", "admin", "ca", "token", "heartbeat-logs", "later-user", "confirm"];
// The endpoint's PowerShell blocks, in the document's order: they run on Windows alone
// (SPEC-024 install_ac_016), and a test below reads what SPEC-024 asks of them.
const ENDPOINT = ["elevation", "directory", "copy", "checksum", "bootstrap", "setup", "cleanup", "wizard", "log",
  "binary", "service", "events", "agent-log", "audit", "dumps", "removal-setup", "uninstall", "purge", "purge-check", "purge-events"];
// The removal's blocks (SPEC-024 Amendments 2026-11-15 and 2026-11-16), under their own section.
const REMOVAL = ["removal-setup", "uninstall", "purge", "purge-check", "purge-events"];
// Explicitly classify every other fence. A new text or PowerShell fence is not
// silently treated as output or as an endpoint command: its classification needs review.
const NON_SERVER = new Map([
  ["output-setup", "text"], ["config-hostnames", "dotenv"], ["config-network", "dotenv"],
  ["output-secrets", "text"], ["config-daemon", "json"], ["output-fingerprint", "text"],
  ...ENDPOINT.map((name) => [`endpoint-${name}`, "powershell"]),
]);

function blocks(text) {
  const seen = new Set();
  for (const fence of text.matchAll(/^([ \t]*)```([^\r\n]*)\r?\n[\s\S]*?^\1```/gm)) {
    const prefix = text.slice(0, fence.index);
    const id = /<!-- deploy_ac_021: ([a-z-]+) -->\r?\n\r?\n$/.exec(prefix)?.[1];
    assert.ok(id && (IDS.includes(id) || NON_SERVER.has(id)), `unhandled server block: ${fence[2] || "unlabelled"}`);
    assert.ok(!seen.has(id), `duplicate block ${id}`);
    seen.add(id);
    assert.equal(fence[2], IDS.includes(id) ? "sh" : NON_SERVER.get(id), `classification of ${id}`);
    if (id.startsWith("endpoint-")) {
      // The installation's blocks under "### On the endpoint:", the removal's under its own section.
      const removal = prefix.lastIndexOf("\n## Remove the agent from an endpoint\n");
      const home = REMOVAL.includes(id.slice("endpoint-".length)) ? removal : prefix.lastIndexOf("\n### On the endpoint:");
      assert.ok(home >= 0 && home > prefix.lastIndexOf("\n## Later users") && (home === removal || home > removal), `${id} belongs to its endpoint procedure`);
    }
  }
  assert.equal(seen.size, IDS.length + NON_SERVER.size, "all classified blocks remain present");
  const all = [...text.matchAll(/^([ \t]*)```sh\r?\n([\s\S]*?)^\1```/gm)];
  const result = new Map();
  for (const match of all) {
    const prefix = text.slice(0, match.index);
    const id = /<!-- deploy_ac_021: ([a-z-]+) -->\r?\n\r?\n$/.exec(prefix)?.[1];
    assert.ok(id && IDS.includes(id), `unhandled server block at line ${prefix.split("\n").length}`);
    assert.ok(!result.has(id), `duplicate block ${id}`);
    result.set(id, match[2].replaceAll("\r", "").trim());
  }
  assert.deepEqual([...result.keys()], IDS, "every server block has an execution case");
  return result;
}

/** The endpoint's PowerShell blocks by id, their text without carriage returns. */
function endpointBlocks(text) {
  const result = new Map();
  for (const match of text.matchAll(/<!-- deploy_ac_021: endpoint-([a-z-]+) -->\r?\n\r?\n```powershell\r?\n([\s\S]*?)^```/gm)) {
    result.set(match[1], match[2].replaceAll("\r", ""));
  }
  assert.deepEqual([...result.keys()], ENDPOINT, "the endpoint's blocks, in order");
  return result;
}

test("SPEC-024: the endpoint's commands run the setup as its amendments ask, and keep the token off command lines", () => {
  const endpoint = endpointBlocks(readFileSync(FILE, "utf8"));
  // The Amendment 2026-11-09, item 2, and the Amendment 2026-11-13, point 1: the bootstrap
  // file's command is quiet and never restarts the computer; -log keeps Burn's log.
  const setup = endpoint.get("setup");
  for (const option of ["'-quiet'", "'-norestart'", "'-log'", '"CG_BOOTSTRAP=$dir\\bootstrap.toml"']) {
    assert.ok(setup.includes(option), `the setup's command passes ${option}`);
  }
  // The wizard's command shows the setup's window, with no bootstrap file.
  const wizard = endpoint.get("wizard");
  assert.ok(wizard.includes("'-norestart'") && wizard.includes("'-log'"), wizard);
  assert.doesNotMatch(wizard, /-quiet|-passive|CG_BOOTSTRAP/);
  // The Amendment 2026-11-15, point 1: the purge is an uninstallation with CG_PURGE=yes, with
  // -norestart and -log; the default uninstallation carries no CG_PURGE. Neither is -unsafeuninstall,
  // which purges nothing (the Amendment 2026-11-16, point 2).
  const uninstall = endpoint.get("uninstall");
  const purge = endpoint.get("purge");
  for (const block of [uninstall, purge]) {
    for (const option of ["'-uninstall'", "'-quiet'", "'-norestart'", "'-log'"]) {
      assert.ok(block.includes(option), `the uninstallation's command passes ${option}`);
    }
    assert.doesNotMatch(block, /unsafeuninstall|CG_BOOTSTRAP/i);
  }
  assert.doesNotMatch(uninstall, /CG_PURGE/);
  assert.equal([...purge.matchAll(/CG_PURGE/g)].length, 1, purge);
  assert.ok(purge.includes("'-uninstall', 'CG_PURGE=yes',"), purge);
  // A windowed program: PowerShell's call operator neither waits for it nor sets $LASTEXITCODE.
  for (const block of [setup, wizard, uninstall, purge]) {
    assert.match(block, /Start-Process -FilePath "\$dir\\cyberguard-agent-<version>-x64-setup\.exe" .* -Wait -PassThru\n\$run\.ExitCode\n$/);
    // Burn's log goes in the inputs' directory, never under %ProgramData%\CyberGuard, where a
    // log would keep the directory the purge removes (the Amendment 2026-11-15, point 7).
    assert.match(block, /^\$log = "\$dir\\[a-z]+-\$\(Get-Date -Format yyyyMMdd-HHmmss\)\.log"$/m);
    assert.match(block, /'-log', \$log[, ]/, "-log names $log");
  }
  // The purge runs only while the setup is installed, so its result is checked on the endpoint.
  assert.equal(endpoint.get("purge-check"), 'Test-Path -LiteralPath "$env:ProgramData\\CyberGuard"\nsc.exe query CyberGuardAgent\n');
  // The removal runs the setup from the same directory, checked again against its digest.
  const directory = endpoint.get("directory").match(/^\$dir = '([^']+)'$/m)[1];
  const removal = endpoint.get("removal-setup");
  assert.equal(removal.match(/^\$dir = '([^']+)'$/m)?.[1], directory, removal);
  assert.ok(removal.includes(endpoint.get("checksum").trim()), removal);
  // §Data contracts §3: the token is read without echo, into a file created new with its
  // protection, in a directory created with its own; no block carries it on a command line.
  const bootstrap = endpoint.get("bootstrap");
  assert.match(bootstrap, /^\$token = Read-Host -AsSecureString /m);
  assert.match(bootstrap, /SetSecurityDescriptorSddlForm\('O:BAD:P\(A;;FA;;;SY\)\(A;;FA;;;BA\)'\)/);
  assert.match(bootstrap, /\[System\.IO\.FileMode\]::CreateNew/);
  assert.match(endpoint.get("directory"), /SetSecurityDescriptorSddlForm\('O:BAD:P\(A;OICI;FA;;;SY\)\(A;OICI;FA;;;BA\)'\)/);
  // The token appears in two places alone: read without echo, and expanded into the file.
  const uses = [...bootstrap.matchAll(/\$token\b/g)].length;
  assert.equal(uses, 2, "the token is read once and written once");
  assert.ok(bootstrap.includes("$([System.Net.NetworkCredential]::new('', $token).Password)"), bootstrap);
  // The Amendment 2026-11-14, point 2: the installed binary's digest is the raw executable's.
  assert.match(endpoint.get("binary"), /Get-FileHash -Algorithm SHA256 "\$env:ProgramFiles\\CyberGuard\\Agent\\cg-agent\.exe"\)\.Hash -eq '<the agent executable's SHA-256 in SHA256SUMS>'/);
  // The Amendment 2026-11-01, point 5, and the Amendment 2026-11-13, point 6: read by GUID,
  // and Windows Error Reporting's LocalDumps read whole, cg-agent.exe's and cg-setup.exe's included.
  const audit = endpoint.get("audit");
  for (const guid of ["{0CCE9215-69AE-11D9-BED3-505054503030}", "{0CCE9217-69AE-11D9-BED3-505054503030}"]) {
    assert.ok(audit.includes(guid), guid);
  }
  assert.match(endpoint.get("dumps"), /reg query 'HKLM\\SOFTWARE\\Microsoft\\Windows\\Windows Error Reporting\\LocalDumps' \/s/);
  assert.match(endpoint.get("dumps"), /CrashControl/);

  for (const [id, block] of endpoint) {
    assert.doesNotMatch(block, /<token>|token=|CG_TOKEN|\$env:[A-Z_]+ *=/i, `${id} holds no token`);
    // The setup creates %ProgramData%\CyberGuard itself, and refuses one made otherwise (27).
    assert.doesNotMatch(block, /(New-Item|Copy-Item|Set-Content|Out-File|CreateDirectory)[^\n]*ProgramData/i, id);
  }
});

/** The section under `heading`, up to the next heading of its level or a higher one. */
function section(text, heading) {
  const start = text.indexOf(`\n${heading}\n`);
  assert.ok(start >= 0, heading);
  const level = heading.match(/^#+/)[0].length;
  const rest = text.slice(start + heading.length + 2);
  const next = new RegExp(`^#{1,${level}} `, "m").exec(rest);
  return rest.slice(0, next ? next.index : undefined);
}

/** The rows of the table under `heading`: code to hexadecimal. */
function codeRows(text, heading) {
  return new Map([...section(text, heading).matchAll(/^\| (-?\d+)[^|]* \| `(0x[0-9a-f]+)`[^|]* \|/gm)].map((m) => [m[1], m[2]]));
}

test("SPEC-024: the setup's result lists every code cg-setup exits with, and Burn's", () => {
  const text = readFileSync(FILE, "utf8");
  const rows = codeRows(text, "#### The setup's result");
  const codes = [...readFileSync(join(REPO, "agent/cg-setup/src/codes.rs"), "utf8").matchAll(/^pub const ([A-Z_]+): u8 = (\d+);/gm)];
  assert.ok(codes.length >= 21 && codes.some(([, name]) => name === "PURGE_INCOMPLETE"), "cg-setup's codes are read");
  for (const [, name, code] of codes) {
    assert.equal(rows.get(code), `0x${Number(code).toString(16)}`, `${name} (${code})`);
  }
  // Burn's and Windows Installer's (the Amendments 2026-11-12 (second) and (third)).
  for (const [code, hex] of [["3010", "0xbc2"], ["3017", "0xbc9"], ["1602", "0x642"], ["1638", "0x666"], ["1603", "0x643"], ["-2146889721", "0x80091007"]]) {
    assert.equal(rows.get(code), hex, code);
  }
  // The uninstallation's own table: what its Purge step exits with (the Amendment 2026-11-15,
  // point 3, and its Effect: 20, 21, 26, 27, 28, 31, 40, 46 and 47) and Burn's.
  const uninstallation = codeRows(text, "### The uninstallation's result");
  const expected = ["0", "3010", "3017", "1602", "1603", "20", "21", "26", "27", "28", "31", "40", "46", "47"];
  assert.deepEqual([...uninstallation.keys()], expected, "the uninstallation's codes, and no other");
  for (const code of expected) {
    assert.equal(uninstallation.get(code), `0x${Number(code).toString(16)}`, `the uninstallation's ${code}`);
  }
  // 46's codes of the service are those cg-setup reads as given before any removal.
  const before = readFileSync(join(REPO, "agent/cg-setup/src/service.rs"), "utf8").match(/^pub const PURGE_BEFORE_REMOVAL: \[u32; \d+\] = \[([\d, ]+)\];/m)[1];
  const row46 = section(text, "### The uninstallation's result").split("\n").find((line) => line.startsWith("| 46 |"));
  assert.ok(row46.includes(`(${before})`), row46);
});

test("SPEC-024: the agent's exit codes list every code of the service, the purge start's 14 and 15 included", () => {
  const text = readFileSync(FILE, "utf8");
  const exits = section(text, "### The agent's exit codes");
  const rows = [...exits.matchAll(/^\| (\d+) \|/gm)].map((m) => Number(m[1]));
  const service = [...readFileSync(join(REPO, "agent/cg-agent/src/service/mod.rs"), "utf8").matchAll(/^pub const EXIT_[A-Z_]+: u8 = (\d+);/gm)].map((m) => Number(m[1]));
  assert.ok(service.includes(14) && service.includes(15), "the service's codes are read");
  // 0 to the service's highest code, each once and in order.
  assert.deepEqual(rows, Array.from({ length: Math.max(...service) + 1 }, (_, code) => code));
  // The purge start's lines (SPEC-024 Amendment 2026-11-15, point 4; service/purge.rs).
  const purge = readFileSync(join(REPO, "agent/cg-agent/src/service/purge.rs"), "utf8");
  for (const line of ["cg-agent: the purge refuses this host:", "cg-agent: the purge left", "cg-agent: the purge ended abnormally", "cg-agent: the purge's directories fail their check:"]) {
    assert.ok(purge.includes(line), `purge.rs writes ${line}`);
    assert.ok(exits.includes(`\`${line}`), `the runbook gives ${line}`);
  }
  // The purge start's argument refusal (migration.rs) and its entry 1001 (mod.rs); the start's
  // refusals of the migration's argument (2) and of a staging or legacy directory that counts (5),
  // SPEC-024 Amendment 2026-11-20, point 2.
  for (const [file, line] of [
    ["migration.rs", "cg-agent: invalid purge arguments:"],
    ["mod.rs", "cg-agent: the purge removed the product's data and left nothing"],
    ["migration.rs", "cg-agent: invalid migration arguments: --migrate-legacy is given, and the migration of a console installation is not supported"],
    ["migration.rs", "no release of the setup writes a console installation's migration directory, and the product neither adopts nor migrates one; nothing was changed"],
  ]) {
    assert.ok(readFileSync(join(REPO, "agent/cg-agent/src/service", file), "utf8").includes(line), `${file} writes ${line}`);
    assert.ok(exits.includes(`\`${line}`), `the runbook gives ${line}`);
  }
});

test("SPEC-024 Amendment 2026-11-20: install.md offers no migration and no removal by hand, and gives R18's lines", () => {
  const text = readFileSync(FILE, "utf8").replaceAll("\r", "");
  // Point 5: the migration has no route; no line offers it, awaits it or takes a migrated host as a condition.
  assert.doesNotMatch(text, /unless its migration|SPEC-024's migration|migration of SPEC-024|Amendment 2026-11-05|migrates a console installation|migrated from a console installation|migrated endpoint|other than a migration|a migration (left|kept)|interrupted migration|does not give yet|not in this document yet|R5 and R1|R1 of the \[threat model\]/);
  // Point 6: one paragraph, which every refusal's row cites, gives no repair and warns against a removal by hand.
  const warning = section(text, "#### No repair, and no removal by hand");
  for (const phrase of ["junction", "by hand", "(code 5)", "neither adopts nor migrates", "formatted, not migrated", "`CG_MIGRATE_LEGACY`", "`CG_TRUST_ANCHORS`"]) {
    assert.ok(warning.includes(phrase), `the warning says ${phrase}`);
  }
  const cite = "(#no-repair-and-no-removal-by-hand)";
  const row = (heading, code) => section(text, heading).split("\n").find((line) => line.startsWith(`| ${code} |`));
  for (const [heading, codes] of [["#### The setup's result", ["27", "28", "42"]], ["### The uninstallation's result", ["27", "28", "46", "47"]], ["### The agent's exit codes", ["13", "14"]]]) {
    for (const code of codes) assert.ok(row(heading, code).includes(cite), `${heading} ${code} cites the warning`);
  }
  // Point 3: 28 is what the product does not support, and nothing changed.
  for (const heading of ["#### The setup's result", "### The uninstallation's result"]) {
    assert.match(row(heading, "28"), /neither adopts nor migrates/, heading);
    assert.doesNotMatch(row(heading, "28"), /set `?CG_MIGRATE|run [^|]*with `?CG_MIGRATE/, heading);
  }
  assert.ok(row("#### The setup's result", "28").includes("a migration asked for, a non-empty `CG_MIGRATE_LEGACY` or `CG_TRUST_ANCHORS`"));
  // Point 2: the service's 2 and 5 gain their causes.
  assert.ok(row("### The agent's exit codes", "2").includes("`--migrate-legacy=`"));
  assert.ok(row("### The agent's exit codes", "5").includes("`agent.migrating`"));
  // Point 1, and the threat model's R18: imaged before the setup, a copy purged and installed anew, the volume encrypted.
  const endpoint = section(text, "### The Windows endpoint");
  for (const sentence of [
    "An endpoint imaged before the setup runs on it, never after:",
    "A copy of an enrolled endpoint, an image or a clone, is purged and installed anew with a new token",
    "its old certificate staying valid on the server until it expires or its row is deleted (R5).",
    "The system volume encrypted, for instance with BitLocker: that is the deployment's control against an offline read",
  ]) {
    assert.ok(endpoint.includes(sentence), sentence);
  }
  assert.ok(text.includes("is a console installation, which the setup refuses (28) and the product neither adopts nor migrates: its development host is formatted, then the product is installed on it with the setup and a new token"));
});

test("SPEC-024: the runbook names the purge's closed list of names, as the service removes them", () => {
  const text = readFileSync(FILE, "utf8");
  const removes = section(text, "#### What the purge removes");
  // Each list item "- in `<directory>`: `<name>`, ...", a range `agent.1.jsonl` to `agent.5.jsonl` expanded.
  const listed = new Map();
  for (const [, directory, names] of removes.matchAll(/^- in `([^`]+)`[^:]*: (.+)$/gm)) {
    const expanded = names.replace(/`agent\.1\.jsonl` to `agent\.5\.jsonl`/, [1, 2, 3, 4, 5].map((n) => `\`agent.${n}.jsonl\``).join(", "));
    listed.set(directory, [...expanded.matchAll(/`([^`]+)`/g)].map((m) => m[1]).sort());
  }
  // purge.rs takes each list from the product's names (service/names.rs, SPEC-024 Amendment
  // 2026-11-19, point 4), whose lists spell a name or a single name's constant.
  const purge = readFileSync(join(REPO, "agent/cg-agent/src/service/purge.rs"), "utf8");
  const source = readFileSync(join(REPO, "agent/cg-agent/src/service/names.rs"), "utf8");
  const single = Object.fromEntries([...source.matchAll(/^pub const ([A-Z_]+): &str = "([^"]+)";/gm)].map((m) => [m[1], m[2]]));
  const names = (constant) => {
    const list = purge.match(new RegExp(`^const ${constant}: \\[&str; \\d+\\] = names::([A-Z_]+);`, "m"))[1];
    const items = source.match(new RegExp(`^pub const ${list}: \\[&str; \\d+\\] = \\[([^\\]]+)\\]`, "m"))[1];
    return items.split(",").map((item) => item.trim()).filter(Boolean).map((item) => {
      const name = item.startsWith('"') ? item.slice(1, -1) : single[item];
      assert.ok(name, `names.rs spells ${item}`);
      return name;
    });
  };
  // The data root's names are the configuration's and the rest's (purge.rs CONFIGURATION, REST).
  assert.deepEqual(Object.fromEntries(listed), {
    agent: [...names("CONFIGURATION"), ...names("REST")].sort(),
    "agent\\identity": names("IDENTITY").sort(),
    "agent\\logs": names("LOGS").sort(),
    "agent.legacy": names("LEGACY").sort(),
  });
});

test("deploy_ac_021: every server shell block has an execution case, including newly added commands", () => {
  const text = readFileSync(FILE, "utf8");
  blocks(text);
  for (const language of ["sh", "bash", "shell", "console", "text", "powershell", ""]) {
    assert.throws(() => blocks(`${text}\n\x60\x60\x60${language}\necho unhandled\n\x60\x60\x60\n`), /unhandled server block/);
  }
});

/** script gives both Compose and Node a real tty, including Node's raw/no-echo path. */
async function terminal(command, cwd, answers) {
  const child = spawn("script", ["-q", "-e", "-c", command, "/dev/null"], { cwd, env: process.env });
  let output = "";
  let failure = "";
  child.stdout.setEncoding("utf8").on("data", (d) => { output += d; });
  child.stderr.setEncoding("utf8").on("data", (d) => { failure += d; });
  const exited = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code) => resolve(code));
  });
  const timeout = setTimeout(() => child.kill("SIGTERM"), 60_000);
  async function waitFor(pattern) {
    const until = Date.now() + 45_000;
    while (!pattern.test(output)) {
      assert.ok(child.exitCode === null && Date.now() < until, `terminal did not reach ${pattern}; exit ${child.exitCode}`);
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    return output.match(pattern);
  }
  try {
    for (const [prompt, answer] of answers) {
      await waitFor(prompt);
      child.stdin.write(`${typeof answer === "function" ? answer(output) : answer}\n`);
    }
    const code = await exited;
    // Deliberately do not print the transcript: it contains the provisioning secret.
    assert.equal(code, 0, `terminal command failed (${failure.slice(0, 200)})`);
    return output;
  } finally {
    clearTimeout(timeout);
    child.stdin.destroy();
    if (child.exitCode === null) child.kill("SIGTERM");
  }
}

test("deploy_ac_021: install.md server commands execute, with interactive users and real verification refusal", {
  skip: !(IS_ROOT && process.env.CG_TEST_STACK === "1") && "set CG_TEST_STACK=1 as POSIX root",
  timeout: 10 * 60_000,
}, async () => {
  const commands = blocks(readFileSync(FILE, "utf8"));
  const executed = new Set();
  const version = "0.0.0-runbook";
  const work = mkdtempSync(join(tmpdir(), "cg-install-"));
  const fixture = copyBundle();
  const name = `cyberguard-deploy-${version}`;
  const dir = join(work, name);
  const project = `cginstall${randomBytes(4).toString("hex")}`;
  let started = false;
  // A fixture archive uses the real bundle files and a normal default override to select
  // local images. Actual releases pin digests, checked by release.test.mjs.
  renameSync(join(fixture, "docker-compose.test.yml"), join(fixture, "docker-compose.override.yml"));
  const staging = join(work, "staging");
  cpSync(fixture, join(staging, name), { recursive: true });
  ok("tar", ["-czf", join(work, `${name}.tar.gz`), "-C", staging, name]);
  const executable = `cg-agent-${version}-x86_64-pc-windows-msvc.exe`;
  const setup = `cyberguard-agent-${version}-x64-setup.exe`;
  writeFileSync(join(work, executable), "unattested runbook fixture, not an executable\n");
  writeFileSync(join(work, setup), "unattested runbook fixture, not a setup\n");
  writeFileSync(join(work, "SHA256SUMS"), [`${name}.tar.gz`, setup, executable].map((file) =>
    `${createHash("sha256").update(readFileSync(join(work, file))).digest("hex")}  ${file}\n`).join(""));
  function command(id, substitutions = {}) {
    executed.add(id);
    let text = commands.get(id).replaceAll("<version>", version);
    for (const [key, value] of Object.entries(substitutions)) text = text.replaceAll(`<${key}>`, value);
    return text;
  }
  function shell(id, { cwd = dir, status = 0, ...options } = {}) {
    const result = run("sh", ["-eu", "-c", command(id)], { cwd, timeout: 600_000, ...options });
    assert.equal(result.status, status, `${id}: ${result.stderr}`);
    return result.stdout;
  }
  const compose = (...args) => ok("docker", ["compose", ...args], { cwd: dir, timeout: 600_000 });
  const sql = (query) => compose("exec", "-T", "postgres", "psql", "-U", "postgres", "-d", "cyberguard", "-Atc", query);
  try {
    const versions = shell("versions", { cwd: work });
    assert.match(versions, /Docker Compose version/);
    assert.match(versions, /gpg \(GnuPG\)/);
    assert.match(versions, /gh version/);
    console.log(`deploy_ac_021 tool versions: ${versions.split("\n").filter((line) => /^(\d|Docker Compose|gpg \(|gh version)/.test(line)).join("; ")}`);
    if (process.env.CG_TEST_PRODUCTION_BASELINE === "1") {
      const engine = versions.match(/^(\d+)\.(\d+)\.(\d+)/);
      const composeVersion = versions.match(/Docker Compose version v?(\d+)\.(\d+)\.(\d+)/);
      for (const [actual, minimum, label] of [[engine, [29, 1, 2], "Engine"], [composeVersion, [2, 40, 3], "Compose"]]) {
        assert.ok(actual, `${label} version was printed`);
        const numbers = actual.slice(1).map(Number);
        const difference = numbers.map((value, index) => value - minimum[index]).find((value) => value !== 0) ?? 0;
        assert.ok(difference >= 0, `${label} ${numbers.join(".")} is below ${minimum.join(".")}`);
      }
    }
    const daemonJson = readFileSync(FILE, "utf8").match(/  ```json\n([\s\S]*?)  ```/)[1];
    const daemonFile = join(work, "daemon.json");
    writeFileSync(daemonFile, daemonJson);
    assert.deepEqual(JSON.parse(daemonJson), { "userland-proxy": false, "ip-forward": true, iptables: true });
    ok("dockerd", ["--validate", "--config-file", daemonFile]);
    // The real gh calls query GitHub for these unattested fixture hashes. CI provides its
    // read-only token; locally the caller supplies the existing credential via GH_TOKEN.
    assert.ok(process.env.GH_TOKEN, "GH_TOKEN is required for the real attestation verifier");
    const verification = command("verify").split("\n");
    assert.equal(verification.length, 4, "review any added verification command");
    const sums = run("sh", ["-eu", "-c", verification[0]], { cwd: work });
    assert.equal(sums.status, 0, sums.stderr);
    assert.equal((sums.stdout.match(/: OK/g) ?? []).length, 3);
    for (const line of verification.slice(1)) {
      const result = run("sh", ["-eu", "-c", line], { cwd: work, timeout: 90_000 });
      assert.notEqual(result.status, 0, "unattested fixture must not verify");
      assert.match(`${result.stdout}\n${result.stderr}`, /no attestations found|HTTP 404/i,
        "refusal must concern attestations, not a missing tool or login");
      assert.doesNotMatch(`${result.stdout}\n${result.stderr}`, /Bad credentials|HTTP 401|authentication required/i);
    }
    shell("unpack", { cwd: work });
    assert.ok(readFileSync(join(dir, "docker-compose.yml"), "utf8").includes("services:"));
    const first = shell("setup-template", { status: 1 });
    assert.match(first, /wrote \.env/);
    const ports = await Promise.all([freePort(), freePort(), freePort(), freePort()]);
    setEnv(dir, {
      COMPOSE_PROJECT_NAME: project, CG_DASHBOARD_HOSTNAME: "cg.example.test",
      CG_AGENT_SERVER_NAMES: "cg.example.test,127.0.0.1", CG_BIND_ADDRESS: "127.0.0.1",
      CG_HTTPS_PORT: ports[0], CG_HTTP_PORT: ports[1], CG_ENROLL_PORT: ports[2], CG_HEARTBEAT_PORT: ports[3],
    });
    makeDashboardTls(dir, "cg.example.test");
    shell("routes");
    shell("setup-secrets");
    started = true;
    shell("start");
    shell("diagnostics");
    shell("addresses");
    const key = shell("forensic-key").trim().split("\n");
    assert.equal(key[1], `sha256:${createHash("sha256").update(Buffer.from(key[0], "base64url")).digest("hex")}`);

    async function create(id, email, role, confirm = true) {
      const password = strongPassword();
      let secret;
      const answers = [[/Password: /, password], [/Password again: /, password]];
      for (let attempt = 1; attempt <= (confirm ? 1 : 3); attempt++) {
        answers.push([attempt === 1 ? /Current TOTP code: / : new RegExp(`not valid \\(${attempt - 1} of 3\\)[\\s\\S]*Current TOTP code: `), (text) => {
          secret = new URL(text.match(/otpauth:\/\/\S+/)[0]).searchParams.get("secret");
          return confirm ? totp(secret) : "not-a-code";
        }]);
      }
      const output = await terminal(command(id, { email, role }), dir, answers);
      assert.ok(!output.includes(password), "terminal must not echo the password");
      assert.equal((output.match(/otpauth:\/\//g) ?? []).length, 1);
      assert.match(output, confirm ? /the TOTP factor is confirmed/ : /the TOTP factor is not confirmed/);
      const row = sql(`SELECT user_id, role, totp_enrolled FROM api.users WHERE email='${email}'`).split("|");
      assert.equal(row[1], role);
      assert.equal(row[2], confirm ? "t" : "f");
      assert.equal(sql(`SELECT count(*) FROM api.audit_log WHERE action='user.create' AND target_id='${row[0]}' AND user_id IS NULL AND detail->>'role'='${role}'`), "1");
      if (confirm) assert.equal(sql(`SELECT count(*) FROM api.audit_log WHERE action='user.totp.enroll' AND user_id='${row[0]}' AND outcome='ok'`), "1");
      return { id: row[0], secret };
    }
    await create("admin", "admin@runbook.test", "admin");
    const ca = shell("ca");
    assert.equal(readFileSync(join(dir, "server-ca.pem"), "utf8"), ca);
    const certificate = new X509Certificate(ca);
    assert.match(ca, new RegExp(`sha256:${createHash("sha256").update(certificate.raw).digest("hex")}`));
    assert.ok(shell("token").trim().length > 0, "the issue-token command prints a token");
    shell("heartbeat-logs");
    await create("later-user", "analyst@runbook.test", "analyst");
    await create("later-user", "lateradmin@runbook.test", "admin");
    const viewer = await create("later-user", "viewer@runbook.test", "viewer", false);
    const recovered = await terminal(command("confirm"), dir, [[/User ID: /, viewer.id], [/Current TOTP code: /, totp(viewer.secret)]]);
    assert.match(recovered, /TOTP confirmation HTTP 200/);
    assert.equal(sql(`SELECT totp_enrolled FROM api.users WHERE user_id='${viewer.id}'`), "t");
    assert.equal(sql(`SELECT count(*) FROM api.audit_log WHERE action='user.totp.enroll' AND user_id='${viewer.id}' AND outcome='ok'`), "1");
    assert.deepEqual([...executed], IDS, "every documented server block was executed");
  } finally {
    if (started) compose("down", "-v", "--remove-orphans");
    rmSync(work, { recursive: true, force: true });
  }
});
