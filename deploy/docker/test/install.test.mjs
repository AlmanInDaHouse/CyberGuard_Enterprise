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
  "binary", "service", "events", "agent-log", "audit", "dumps"];
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
      assert.ok(prefix.lastIndexOf("### On the endpoint:") > prefix.lastIndexOf("## Later users"), `${id} belongs to the endpoint procedure`);
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
  // A windowed program: PowerShell's call operator neither waits for it nor sets $LASTEXITCODE.
  for (const block of [setup, wizard]) {
    assert.match(block, /Start-Process -FilePath "\$dir\\cyberguard-agent-<version>-x64-setup\.exe" .* -Wait -PassThru\n\$run\.ExitCode\n$/);
  }
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

test("SPEC-024: the setup's result lists every code cg-setup exits with on an installation, and Burn's", () => {
  const text = readFileSync(FILE, "utf8");
  const start = text.indexOf("#### The setup's result");
  const table = text.slice(start, text.indexOf("\n#### ", start + 1));
  const rows = new Map([...table.matchAll(/^\| (-?\d+)[^|]* \| `(0x[0-9a-f]+)`[^|]* \|/gm)].map((m) => [m[1], m[2]]));
  const codes = [...readFileSync(join(REPO, "agent/cg-setup/src/codes.rs"), "utf8").matchAll(/^pub const ([A-Z_]+): u8 = (\d+);/gm)];
  assert.ok(codes.length >= 20, "cg-setup's codes are read");
  // 46 and 47 are the uninstallation's (SPEC-024 Amendment 2026-11-15).
  for (const [, name, code] of codes.filter(([, name]) => !name.startsWith("PURGE_"))) {
    assert.equal(rows.get(code), `0x${Number(code).toString(16)}`, `${name} (${code})`);
  }
  // Burn's and Windows Installer's (the Amendments 2026-11-12 (second) and (third)).
  for (const [code, hex] of [["3010", "0xbc2"], ["3017", "0xbc9"], ["1602", "0x642"], ["1638", "0x666"], ["1603", "0x643"], ["-2146889721", "0x80091007"]]) {
    assert.equal(rows.get(code), hex, code);
  }
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
