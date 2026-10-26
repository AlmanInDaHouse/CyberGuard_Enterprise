// SPEC-023 deploy_ac_021 and the third Amendment 2026-10-26:
// complete documents, a valid logging override and the operator's ingest pair.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes, X509Certificate } from "node:crypto";
import { existsSync, readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { connect, createServer } from "node:tls";
import { test } from "node:test";
import { IS_ROOT, REPO, SECRETS, TAG, copyBundle, freePort, makeDashboardTls, ok, run, setEnv } from "./lib.mjs";

const DOCS = join(REPO, "docs/operations");
const configuration = readFileSync(join(DOCS, "configuration.md"), "utf8");
const backup = readFileSync(join(DOCS, "backup-and-restore.md"), "utf8");
function block(id, language) {
  const found = new RegExp(`<!-- configuration: ${id} -->\\r?\\n\\r?\\n\x60\x60\x60${language}\\r?\\n([\\s\\S]*?)\x60\x60\x60`).exec(configuration);
  assert.ok(found, `configuration block ${id}`);
  return found[1].replaceAll("\r", "");
}
function retryBlock(id) {
  const found = new RegExp(`<!-- backup: ${id} -->\\r?\\n\\r?\\n\x60\x60\x60sh\\r?\\n([\\s\\S]*?)\x60\x60\x60`).exec(backup);
  assert.ok(found, `backup block ${id}`);
  return found[1].replaceAll("\r", "");
}

test("deploy_ac_021: all operator documents exist and their local document links resolve", () => {
  for (const name of ["README", "install", "configuration", "backup-and-restore", "upgrade", "data-protection", "forensic-verification"]) {
    const file = join(DOCS, `${name}.md`);
    assert.ok(existsSync(file), name);
    const text = readFileSync(file, "utf8");
    for (const [, target] of text.matchAll(/\[[^\]]+\]\(([^\s)]+)\)/g)) {
      if (/^[a-z]+:|^#/.test(target)) continue;
      assert.ok(existsSync(resolve(dirname(file), target.split("#")[0])), `${name}: ${target}`);
    }
  }
});

test("the documented TLS syslog override replaces json-file options on every service and bounds its local cache", () => {
  const dir = copyBundle();
  writeFileSync(join(dir, ".env"), readFileSync(join(dir, ".env.example")));
  setEnv(dir, { ...Object.fromEntries(SECRETS.map((s) => [s, "0".repeat(64)])),
    CG_DASHBOARD_HOSTNAME: "cg.example.test", CG_AGENT_SERVER_NAMES: "localhost" });
  writeFileSync(join(dir, "logging.yml"), block("logging", "yaml"));
  const rendered = JSON.parse(ok("docker", ["compose", "-f", "docker-compose.yml", "-f", "logging.yml", "config", "--format", "json"], { cwd: dir }));
  assert.equal(Object.keys(rendered.services).length, 9);
  for (const [name, service] of Object.entries(rendered.services)) {
    assert.equal(service.logging.driver, "syslog", name);
    const options = service.logging.options;
    assert.equal(options["syslog-address"], "tcp+tls://logs.example.net:6514");
    assert.equal(options["syslog-tls-skip-verify"], undefined);
    assert.equal(options["syslog-tls-ca-cert"], "/etc/docker/cyberguard-logging/collector-ca.pem");
    assert.equal(options["max-size"], undefined);
    assert.equal(options["max-file"], undefined);
    assert.equal(options["cache-disabled"], "false");
    assert.equal(options["cache-max-size"], "10m");
    assert.equal(options["cache-max-file"], "5");
  }
});

