// SPEC-023 deploy_ac_003, 006, 007 and 008, against the bundle brought up as an operator
// does: setup.sh, `docker compose up --wait`, the first admin through create-user.js, with
// the images built from the commit (docker-compose.test.yml) and provided TLS from a test CA.
// It needs Docker, openssl and POSIX root (setup.sh's chown), and runs when CG_TEST_STACK=1,
// as deploy-ci.yml sets it.
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";
import { after, before, describe, test } from "node:test";
import {
  IS_ROOT,
  copyBundle,
  freePort,
  freshCode,
  interactive,
  makeDashboardTls,
  ok,
  readEnv,
  run,
  setEnv,
  strongPassword,
} from "./lib.mjs";

const ENABLED = process.env.CG_TEST_STACK === "1" && IS_ROOT;
const HOST = "cg.example.test";
const BUDGET_MS = 10 * 60_000;

let dir;
let project;
let env;
let caPem;
let ports;
let elapsedMs;
const admin = { email: "admin@cg.example.test", password: strongPassword(), secret: "" };
const usedCodes = new Set();

const composeArgs = (args) => ["compose", "-p", project, "-f", "docker-compose.yml", "-f", "docker-compose.test.yml", ...args];
const compose = (args, opts = {}) => run("docker", composeArgs(args), { cwd: dir, ...opts });

/** The services' states, from `docker compose ps -a --format json` (an array or one object a line). */
function states() {
  const out = compose(["ps", "-a", "--format", "json"]).stdout.trim();
  const rows = out.startsWith("[") ? JSON.parse(out) : out.split("\n").filter(Boolean).map((l) => JSON.parse(l));
  return new Map(rows.map((r) => [r.Service, r]));
}

function psql(sql) {
  return ok("docker", composeArgs(["exec", "-T", "postgres", "psql", "-U", "postgres", "-d", "cyberguard", "-Atc", sql]), { cwd: dir });
}

function valkey(user, password, ...command) {
  const r = compose(["exec", "-T", "-e", `VALKEYCLI_AUTH=${password}`, "redis", "valkey-cli", "--user", user, ...command]);
  return `${r.stdout}${r.stderr}`.trim();
}

function clickhouse(user, password, query) {
  return compose(["exec", "-T", "clickhouse", "clickhouse-client", "--user", user, "--password", password, "-q", query]);
}

/** A request to the proxy's HTTPS port, as a browser reaching CG_DASHBOARD_HOSTNAME sends it. */
function https(method, path, { headers = {}, body } = {}) {
  return new Promise((resolve, reject) => {
    const req = httpsRequest(
      {
        host: "127.0.0.1",
        port: ports.https,
        servername: HOST,
        ca: caPem,
        method,
        path,
        headers: { Host: `${HOST}:${ports.https}`, ...headers },
      },
      (res) => {
        let text = "";
        res.setEncoding("utf8");
        res.on("data", (c) => {
          text += c;
        });
        res.on("end", () => resolve({ status: res.statusCode, headers: res.headers, body: text }));
      },
    );
    req.on("error", reject);
    if (body !== undefined) req.write(body);
    req.end();
  });
}

function http(path, headers) {
  return new Promise((resolve, reject) => {
    const req = httpRequest({ host: "127.0.0.1", port: ports.http, path, headers }, (res) => {
      res.resume();
      res.on("end", () => resolve({ status: res.statusCode, headers: res.headers }));
    });
    req.on("error", reject);
    req.end();
  });
}

const origin = () => `https://${HOST}:${ports.https}`;

/** A sign-in through the proxy; `code` defaults to a fresh TOTP code. */
async function login(extraHeaders = {}, originHeader = origin(), code = undefined) {
  const totp_code = code ?? (await freshCode(admin.secret, usedCodes));
  return https("POST", "/api/auth/login", {
    headers: { Origin: originHeader, "Content-Type": "application/json", ...extraHeaders },
    body: JSON.stringify({ email: admin.email, password: admin.password, totp_code }),
  });
}

