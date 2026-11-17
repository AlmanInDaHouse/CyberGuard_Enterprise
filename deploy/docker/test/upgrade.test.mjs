// SPEC-023's second Amendment 2026-10-26, #127: the upgrade keeps writers
// stopped through both migrations. Exercise the runbook, using the current
// images in two directories; this is not evidence for a future version pair.
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { cpSync, readFileSync, readdirSync, renameSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { IS_ROOT, REPO, copyBundle, freePort, makeDashboardTls, ok, readEnv, run, setEnv } from "./lib.mjs";

const document = readFileSync(join(REPO, "docs/operations/upgrade.md"), "utf8");
function command(id) {
  const match = new RegExp(`<!-- upgrade: ${id} -->\\r?\\n\\r?\\n\x60\x60\x60sh\\r?\\n([\\s\\S]*?)\x60\x60\x60`).exec(document);
  assert.ok(match, `upgrade block ${id}`);
  return match[1].replaceAll("\r", "");
}

const install = readFileSync(join(REPO, "docs/operations/install.md"), "utf8").replaceAll("\r", "");
const configuration = readFileSync(join(REPO, "docs/operations/configuration.md"), "utf8").replaceAll("\r", "");
// The agent's upgrade on an endpoint (SPEC-024 §Operational §4, the Amendment 2026-11-12
// (second), points 3, 4 and 9): its PowerShell blocks, in the document's order. They run on
// Windows alone (install_ac_016); these tests read what the contract asks of them.
const AGENT = ["directory", "copy", "setup", "log", "repair"];

/** The section under `heading`, up to the next heading of its level or a higher one. */
function section(text, heading) {
  const start = text.indexOf(`\n${heading}\n`);
  assert.ok(start >= 0, heading);
  const level = heading.match(/^#+/)[0].length;
  const rest = text.slice(start + heading.length + 2);
  const next = new RegExp(`^#{1,${level}} `, "m").exec(rest);
  return rest.slice(0, next ? next.index : undefined);
}

/** Every PowerShell fence of `text`, each marked `<!-- <prefix>: <id> -->`: id to text. */
function powershell(text, prefix) {
  const result = new Map();
  // A fence indented in a list item is a block too.
  for (const fence of text.matchAll(/^([ \t]*)```powershell\n([\s\S]*?)^\1```/gm)) {
    const id = new RegExp(`<!-- ${prefix}: ([a-z-]+) -->\\n\\n$`).exec(text.slice(0, fence.index))?.[1];
    assert.ok(id, `an unmarked PowerShell block at line ${text.slice(0, fence.index).split("\n").length}`);
    assert.ok(!result.has(id), `duplicate block ${id}`);
    result.set(id, fence[2]);
  }
  return result;
}

/** install.md's endpoint block `id`. */
function installBlock(id) {
  const match = new RegExp(`<!-- deploy_ac_021: endpoint-${id} -->\\n\\n\x60\x60\x60powershell\\n([\\s\\S]*?)^\x60\x60\x60`, "m").exec(install);
  assert.ok(match, `install.md's endpoint-${id}`);
  return match[1];
}

/** The codes of a result table's first column, "20 to 28, 31" expanded. */
function codes(cell) {
  return cell.split(", ").flatMap((part) => {
    const range = /^(\d+) to (\d+)$/.exec(part);
    if (!range) return [part];
    return Array.from({ length: Number(range[2]) - Number(range[1]) + 1 }, (_, i) => String(Number(range[1]) + i));
  });
}

test("SPEC-024: the agent's upgrade runs the newer setup as the installation does, without a bootstrap file", () => {
  const blocks = powershell(document, "upgrade");
  assert.deepEqual([...blocks.keys()], AGENT.map((id) => `endpoint-${id}`), "the agent's blocks, in order");
  const agent = section(document, "## Upgrading the agent on an endpoint");
  for (const [id, block] of blocks) {
    assert.ok(agent.includes(block), `${id} belongs to the agent's section`);
    // §Data contracts §3: no token on a command line; the setup creates %ProgramData%\CyberGuard.
    assert.doesNotMatch(block, /<token>|token=|CG_TOKEN|\$env:[A-Z_]+ *=/i, id);
    assert.doesNotMatch(block, /(New-Item|Copy-Item|Set-Content|Out-File|CreateDirectory)[^\n]*ProgramData/i, id);
  }
  // The installation's inputs' directory, checked again before a setup runs elevated from it:
  // its attributes, owner and DACL as install.md's block prints them at its creation.
  const check = blocks.get("endpoint-directory");
  const created = installBlock("directory");
  assert.equal(check.match(/^\$dir = '([^']+)'$/m)?.[1], created.match(/^\$dir = '([^']+)'$/m)[1], check);
  assert.ok(check.includes("$item = Get-Item -Force $dir\n") && created.includes("$item = Get-Item -Force $dir\n"), check);
  assert.ok(check.endsWith("'{0} {1}' -f $item.Attributes, $item.GetAccessControl().GetSecurityDescriptorSddlForm('Owner, Access')\n"), check);
  const sddl = created.match(/SetSecurityDescriptorSddlForm\('([^']+)'\)/)[1];
  assert.ok(install.includes(`It prints \`Directory ${sddl} 0\`.`), "install.md's directory block prints that form");
  assert.ok(agent.includes(`It prints \`Directory ${sddl}\`. Stop if it prints anything else`), "the check's expected output");
  // The newer setup, checked against its digest there.
  const copy = blocks.get("endpoint-copy");
  assert.ok(copy.includes(installBlock("checksum").trim()), copy);
  assert.doesNotMatch(copy, /server-ca\.pem|bootstrap|^\$dir =/m);
  assert.ok(agent.includes("It prints `True`. Stop if it prints `False`."), "the copy block's expected output");
  // A repair checks the directory and the installed version's setup, as the uninstallation checks it.
  const repair = section(document, "### A repair");
  assert.ok(repair.includes("(#the-newer-setup)") && repair.includes("(install.md#the-setup-to-uninstall-with)"), "the repair checks its directory and setup");
  // The installation's command, without CG_BOOTSTRAP (30 on an enrolled endpoint): quiet,
  // never restarting the computer (the Amendment 2026-11-09, item 2), Burn's log in the
  // inputs' directory, outside %ProgramData%\CyberGuard (the Amendment 2026-11-15, point 7).
  const setup = blocks.get("endpoint-setup");
  const installation = installBlock("setup").replace(', "CG_BOOTSTRAP=$dir\\bootstrap.toml"', "").replace("$dir\\setup-", "$dir\\upgrade-");
  assert.notEqual(installation, installBlock("setup"), "install.md's command carries CG_BOOTSTRAP");
  assert.equal(setup, installation);
  // A repair is the same command with -repair (point 9), never with a purge or a bootstrap file.
  assert.equal(blocks.get("endpoint-repair"), setup.replace("$dir\\upgrade-", "$dir\\repair-").replace("-ArgumentList '-quiet'", "-ArgumentList '-repair', '-quiet'"));
  for (const block of [setup, blocks.get("endpoint-repair")]) {
    assert.doesNotMatch(block, /CG_BOOTSTRAP|CG_PURGE|-uninstall|unsafeuninstall|-passive|CG_MIGRATE|CG_TRUST/);
  }
  // Burn's log on an upgrade: the installation's lines, and the restore's (the third Amendment
  // 2026-11-12, point 1: "Applied rollback package", or "Applied non-vital package" when it fails).
  const log = blocks.get("endpoint-log");
  for (const pattern of ["'Applied execute package'", "'Apply complete'", "'Exit code'"]) {
    assert.ok(installBlock("log").includes(pattern) && log.includes(pattern), pattern);
  }
  assert.ok(log.includes("'Applied rollback package'") && log.includes("'Applied non-vital package'"), log);
  assert.match(log, /^Select-String -Path \$log -Pattern /);
});

test("SPEC-024: the upgrade's result lists every code an upgrade ends with, as install.md's table writes it", () => {
  const table = section(document, "### The upgrade's result");
  const rows = [...table.matchAll(/^\| ([-\d][^|]*?) \|/gm)].map((m) => m[1]);
  const listed = rows.flatMap(codes);
  assert.equal(new Set(listed).size, listed.length, "each code once");
  // cg-setup's codes but the purge's, which only an uninstallation ends with, and Burn's.
  const setup = [...readFileSync(join(REPO, "agent/cg-setup/src/codes.rs"), "utf8").matchAll(/^pub const ([A-Z_]+): u8 = (\d+);/gm)];
  assert.ok(setup.some(([, name]) => name === "DOWNGRADE") && setup.some(([, name]) => name === "ALREADY_CONFIGURED"), "cg-setup's codes are read");
  const expected = [...setup.filter(([, name]) => !name.startsWith("PURGE_")).map(([, , code]) => code), "0", "3010", "3017", "1602", "1638", "1603", "-2146889721"];
  assert.deepEqual([...listed].sort(), [...new Set(expected)].sort());
  // Each is a code of install.md's table, which gives its hexadecimal value and cause.
  const installed = section(install, "#### The setup's result");
  for (const code of listed) {
    assert.match(installed, new RegExp(`^\\| ${code.replace("-", "\\-")}[ ,|]`, "m"), code);
  }
  // What has not been read stays marked (the Amendment 2026-11-12 (second), points 3 and 4).
  // The marks are read in the column of the endpoint's state.
  const after = (code) => table.split("\n").find((line) => line.startsWith(`| ${code} |`)).split(" | ")[1];
  for (const code of ["29", "20 to 28, 31", "40 to 43", "45", "1602"]) {
    assert.match(after(code), /not been read|not read/, code);
  }
  for (const code of ["0", "1638", "30", "1603", "44"]) {
    assert.match(after(code), /\(read\)/, code);
  }
});

test("SPEC-024: upgrade.md teaches no console route; install.md names the re-enrollment of an enrolled endpoint (M6)", () => {
  // The service's identity is SYSTEM's alone, in identity\ (the Amendment 2026-11-04); files
  // directly in the data root are a console installation the setup refuses (28).
  assert.doesNotMatch(document, /cert\.pem|key\.dat|identity\.json|enrollment\.(cert|key)_path|run by hand until|^```toml/m);
  assert.doesNotMatch(document, /(Remove-Item|Copy-Item)[^\n]*ProgramData/i);
  assert.match(section(document, "### Agents"), /Enrolling such an agent again with the new installation is not in these runbooks yet\./);
  // SPEC-023 §Operational §6, as its upgrade document must say it.
  assert.ok(document.includes("`server.url must use https when [enrollment] is set`"));
  // M6 (Manuel, 2026-11-17): an enrolled endpoint enrolls again by the purge and a new token, the
  // old agent left on the server (R5; SPEC-024 Amendment 2026-11-15, point 8).
  assert.ok(install.includes("To enroll an enrolled endpoint again, as when its certificate has expired (code 7) or its server is a new installation with another CA, purge it ([Remove the agent from an endpoint](#remove-the-agent-from-an-endpoint)) and run the setup again with a new token, and the server's current anchor and fingerprints, in a new bootstrap file or the wizard: the endpoint then enrolls a new agent."));
  assert.ok(install.includes("The purge revokes nothing: the old agent stays on the server that enrolled it, which has no command that removes it yet, and its certificate stays valid there until it expires or an operator deletes its row"));
  assert.ok(install.includes("The purge followed by an enrollment with a new token has not been run, in CI or on an endpoint."));
  assert.ok(install.includes("for a copy of its key that a backup or a shadow copy kept (R5 and R1 of the [threat model](../security/threat-model.md));"));
  assert.ok(install.includes("Between the purge and the new setup an ordinary user can create `C:\\ProgramData\\CyberGuard` first, and the setup then refuses it (27 or 28)."));
  assert.doesNotMatch(install, /Re-enrolling an enrolled endpoint is not in this document/);
});

test("#165: the agent's keys change by a rewrite of agent.toml in place, never by its replacement", () => {
  const blocks = powershell(section(configuration, "### Change a key on an endpoint"), "configuration");
  assert.deepEqual([...blocks.keys()], ["agent-directory", "agent-copy", "agent-write", "agent-cleanup"]);
  // The copy may hold the token: the inputs' directory is checked first, as the upgrade checks it.
  assert.equal(blocks.get("agent-directory"), powershell(document, "upgrade").get("endpoint-directory"));
  assert.equal(blocks.get("agent-directory").match(/^\$dir = '([^']+)'$/m)?.[1], installBlock("directory").match(/^\$dir = '([^']+)'$/m)[1]);
  // The service is stopped before the copy, and the copy's hash kept for the write-back's guard.
  assert.equal(blocks.get("agent-copy"), '$config = "$env:ProgramData\\CyberGuard\\agent\\agent.toml"\nStop-Service CyberGuardAgent\nCopy-Item $config "$dir\\agent.toml.edit"\n$before = (Get-FileHash "$dir\\agent.toml.edit").Hash\n');
  // The one write into %ProgramData%: into the existing file, which keeps its owner and DACL, only
  // while the service is stopped and agent.toml unchanged since the copy (no start meanwhile
  // enrolled and rewrote it), and the service started only after a write that did not fail.
  assert.equal(blocks.get("agent-write"), 'if ($before -and (Get-Service CyberGuardAgent).Status -eq \'Stopped\' -and (Get-FileHash $config).Hash -eq $before) { try { [System.IO.File]::WriteAllText($config, [System.IO.File]::ReadAllText("$dir\\agent.toml.edit")); \'written\'; Start-Service CyberGuardAgent } catch { "not written: $_" } } else { \'not written: the service is not stopped, or agent.toml changed since the copy\' }\nsc.exe query CyberGuardAgent\n');
  assert.equal(blocks.get("agent-cleanup"), 'Remove-Item "$dir\\agent.toml.edit"\n');
  for (const [id, block] of blocks) {
    assert.doesNotMatch(block, /(\bMove-Item|Set-Content|Out-File|\bNew-Item|(Remove-Item|Copy-Item)[^\n]*(ProgramData|\$config\s*$))/im, id);
  }
});

test("upgrade: every PowerShell block of upgrade.md is classified, including newly added ones", () => {
  assert.throws(() => powershell(`${document}\n\x60\x60\x60powershell\nWrite-Output unhandled\n\x60\x60\x60\n`, "upgrade"), /an unmarked PowerShell block/);
});

test("upgrade: both migration commands finish before writers restart, and a failure stops the block (#127)", {
  skip: !(IS_ROOT && process.env.CG_TEST_STACK === "1") && "set CG_TEST_STACK=1 as POSIX root",
  timeout: 5 * 60_000,
}, async () => {
  const older = copyBundle();
  const newer = copyBundle();
  const project = `cgupgrade${randomBytes(4).toString("hex")}`;
  for (const dir of [older, newer]) {
    renameSync(join(dir, "docker-compose.test.yml"), join(dir, "docker-compose.override.yml"));
  }
  assert.equal(run("sh", ["setup.sh"], { cwd: older }).status, 1);
  setEnv(older, {
    COMPOSE_PROJECT_NAME: project, CG_DASHBOARD_HOSTNAME: "upgrade.example.test",
    CG_AGENT_SERVER_NAMES: "localhost,127.0.0.1", CG_BIND_ADDRESS: "127.0.0.1",
    CG_HTTP_PORT: String(await freePort()), CG_HTTPS_PORT: String(await freePort()),
    CG_ENROLL_PORT: String(await freePort()), CG_HEARTBEAT_PORT: String(await freePort()),
  });
  makeDashboardTls(older, "upgrade.example.test");
  ok("sh", ["setup.sh"], { cwd: older });
  const compose = (dir, args) => ok("docker", ["compose", ...args], { cwd: dir });
  const states = () => {
    const rows = compose(newer, ["ps", "-a", "--format", "json"]).trim();
    return new Map((rows.startsWith("[") ? JSON.parse(rows) : rows.split("\n").map(JSON.parse)).map((r) => [r.Service, r]));
  };
  const startTime = (service) => {
    const id = compose(newer, ["ps", "-aq", service]);
    return ok("docker", ["inspect", "--format", "{{.State.StartedAt}}", id]);
  };
  try {
    compose(older, ["up", "-d", "--wait"]);
    const anchor = compose(older, ["exec", "-T", "api", "node", "dist/cli/forensic-key.js"]);
    const ca = compose(older, ["exec", "-T", "ingest", "node", "dist/cli/export-ca.js"]);
    cpSync(join(older, ".env"), join(newer, ".env"));
    cpSync(join(older, "tls"), join(newer, "tls"), { recursive: true });
    ok("sh", ["setup.sh"], { cwd: newer });
    ok("sh", ["-eu", "-c", command("stop")], { cwd: newer });
    const writers = ["ingest", "api", "dashboard"];
    const stoppedAt = new Map(writers.map((s) => [s, startTime(s)]));
    const assertStopped = () => {
      const rows = states();
      for (const service of writers) {
        assert.equal(rows.get(service)?.State, "exited", service);
        assert.equal(startTime(service), stoppedAt.get(service), `${service} did not restart`);
      }
    };
    assertStopped();
    ok("sh", ["-eu", "-c", command("backup")], { cwd: newer });
    const backups = readdirSync(join(newer, "backups"));
    assert.equal(backups.filter((f) => f.endsWith(".gpg")).length, 2);

    // A real invalid configuration fails the first one-shot. The exact shell
    // block must not run api-migrate afterwards or restart either writer.
    const apiBefore = startTime("api-migrate");
    setEnv(newer, { CG_RETENTION_PROCESS_DAYS: "6" });
    const failed = run("sh", ["-c", command("migrate")], { cwd: newer });
    assert.notEqual(failed.status, 0);
    assert.match(failed.stdout + failed.stderr, /INGEST_RETENTION_PROCESS_DAYS/);
    assert.equal(startTime("api-migrate"), apiBefore);
    assertStopped();

    setEnv(newer, { CG_RETENTION_PROCESS_DAYS: "365" });
    const apiPassword = readEnv(newer).get("CG_PG_API_OWNER_PASSWORD");
    setEnv(newer, { CG_PG_API_OWNER_PASSWORD: "deliberately-wrong-upgrade-fixture" });
    const apiFailed = run("sh", ["-c", command("migrate")], { cwd: newer });
    assert.notEqual(apiFailed.status, 0);
    assert.match(apiFailed.stdout + apiFailed.stderr, /password authentication failed for user "cg_api_owner"/);
    assert.equal(states().get("ingest-migrate")?.ExitCode, 0);
    assert.notEqual(states().get("api-migrate")?.ExitCode, 0);
    assertStopped();
    setEnv(newer, { CG_PG_API_OWNER_PASSWORD: apiPassword });
    ok("sh", ["-c", command("migrate")], { cwd: newer });
    assertStopped();
    const migrated = states();
    for (const service of ["ingest-migrate", "api-migrate"]) {
      assert.equal(migrated.get(service)?.State, "exited");
      assert.equal(migrated.get(service)?.ExitCode, 0);
    }
    assert.notEqual(startTime("api-migrate"), apiBefore);
    ok("sh", ["-c", command("start")], { cwd: newer });
    for (const service of ["ingest", "api", "dashboard", "proxy"]) {
      assert.equal(states().get(service)?.Health, "healthy", service);
    }
    assert.equal(compose(newer, ["exec", "-T", "api", "node", "dist/cli/forensic-key.js"]), anchor);
    assert.equal(compose(newer, ["exec", "-T", "ingest", "node", "dist/cli/export-ca.js"]), ca);
  } finally {
    run("docker", ["compose", "down", "-v", "--remove-orphans"], { cwd: older });
  }
});
