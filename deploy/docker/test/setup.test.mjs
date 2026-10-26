// SPEC-023 deploy_ac_002, the set-up script (§Operational §3), on copies of the bundle. It
// runs as root, as the runbook's installation does (deploy-ci.yml runs the bundle's tests
// under sudo); elsewhere it is skipped.
import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, test } from "node:test";
import { IS_ROOT, SECRETS, copyBundle, makeDashboardTls, readEnv, run, setEnv } from "./lib.mjs";

const PROXY_UID = 10001;
const setup = (dir) => run("sh", ["setup.sh"], { cwd: dir });
const mode = (file) => statSync(file).mode & 0o777;

/** A copy whose .env has the hostnames set (provided mode, with its TLS files). */
function ready() {
  const dir = copyBundle();
  assert.equal(setup(dir).status, 1);
  setEnv(dir, { CG_DASHBOARD_HOSTNAME: "cg.example.test", CG_AGENT_SERVER_NAMES: "cg.example.test,10.0.0.5" });
  makeDashboardTls(dir, "cg.example.test");
  return dir;
}

describe("deploy_ac_002 — setup.sh", { skip: !IS_ROOT && "needs POSIX root, as the installation does" }, () => {
  test("a first run copies .env.example to .env, mode 0600, and stops naming the hostnames", () => {
    const dir = copyBundle();
    const r = setup(dir);
    assert.notEqual(r.status, 0);
    assert.equal(readFileSync(join(dir, ".env"), "utf8"), readFileSync(join(dir, ".env.example"), "utf8"));
    assert.equal(mode(join(dir, ".env")), 0o600);
    for (const name of ["CG_DASHBOARD_HOSTNAME", "CG_AGENT_SERVER_NAMES", "CG_TLS_MODE"]) {
      assert.ok(r.stdout.includes(name), `${name} not named: ${r.stdout}`);
    }
  });

  test("a second run fills each secret with 64 distinct lowercase hexadecimal characters, naming none of them", () => {
    const dir = ready();
    const r = setup(dir);
    assert.equal(r.status, 0, r.stdout + r.stderr);
    const env = readEnv(dir);
    const values = SECRETS.map((name) => env.get(name));
    for (const [i, v] of values.entries()) assert.match(v ?? "", /^[0-9a-f]{64}$/, SECRETS[i]);
    assert.equal(new Set(values).size, SECRETS.length);
    for (const v of values) assert.ok(!(r.stdout + r.stderr).includes(v), "a generated value was printed");
    for (const name of SECRETS) assert.ok(r.stdout.includes(name), `${name} not named as filled`);
    assert.equal(mode(join(dir, ".env")), 0o600);
  });

  test("a COMPOSE_PROJECT_NAME line of .env is kept (the second Amendment 2026-10-26, item 3)", () => {
    const dir = ready();
    writeFileSync(join(dir, ".env"), `${readFileSync(join(dir, ".env"), "utf8")}COMPOSE_PROJECT_NAME=cgother\n`);
    assert.equal(setup(dir).status, 0);
    assert.equal(readEnv(dir).get("COMPOSE_PROJECT_NAME"), "cgother");
    assert.equal(readFileSync(join(dir, ".env"), "utf8").match(/^COMPOSE_PROJECT_NAME=/gm)?.length, 1);
  });

  test("a third run changes no byte", () => {
    const dir = ready();
    assert.equal(setup(dir).status, 0);
    const before = readFileSync(join(dir, ".env"));
    const r = setup(dir);
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.ok(readFileSync(join(dir, ".env")).equals(before));
  });

  test("a variable the template has and .env lacks is added with its default; a secret set is kept", () => {
    const dir = ready();
    const text = readFileSync(join(dir, ".env"), "utf8")
      .split("\n")
      .filter((l) => !l.startsWith("CG_RETENTION_LOGON_DAYS="))
      .join("\n");
    writeFileSync(join(dir, ".env"), text);
    setEnv(dir, { CG_PG_API_PASSWORD: "operator-chosen-value" });
    const r = setup(dir);
    assert.equal(r.status, 0, r.stdout + r.stderr);
    const env = readEnv(dir);
    assert.equal(env.get("CG_RETENTION_LOGON_DAYS"), "365");
    assert.equal(env.get("CG_PG_API_PASSWORD"), "operator-chosen-value");
  });

  test("an empty hostname, or provided mode without its TLS files, is refused, naming what is missing", () => {
    for (const [name, value] of [["CG_DASHBOARD_HOSTNAME", ""], ["CG_AGENT_SERVER_NAMES", ""]]) {
      const dir = ready();
      setEnv(dir, { [name]: value });
      const r = setup(dir);
      assert.notEqual(r.status, 0, name);
      assert.ok(r.stdout.includes(name), r.stdout);
      assert.equal(readEnv(dir).get("CG_PG_SUPERUSER_PASSWORD"), "", "a secret was filled");
    }
    for (const file of ["dashboard.crt", "dashboard.key"]) {
      const dir = ready();
      run("rm", ["-f", join(dir, "tls", file)]);
      const r = setup(dir);
      assert.notEqual(r.status, 0, file);
      assert.ok(r.stdout.includes(`tls/${file}`), r.stdout);
    }
  });

  test("a variable added to a .env whose last line lacks its newline goes on a line of its own, that line kept", () => {
    const dir = ready();
    assert.equal(setup(dir).status, 0);
    const lines = readFileSync(join(dir, ".env"), "utf8")
      .split("\n")
      .filter((l) => l !== "" && !l.startsWith("CG_LOG_LEVEL="));
    const last = lines[lines.length - 1];
    assert.match(last, /^CG_BACKUP_PASSPHRASE=[0-9a-f]{64}$/);
    writeFileSync(join(dir, ".env"), lines.join("\n"));
    assert.equal(setup(dir).status, 0);
    const after = readFileSync(join(dir, ".env"), "utf8").split("\n");
    assert.ok(after.includes(last), "the last line changed");
    assert.ok(after.includes("CG_LOG_LEVEL=info"), "CG_LOG_LEVEL not added on a line of its own");
  });

  test("a refused run changes nothing in .env", () => {
    const dir = ready();
    setEnv(dir, { CG_AGENT_SERVER_NAMES: "" });
    writeFileSync(
      join(dir, ".env"),
      readFileSync(join(dir, ".env"), "utf8")
        .split("\n")
        .filter((l) => !l.startsWith("CG_LOG_LEVEL="))
        .join("\n"),
    );
    const before = readFileSync(join(dir, ".env"));
    assert.notEqual(setup(dir).status, 0);
    assert.ok(readFileSync(join(dir, ".env")).equals(before));
  });

  test("an empty or a quoted CG_TLS_MODE is read as the compose reads it: provided", () => {
    for (const value of ["", '"provided"', "'provided'"]) {
      const dir = ready();
      setEnv(dir, { CG_TLS_MODE: value });
      run("rm", ["-f", join(dir, "tls", "dashboard.key")]);
      const r = setup(dir);
      assert.notEqual(r.status, 0, value);
      assert.ok(r.stdout.includes("tls/dashboard.key"), `${value}: ${r.stdout}`);
    }
    const dir = ready();
    setEnv(dir, { CG_TLS_MODE: '"provided"' });
    assert.equal(setup(dir).status, 0);
    assert.equal(statSync(join(dir, "tls", "dashboard.key")).uid, PROXY_UID);
  });

  test("in provided mode the key is the proxy's user's, mode 0400; internal mode needs no TLS file", () => {
    const dir = ready();
    assert.equal(setup(dir).status, 0);
    const key = statSync(join(dir, "tls", "dashboard.key"));
    assert.equal(key.uid, PROXY_UID);
    assert.equal(key.mode & 0o777, 0o400);

    const internal = copyBundle();
    assert.equal(setup(internal).status, 1);
    setEnv(internal, { CG_DASHBOARD_HOSTNAME: "cg.example.test", CG_AGENT_SERVER_NAMES: "cg.example.test", CG_TLS_MODE: "internal" });
    const r = setup(internal);
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.ok(!existsSync(join(internal, "tls", "dashboard.key")));
  });
});
