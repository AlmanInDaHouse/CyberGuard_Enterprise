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