test("the documented syslog options deliver over verified TLS, keep local reads, and refuse another issuer or DNS name", {
  skip: !(IS_ROOT && process.env.CG_TEST_STACK === "1") && "set CG_TEST_STACK=1 as POSIX root",
  timeout: 60_000,
}, async () => {
  const dir = copyBundle();
  const other = copyBundle();
  const ca = makeDashboardTls(dir, "localhost");
  const otherCa = makeDashboardTls(other, "localhost");
  writeFileSync(join(dir, "collector-ca.pem"), ca);
  writeFileSync(join(dir, "other-ca.pem"), otherCa);
  writeFileSync(join(dir, ".env"), readFileSync(join(dir, ".env.example")));
  setEnv(dir, { ...Object.fromEntries(SECRETS.map((s) => [s, "0".repeat(64)])),
    CG_DASHBOARD_HOSTNAME: "cg.example.test", CG_AGENT_SERVER_NAMES: "localhost" });
  writeFileSync(join(dir, "logging.yml"), block("logging", "yaml"));
  const logging = JSON.parse(ok("docker", ["compose", "-f", "docker-compose.yml", "-f", "logging.yml", "config", "--format", "json"], { cwd: dir })).services.ingest.logging;
  const sockets = new Set();
  let received = "";
  const collector = createServer({ key: readFileSync(join(dir, "tls/dashboard.key")), cert: readFileSync(join(dir, "tls/dashboard.crt")) },
    (socket) => socket.on("data", (bytes) => { received += bytes.toString(); }));
  collector.on("connection", (socket) => { sockets.add(socket); socket.once("close", () => sockets.delete(socket)); });
  collector.on("tlsClientError", () => {}); // Expected for the two negative certificate cases.
  await new Promise((resolve, reject) => { collector.once("error", reject); collector.listen(0, "127.0.0.1", resolve); });
  const port = collector.address().port;
  const prefix = `cgsyslog${randomBytes(4).toString("hex")}`;
  const names = ["good", "issuer", "name"].map((suffix) => `${prefix}-${suffix}`);
  const options = { ...logging.options, "syslog-address": `tcp+tls://localhost:${port}`,
    "syslog-tls-ca-cert": join(dir, "collector-ca.pem") };
  // The collector runs in this Node process, so Docker must run asynchronously.
  const dockerAsync = (args) => new Promise((resolve, reject) => {
    const child = spawn("docker", args);
    let stdout = ""; let stderr = "";
    child.stdout.on("data", (b) => { stdout += b; });
    child.stderr.on("data", (b) => { stderr += b; });
    const timeout = setTimeout(() => child.kill("SIGKILL"), 15_000);
    child.once("error", (error) => { clearTimeout(timeout); reject(error); });
    child.once("close", (status) => { clearTimeout(timeout); resolve({ status, stdout, stderr }); });
  });
  const emit = (name, opts) => dockerAsync(["run", "--name", name, "--network", "none", "--log-driver", logging.driver,
    ...Object.entries(opts).flatMap(([key, value]) => ["--log-opt", `${key}=${value}`]),
    `cyberguard-ingest:${TAG}`, "node", "-e", 'process.stdout.write(process.argv[1]+"\\n")', name]);
  try {
    const good = await emit(names[0], options);
    assert.equal(good.status, 0, good.stderr);
    const until = Date.now() + 5000;
    while (!received.includes(names[0]) && Date.now() < until) await new Promise((r) => setTimeout(r, 20));
    assert.ok(received.includes(names[0]), "collector received the container's marker");
    const local = await dockerAsync(["logs", names[0]]);
    assert.equal(local.status, 0, local.stderr);
    assert.match(local.stdout, new RegExp(names[0]));
    const issuer = await emit(names[1], { ...options, "syslog-tls-ca-cert": join(dir, "other-ca.pem") });
    assert.notEqual(issuer.status, 0);
    assert.match(issuer.stderr, /certificate|x509/i);
    const name = await emit(names[2], { ...options, "syslog-address": `tcp+tls://127.0.0.1:${port}` });
    assert.notEqual(name.status, 0);
    assert.match(name.stderr, /certificate|x509/i);
    assert.ok(!received.includes(names[1]) && !received.includes(names[2]));
  } finally {
    for (const name of names) await dockerAsync(["rm", "-f", name]);
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve) => collector.close(resolve));
  }
});