/** The cookies a response sets, by name. */
function cookiesOf(res) {
  const out = new Map();
  for (const line of res.headers["set-cookie"] ?? []) {
    const [pair] = line.split(";");
    const eq = pair.indexOf("=");
    out.set(pair.slice(0, eq), pair.slice(eq + 1));
  }
  return out;
}

const nonceOf = (csp) => /'nonce-([^']+)'/.exec(csp ?? "")?.[1];

describe("the bundle's stack", { skip: !ENABLED && "set CG_TEST_STACK=1, as root, with the images built" }, () => {
  before(async () => {
    const start = Date.now();
    dir = copyBundle();
    assert.equal(run("sh", ["setup.sh"], { cwd: dir }).status, 1);
    ports = { https: await freePort(), http: await freePort(), enroll: await freePort(), heartbeat: await freePort() };
    setEnv(dir, {
      CG_DASHBOARD_HOSTNAME: HOST,
      CG_AGENT_SERVER_NAMES: `${HOST},127.0.0.1`,
      CG_BIND_ADDRESS: "127.0.0.1",
      CG_HTTPS_PORT: String(ports.https),
      CG_HTTP_PORT: String(ports.http),
      CG_ENROLL_PORT: String(ports.enroll),
      CG_HEARTBEAT_PORT: String(ports.heartbeat),
    });
    caPem = makeDashboardTls(dir, HOST);
    ok("sh", ["setup.sh"], { cwd: dir });
    env = readEnv(dir);
    project = `cgtest${randomBytes(4).toString("hex")}`;

    const up = compose(["up", "-d", "--wait", "--wait-timeout", "600"]);
    if (up.status !== 0) {
      const logs = compose(["logs", "--no-color", "--tail", "60"]);
      assert.fail(`docker compose up --wait: ${up.status}\n${up.stdout}\n${up.stderr}\n${logs.stdout}`);
    }

    // The first admin, as the runbook creates it: the password on standard input, the TOTP
    // factor confirmed at the command.
    const cli = interactive("docker", composeArgs(["exec", "-T", "api", "node", "dist/cli/create-user.js", "--email", admin.email]), { cwd: dir });
    cli.write(admin.password);
    const [uri] = await cli.waitFor(/otpauth:\/\/\S+/);
    admin.secret = new URL(uri).searchParams.get("secret") ?? "";
    cli.write(await freshCode(admin.secret, usedCodes));
    cli.child.stdin.end();
    assert.equal(await cli.exited, 0, `${cli.stdout}\n${cli.stderr}`);
    elapsedMs = Date.now() - start;
    console.log(JSON.stringify({ event: "deploy_ac_003", elapsed_seconds: elapsedMs / 1000, budget_seconds: BUDGET_MS / 1000 }));
  });

  after(() => {
    if (project && dir) compose(["down", "-v", "--remove-orphans"]);
  });

  test("deploy_ac_003: the seven containers healthy, the migrations applied, the admin written, within 10 minutes", () => {
    const s = states();
    for (const name of ["proxy", "dashboard", "api", "ingest", "postgres", "clickhouse", "redis"]) {
      assert.equal(s.get(name)?.State, "running", name);
      assert.equal(s.get(name)?.Health, "healthy", name);
    }
    for (const name of ["ingest-migrate", "api-migrate"]) {
      assert.equal(s.get(name)?.State, "exited", name);
      assert.equal(s.get(name)?.ExitCode, 0, name);
    }
    const [ingestMigrations, apiMigrations, enrolled] = psql(
      `SELECT (SELECT count(*) FROM ingest.kysely_migration), (SELECT count(*) FROM api.api_kysely_migration), (SELECT totp_enrolled FROM api.users WHERE email = '${admin.email}')`,
    ).split("|");
    assert.ok(Number(ingestMigrations) > 0, "ingest's migrations");
    assert.ok(Number(apiMigrations) > 0, "the api's migrations");
    assert.equal(enrolled, "t", "the admin's TOTP factor confirmed");
    assert.ok(elapsedMs < BUDGET_MS, `${elapsedMs} ms`);
  });

  test("deploy_ac_006: each Redis user reaches its own keys and commands alone; default cannot authenticate", () => {
    const ingestPw = env.get("CG_REDIS_INGEST_PASSWORD");
    const apiPw = env.get("CG_REDIS_API_PASSWORD");
    assert.equal(valkey("cg_ingest", ingestPw, "SET", "nonce:x", "1"), "OK");
    assert.match(valkey("cg_ingest", ingestPw, "GET", "cgsess:x"), /NOPERM/);
    assert.match(valkey("cg_ingest", ingestPw, "SET", "cgsess:x", "forged"), /NOPERM/);
    assert.equal(valkey("cg_api", apiPw, "SET", "cgsess:x", "session"), "OK");
    assert.equal(valkey("cg_api", apiPw, "GET", "cgsess:x"), "session");
    assert.match(valkey("cg_api", apiPw, "SET", "nonce:x", "1"), /NOPERM/);
    for (const [user, pw] of [["cg_ingest", ingestPw], ["cg_api", apiPw]]) {
      for (const command of [["KEYS", "*"], ["FLUSHALL"], ["CONFIG", "GET", "maxmemory"], ["CLIENT", "KILL", "ID", "999999"], ["CLIENT", "PAUSE", "1"]]) {
        assert.match(valkey(user, pw, ...command), /NOPERM/, `${user} ${command.join(" ")}`);
      }
    }
    const asDefault = compose(["exec", "-T", "-e", "VALKEYCLI_AUTH=", "redis", "valkey-cli", "--user", "default", "--pass", "anything", "PING"]);
    assert.match(`${asDefault.stdout}${asDefault.stderr}`, /WRONGPASS/);
    assert.doesNotMatch(asDefault.stdout, /PONG/);
  });

  test("deploy_ac_007: the api reads class 1007 alone and writes nothing; ingest reads every class", () => {
    const ids = { 1007: crypto.randomUUID(), 4001: crypto.randomUUID(), 3002: crypto.randomUUID() };
    const rows = Object.entries(ids)
      .map(([cls, id]) => `(generateUUIDv4(), '${id}', ${cls}, 1, 1, 'u', 'n', now64(9))`)
      .join(", ");
    const ingestPw = env.get("CG_CH_INGEST_PASSWORD");
    const apiPw = env.get("CG_CH_API_PASSWORD");
    const adminPw = env.get("CG_CH_ADMIN_PASSWORD");
    const insert = clickhouse("cg_ingest", ingestPw, `INSERT INTO cyberguard.cges_events (agent_id, event_id, class_uid, activity_id, process_pid, process_uid, process_name, time) VALUES ${rows}`);
    assert.equal(insert.status, 0, insert.stderr);
    const which = `SELECT class_uid FROM cyberguard.cges_events WHERE toString(event_id) IN ('${Object.values(ids).join("','")}') ORDER BY class_uid`;
    const asApi = clickhouse("cg_api", apiPw, which);
    assert.equal(asApi.status, 0, asApi.stderr);
    assert.equal(asApi.stdout.trim(), "1007");
    const asIngest = clickhouse("cg_ingest", ingestPw, which);
    assert.equal(asIngest.stdout.trim().split("\n").join(","), "1007,3002,4001");
    const apiInsert = clickhouse("cg_api", apiPw, `INSERT INTO cyberguard.cges_events (agent_id, event_id, class_uid, activity_id, process_pid, process_uid, process_name, time) VALUES (generateUUIDv4(), generateUUIDv4(), 1007, 1, 1, 'u', 'n', now64(9))`);
    assert.notEqual(apiInsert.status, 0, "cg_api inserted a row");
    assert.match(apiInsert.stderr, /READONLY|ACCESS_DENIED/, apiInsert.stderr);
    const heartbeats = clickhouse("cg_api", apiPw, "SELECT count() FROM cyberguard.heartbeats");
    assert.notEqual(heartbeats.status, 0, "cg_api read heartbeats");
    assert.match(heartbeats.stderr, /ACCESS_DENIED/, heartbeats.stderr);
    // cges_events alone: not the system log tables, which hold other users' queries, and
    // which the administrator reads. (ClickHouse leaves system.tables, columns and databases
    // to every user, filtered to what that user may see.)
    assert.equal(clickhouse("cg_admin", adminPw, "SYSTEM FLUSH LOGS").status, 0);
    for (const table of ["system.query_log", "system.text_log"]) {
      assert.equal(clickhouse("cg_admin", adminPw, `EXISTS TABLE ${table}`).stdout.trim(), "1", table);
      const r = clickhouse("cg_api", apiPw, `SELECT count() FROM ${table}`);
      assert.notEqual(r.status, 0, `cg_api read ${table}`);
      assert.match(r.stderr, /ACCESS_DENIED/, `${table}: ${r.stderr}`);
    }
  });

  describe("deploy_ac_008: the edge and a sign-in", () => {
    test("HTTPS answers with HSTS and the dashboard's headers; HTTP redirects to HTTPS on the published port", async () => {
      const first = await https("GET", "/login");
      const second = await https("GET", "/login");
      assert.equal(first.status, 200);
      assert.equal(first.headers["strict-transport-security"], "max-age=31536000");
      const nonce = nonceOf(first.headers["content-security-policy"]);
      assert.ok(nonce);
      assert.notEqual(nonceOf(second.headers["content-security-policy"]), nonce);
      assert.equal(
        first.headers["content-security-policy"],
        `default-src 'self'; script-src 'self' 'nonce-${nonce}' 'strict-dynamic'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'`,
      );
      assert.equal(first.headers["x-content-type-options"], "nosniff");
      assert.equal(first.headers["referrer-policy"], "same-origin");
      assert.equal(first.headers["permissions-policy"], "camera=(), microphone=(), geolocation=()");
      const scripts = [...first.body.matchAll(/<script\b[^>]*>/g)].map((m) => m[0]);
      assert.ok(scripts.length > 0);
      for (const tag of scripts) assert.ok(tag.includes(`nonce="${nonce}"`), tag);

      const redirect = await http("/incidents?x=1", { Host: HOST });
      assert.equal(redirect.status, 308);
      assert.equal(redirect.headers.location, `https://${HOST}:${ports.https}/incidents?x=1`);
    });

    test("only the proxy's and ingest's ports are published on a host address", () => {
      const s = states();
      for (const name of ["api", "dashboard", "postgres", "clickhouse", "redis"]) {
        const published = (s.get(name)?.Publishers ?? []).filter((p) => p.PublishedPort);
        assert.deepEqual(published, [], name);
      }
    });

    test("the admin signs in: 200, the __Host- cookies, and the incidents page answers with them", async () => {
      const res = await login();
      assert.equal(res.status, 200, res.body);
      const cookies = cookiesOf(res);
      assert.ok(cookies.get("__Host-cgsess"));
      assert.ok(cookies.get("__Host-cgcsrf"));
      const incidents = await https("GET", "/incidents", {
        headers: { Cookie: `__Host-cgsess=${cookies.get("__Host-cgsess")}; __Host-cgcsrf=${cookies.get("__Host-cgcsrf")}` },
      });
      assert.equal(incidents.status, 200);
    });

    test("a sign-in carrying X-Forwarded-For is recorded with the address the proxy saw, not the header's", async () => {
      const res = await login({ "X-Forwarded-For": "203.0.113.9" });
      assert.equal(res.status, 200, res.body);
      const recorded = psql("SELECT host(source_ip) FROM api.audit_log WHERE action = 'auth.login.ok' ORDER BY id DESC LIMIT 1");
      assert.notEqual(recorded, "203.0.113.9");
      // The proxy's access log names the client it saw for each sign-in.
      const seen = compose(["logs", "--no-color", "--no-log-prefix", "proxy"])
        .stdout.split("\n")
        .filter((l) => l.includes('"uri":"/api/auth/login"'))
        .map((l) => JSON.parse(l.slice(l.indexOf("{"))).request.remote_ip);
      assert.ok(seen.includes(recorded), `${recorded} not among ${seen.join(", ")}`);
    });

    test("a sign-in from another origin is refused, the proxy having set X-Forwarded-Host to the request's Host", async () => {
      // Refused before the code is read: any code does.
      const res = await login({ "X-Forwarded-Host": "evil.example" }, "https://evil.example", "000000");
      assert.equal(res.status, 403);
    });
  });
});