test("the documented operator pair is placed, renewed and restored after recovery-volume removal", {
  skip: !(IS_ROOT && process.env.CG_TEST_STACK === "1") && "set CG_TEST_STACK=1 as POSIX root",
  timeout: 3 * 60_000,
}, async () => {
  const dir = copyBundle();
  const material = copyBundle();
  let operatorCa = makeDashboardTls(material, "operator.example.test");
  const certificate = join(material, "tls/dashboard.crt");
  const key = join(material, "tls/dashboard.key");
  let expected = new X509Certificate(readFileSync(certificate));
  renameSync(join(dir, "docker-compose.test.yml"), join(dir, "docker-compose.override.yml"));
  assert.equal(run("sh", ["setup.sh"], { cwd: dir }).status, 1);
  const enrollmentPort = await freePort();
  setEnv(dir, { COMPOSE_PROJECT_NAME: `cgpair${randomBytes(4).toString("hex")}`,
    CG_DASHBOARD_HOSTNAME: "cg.example.test", CG_AGENT_SERVER_NAMES: "operator.example.test", CG_BIND_ADDRESS: "127.0.0.1",
    CG_HTTP_PORT: String(await freePort()), CG_HTTPS_PORT: String(await freePort()),
    CG_ENROLL_PORT: String(enrollmentPort), CG_HEARTBEAT_PORT: String(await freePort()) });
  makeDashboardTls(dir, "cg.example.test");
  ok("sh", ["setup.sh"], { cwd: dir });
  try {
    const place = block("ingest-pair", "sh").replaceAll("<server-cert.pem>", certificate).replaceAll("<server-key.pem>", key);
    ok("sh", ["-c", place], { cwd: dir });
    const id = ok("docker", ["compose", "ps", "-aq", "ingest"], { cwd: dir });
    assert.equal(ok("docker", ["inspect", "--format", "{{.State.Running}}", id]), "false");
    const permissions = ok("docker", ["compose", "run", "--rm", "--no-deps", "--entrypoint", "stat", "ingest",
      "-c", "%u:%g:%a", "/certs/server.pem", "/certs/server-key.pem"], { cwd: dir });
    assert.equal(permissions, "1000:1000:644\n1000:1000:600");
    ok("docker", ["compose", "up", "-d", "--wait"], { cwd: dir });
    const checkPair = () => new Promise((resolve, reject) => {
      const socket = connect({ host: "127.0.0.1", port: enrollmentPort, servername: "operator.example.test", ca: operatorCa }, () => {
        try {
          assert.equal(socket.authorized, true);
          assert.deepEqual(socket.getPeerCertificate().raw, expected.raw);
          resolve();
        } catch (error) { reject(error); }
        finally { socket.destroy(); }
      });
      socket.setTimeout(5000, () => socket.destroy(new Error("operator pair probe timed out")));
      socket.once("error", reject);
    });
    await checkPair();
    // A different externally issued pair exercises the stopped renewal path.
    operatorCa = makeDashboardTls(material, "operator.example.test");
    expected = new X509Certificate(readFileSync(certificate));
    ok("sh", ["-c", place], { cwd: dir });
    ok("sh", ["-c", block("ingest-renew", "sh")], { cwd: dir });
    await checkPair();

    const internalCa = ok("docker", ["compose", "exec", "-T", "ingest", "node", "dist/cli/export-ca.js"], { cwd: dir });
    ok("sh", ["backup.sh"], { cwd: dir });
    const pg = readdirSync(join(dir, "backups")).find((f) => f.endsWith(".dump.gpg"));
    const stamp = /^cyberguard-postgres-(.+)\.dump\.gpg$/.exec(pg)?.[1];
    assert.ok(stamp);
    // Retrying after removal must replace the external pair before restore.sh's up.
    ok("sh", ["-c", retryBlock("retry-clear")], { cwd: dir });
    ok("sh", ["-c", place], { cwd: dir });
    const restore = retryBlock("retry-restore").replaceAll("<dir>", join(dir, "backups")).replaceAll("<stamp>", stamp);
    ok("sh", ["-c", restore], { cwd: dir });
    await checkPair();
    assert.equal(ok("docker", ["compose", "exec", "-T", "ingest", "node", "dist/cli/export-ca.js"], { cwd: dir }), internalCa);
  } finally {
    run("docker", ["compose", "down", "-v", "--remove-orphans"], { cwd: dir });
  }
});
