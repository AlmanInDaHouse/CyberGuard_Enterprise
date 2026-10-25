// SPEC-023 deploy_ac_003 and 006 to 012, against the bundle brought up as an operator does:
// setup.sh, `docker compose up --wait`, the first admin through create-user.js, with the
// images built from the commit (docker-compose.test.yml) and provided TLS from a test CA.
// It needs Docker, openssl and POSIX root (setup.sh's chown), and runs when CG_TEST_STACK=1,
// as deploy-ci.yml sets it; deploy_ac_009 and 012 also need a Linux debug build of cg-agent,
// named by CG_TEST_AGENT, and 012 gpg.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { X509Certificate, createHash, randomBytes, randomUUID } from "node:crypto";
import { chmodSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, describe, test } from "node:test";
import { connect as tlsConnect } from "node:tls";
import {
  IS_ROOT,
  TAG,
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
const AGENT = process.env.CG_TEST_AGENT ?? "";

let dir;
let project;
let env;
let caPem;
let ports;
let elapsedMs;
const admin = { email: "admin@cg.example.test", password: strongPassword(), secret: "" };
const usedCodes = new Set();
const temps = [];
// What deploy_ac_012 finds after a restore: the agent deploy_ac_009 enrolled, the incident
// deploy_ac_010 seeded, and the fingerprints both printed.
const prior = { agentHome: "", agentId: "", caFingerprint: "", forensicFingerprint: "", incident: "" };

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

/** The certificate a TLS listener on a published port presents. */
function peerCertificate(port) {
  return new Promise((resolve, reject) => {
    const socket = tlsConnect({ host: "127.0.0.1", port, servername: HOST, rejectUnauthorized: false }, () => {
      const cert = socket.getPeerCertificate();
      socket.destroy();
      resolve(cert);
    });
    socket.on("error", reject);
  });
}

/** A GET to the api from the dashboard's container, on the `app` network, with a session: its
 *  status, media type, for JSON its body, and the first five bytes of any other body. */
function apiGet(path, session) {
  const script = `fetch(new URL(process.argv[1], process.env.API_BASE_URL), { headers: { cookie: "cgsess=" + process.argv[2] } }).then(async (r) => {
    const type = (r.headers.get("content-type") ?? "").split(";")[0];
    const bytes = Buffer.from(await r.arrayBuffer());
    process.stdout.write(JSON.stringify({ status: r.status, type, body: type === "application/json" ? bytes.toString("utf8") : "", head: bytes.subarray(0, 5).toString("latin1") }));
  }, (e) => { process.stderr.write(String(e)); process.exit(1); })`;
  return JSON.parse(ok("docker", composeArgs(["exec", "-T", "dashboard", "node", "-e", script, path, session]), { cwd: dir }));
}

/** The agent started with `toml`: its standard error so far, and a stop that ends it. A
 *  binary that cannot start ends the run as an exit does. */
function startAgent(toml) {
  const child = spawn(AGENT, ["--config", toml], { stdio: ["ignore", "pipe", "pipe"] });
  const out = { stderr: "" };
  child.stderr.on("data", (d) => {
    out.stderr += d;
  });
  const exited = new Promise((resolve) => {
    child.on("exit", resolve);
    child.on("error", (e) => {
      out.stderr += `${e}`;
      resolve(null);
    });
  });
  out.stop = async () => {
    child.kill("SIGINT");
    await exited;
  };
  return out;
}

/** The agent id identity.json holds once the agent has enrolled; empty before, or while it is
 *  being written. */
function enrolledId(home) {
  try {
    return JSON.parse(readFileSync(join(home, "identity.json"), "utf8")).agent_id ?? "";
  } catch {
    return "";
  }
}

/** An agent's configuration, as the marquees write it, with its files under `home`. */
function agentToml(home, enrollUrl, heartbeatUrl, token) {
  return [
    "[server]",
    `url = ${JSON.stringify(enrollUrl)}`,
    `heartbeat_url = ${JSON.stringify(heartbeatUrl)}`,
    `trust_anchor_path = ${JSON.stringify(join(home, "ca.pem"))}`,
    "",
    "[agent]",
    'id = "01934abc-def0-7000-89ab-0000000000cc"',
    'hostname = "BUNDLE-TEST"',
    "",
    "[heartbeat]",
    "interval_seconds = 1",
    "request_timeout_seconds = 5",
    "max_retries = 3",
    "backoff_initial_ms = 100",
    "backoff_factor = 2.0",
    "backoff_max_ms = 1000",
    "",
    "[enrollment]",
    `token = ${JSON.stringify(token)}`,
    `cert_path = ${JSON.stringify(join(home, "cert.pem"))}`,
    `key_path = ${JSON.stringify(join(home, "key.dat"))}`,
    `identity_path = ${JSON.stringify(join(home, "identity.json"))}`,
    "timeout_seconds = 30",
    "max_retries = 3",
    "backoff_initial_ms = 100",
    "backoff_factor = 2.0",
    "",
    "[tls]",
    'minimum_version = "1.3"',
    "",
    "[envelope]",
    'canonical_form = "JCS"',
    "",
  ].join("\n");
}

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
    for (const t of temps) rmSync(t, { recursive: true, force: true });
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

  describe("deploy_ac_009: an agent enrolls and reports", { skip: !AGENT && "set CG_TEST_AGENT to a Linux debug build of cg-agent" }, () => {
    let caPemIngest;
    let token;

    test("export-ca.js prints the CA and the SHA-256 of its DER; issue-token.js prints a token", () => {
      const out = ok("docker", composeArgs(["exec", "-T", "ingest", "node", "dist/cli/export-ca.js"]), { cwd: dir });
      const m = /^(-----BEGIN CERTIFICATE-----\n[A-Za-z0-9+/=\n]+-----END CERTIFICATE-----)\nsha256:([0-9a-f]{64})$/.exec(out);
      assert.ok(m, out);
      caPemIngest = `${m[1]}\n`;
      prior.caFingerprint = `sha256:${m[2]}`;
      assert.equal(m[2], createHash("sha256").update(new X509Certificate(caPemIngest).raw).digest("hex"));
      token = ok("docker", composeArgs(["exec", "-T", "ingest", "node", "dist/cli/issue-token.js"]), { cwd: dir });
      assert.match(token, /^\S{16,}$/);
    });

    test("the certificate both listeners present names each entry of CG_AGENT_SERVER_NAMES", async () => {
      const certs = [];
      for (const port of [ports.enroll, ports.heartbeat]) {
        const cert = await peerCertificate(port);
        assert.deepEqual(cert.subjectaltname.split(", ").sort(), ["DNS:cg.example.test", "IP Address:127.0.0.1"], `port ${port}`);
        certs.push(cert.fingerprint256);
      }
      assert.equal(certs[0], certs[1], "the two listeners present one certificate");
    });

    test("the agent, trusting the exported CA, enrolls and has a heartbeat row within 60 seconds", async () => {
      assert.ok(caPemIngest && token, "export-ca.js and issue-token.js ran");
      const home = mkdtempSync(join(tmpdir(), "cg-agent-"));
      temps.push(home);
      writeFileSync(join(home, "ca.pem"), caPemIngest);
      // A name of CG_AGENT_SERVER_NAMES (its address) and the published ports.
      writeFileSync(join(home, "agent.toml"), agentToml(home, `https://127.0.0.1:${ports.enroll}`, `https://127.0.0.1:${ports.heartbeat}`, token));
      const start = Date.now();
      const agent = startAgent(join(home, "agent.toml"));
      try {
        let agentId = "";
        let rows = 0;
        let seenAt = 0;
        while (rows === 0 && Date.now() - start < 60_000) {
          await new Promise((r) => setTimeout(r, 1000));
          agentId ||= enrolledId(home);
          if (agentId) {
            const r = clickhouse("cg_admin", env.get("CG_CH_ADMIN_PASSWORD"), `SELECT count() FROM cyberguard.heartbeats WHERE agent_id = toUUID('${agentId}')`);
            rows = Number(r.stdout.trim() || 0);
            seenAt = Date.now();
          }
        }
        assert.ok(agentId, `the agent did not enroll: ${agent.stderr}`);
        prior.agentHome = home;
        prior.agentId = agentId;
        assert.ok(rows > 0, `no heartbeat row for ${agentId}: ${agent.stderr}`);
        // The row was there when the query began, at most 60 s in.
        const seconds = (seenAt - start) / 1000;
        assert.ok(seconds <= 60, `the heartbeat row came after ${seconds} s`);
        console.log(JSON.stringify({ event: "deploy_ac_009", agent_id: agentId, seconds_to_heartbeat_row: seconds }));
      } finally {
        await agent.stop();
      }
    });
  });

  describe("deploy_ac_010: the forensic anchor", () => {
    let fingerprint;
    let exportPath;

    test("forensic-key.js prints the key the forensic_key row holds and the SHA-256 of its 32 bytes", () => {
      const [key, fp] = ok("docker", composeArgs(["exec", "-T", "api", "node", "dist/cli/forensic-key.js"]), { cwd: dir }).split("\n");
      const stored = psql("SELECT encode(public_key, 'hex') FROM api.forensic_key WHERE id = 1");
      assert.match(key, /^[A-Za-z0-9_-]{43}$/, "the key in base64url, unpadded");
      const raw = Buffer.from(key, "base64url");
      assert.equal(raw.length, 32);
      assert.equal(raw.toString("hex"), stored);
      assert.equal(fp, `sha256:${createHash("sha256").update(raw).digest("hex")}`);
      fingerprint = fp;
      prior.forensicFingerprint = fp;
    });

    test("an export of a seeded incident, requested from the api on the app network with an admin's session, carries the fingerprint; the report answers", async () => {
      assert.ok(fingerprint, "forensic-key.js ran");
      const ids = { agent: randomUUID(), event: randomUUID(), alert: randomUUID(), incident: randomUUID() };
      psql(`INSERT INTO ingest.agents (agent_id, org_id, pubkey, cert_pem, expires_at) VALUES ('${ids.agent}', 'default', '\\x00', 'seeded', now() + interval '1 day')`);
      psql(
        `INSERT INTO ingest.alerts (alert_id, org_id, agent_id, title, severity_id, cg_detection_source, rule_id, source_events, heuristic_score, final_score, cg_mitre, dedup_key, event_time, status)
         VALUES ('${ids.alert}', 'default', '${ids.agent}', 'Office spawned a script host', 4, 'rule', 'rule.office_spawns_script_host', ARRAY['${ids.event}']::uuid[], 0.9, 0.9, '{"tactics":["execution"],"techniques":["T1059.001"]}', '${ids.agent}::rule::${ids.alert}', now(), 'new')`,
      );
      psql(
        `INSERT INTO ingest.incidents (incident_id, org_id, agent_id, status, title, severity_id, cg_mitre, alert_ids, grouping_key, window_start)
         VALUES ('${ids.incident}', 'default', '${ids.agent}', 'open', 'Execution activity', 4, '{"tactics":["execution"],"techniques":["T1059.001"]}', ARRAY['${ids.alert}']::uuid[], 'default::${ids.agent}::${ids.incident}', now())`,
      );
      const insert = clickhouse(
        "cg_ingest",
        env.get("CG_CH_INGEST_PASSWORD"),
        `INSERT INTO cyberguard.cges_events (org_id, agent_id, event_id, class_uid, activity_id, process_pid, process_uid, process_name, time) VALUES ('default', '${ids.agent}', '${ids.event}', 1007, 1, 4242, 'u', 'powershell.exe', now64(9))`,
      );
      assert.equal(insert.status, 0, insert.stderr);

      const signedIn = await login();
      assert.equal(signedIn.status, 200, signedIn.body);
      const session = cookiesOf(signedIn).get("__Host-cgsess");
      const doc = apiGet(`/v1/incidents/${ids.incident}/forensic-export`, session);
      assert.equal(doc.status, 200, doc.body);
      const exported = JSON.parse(doc.body);
      assert.equal(exported.forensic_pubkey_sha256, fingerprint);
      assert.equal(exported.events.length, 1);
      const report = apiGet(`/v1/incidents/${ids.incident}/report`, session);
      assert.equal(report.status, 200);
      assert.equal(report.type, "application/pdf");
      assert.equal(report.head, "%PDF-");

      prior.incident = ids.incident;
      const io = mkdtempSync(join(tmpdir(), "cg-export-"));
      temps.push(io);
      // Read by the api image's uid 1000.
      chmodSync(io, 0o755);
      exportPath = join(io, "export.json");
      writeFileSync(exportPath, doc.body);
    });

    test("verify-export.js, from the api's image with no store reachable: verified with the fingerprint, refused without, failed for another key, a changed event or a changed signature", () => {
      assert.ok(exportPath, "an export was written");
      const io = dirname(exportPath);
      const verify = (file, ...args) =>
        run("docker", ["run", "--rm", "--network", "none", "-v", `${io}:/in:ro`, `cyberguard-api:${TAG}`, "node", "dist/cli/verify-export.js", ...args, `/in/${file}`]);
      const original = JSON.parse(readFileSync(exportPath, "utf8"));

      const good = verify("export.json", "--fingerprint", fingerprint);
      assert.equal(good.status, 0, good.stderr);
      assert.equal(good.stdout.trim(), `verified: 1 events, chain head ${original.chain_root}`);

      assert.equal(verify("export.json").status, 2);

      const other = verify("export.json", "--fingerprint", `sha256:${"0".repeat(64)}`);
      assert.equal(other.status, 1);
      assert.match(`${other.stdout}${other.stderr}`, /the export's key does not match the recorded fingerprint/);

      const changedEvent = structuredClone(original);
      changedEvent.events[0].process_name = "cmd.exe";
      writeFileSync(join(io, "event.json"), JSON.stringify(changedEvent));
      const event = verify("event.json", "--fingerprint", fingerprint);
      assert.equal(event.status, 1);
      assert.match(`${event.stdout}${event.stderr}`, /the events do not give the export's chain head/);

      const changedSignature = structuredClone(original);
      const sig = Buffer.from(changedSignature.root_signature, "base64url");
      sig[0] ^= 0xff;
      changedSignature.root_signature = sig.toString("base64url");
      writeFileSync(join(io, "signature.json"), JSON.stringify(changedSignature));
      const signature = verify("signature.json", "--fingerprint", fingerprint);
      assert.equal(signature.status, 1);
      assert.match(`${signature.stdout}${signature.stderr}`, /the signature does not verify/);
    });
  });

  // Last: it restarts ingest with other periods.
  describe("deploy_ac_011: retention", () => {
    const ttlOf = (table) => {
      const r = clickhouse("cg_admin", env.get("CG_CH_ADMIN_PASSWORD"), `SHOW CREATE TABLE cyberguard.${table} FORMAT TabSeparatedRaw`);
      assert.equal(r.status, 0, r.stderr);
      return (/\nTTL (.*)\n/.exec(r.stdout)?.[1] ?? "").replace(/ DELETE/g, "");
    };
    const expected = (p, n, l, h) => ({
      cges_events: [
        `toDateTime(arrived_at) + toIntervalDay(${p}) WHERE class_uid = 1007`,
        `toDateTime(arrived_at) + toIntervalDay(${n}) WHERE class_uid = 4001`,
        `toDateTime(arrived_at) + toIntervalDay(${l}) WHERE class_uid = 3002`,
        `toDateTime(arrived_at) + toIntervalDay(${Math.max(p, n, l)}) WHERE class_uid NOT IN (1007, 4001, 3002)`,
      ].join(", "),
      heartbeats: `toDateTime(arrived_at) + toIntervalDay(${h})`,
    });
    const set = (p, n, l, h) =>
      setEnv(dir, { CG_RETENTION_PROCESS_DAYS: String(p), CG_RETENTION_NETWORK_DAYS: String(n), CG_RETENTION_LOGON_DAYS: String(l), CG_RETENTION_HEARTBEAT_DAYS: String(h) });
    const up = () => {
      const r = compose(["up", "-d", "--wait", "--wait-timeout", "300"]);
      assert.equal(r.status, 0, `${r.stdout}\n${r.stderr}\n${compose(["logs", "--no-color", "--tail", "40", "ingest-migrate", "ingest"]).stdout}`);
    };

    test("with 7, 8, 9 and 10 days the time-to-live says so; with other values at the next up, the new ones, and a row stored before whose arrival is past its new period is gone once merged", () => {
      // Stored under the defaults, 30 days after its arrival: kept for 365 days.
      const id = randomUUID();
      const old = clickhouse(
        "cg_ingest",
        env.get("CG_CH_INGEST_PASSWORD"),
        `INSERT INTO cyberguard.cges_events (org_id, agent_id, event_id, class_uid, activity_id, process_pid, process_uid, process_name, time, arrived_at) VALUES ('default', generateUUIDv4(), '${id}', 1007, 1, 1, 'u', 'n', now64(9), now64(3) - INTERVAL 30 DAY)`,
      );
      assert.equal(old.status, 0, old.stderr);
      const count = () =>
        clickhouse("cg_admin", env.get("CG_CH_ADMIN_PASSWORD"), `SELECT count() FROM cyberguard.cges_events WHERE event_id = toUUID('${id}')`).stdout.trim();
      assert.equal(count(), "1");

      set(7, 8, 9, 10);
      up();
      assert.deepEqual({ cges_events: ttlOf("cges_events"), heartbeats: ttlOf("heartbeats") }, expected(7, 8, 9, 10));
      // Past its new period of 7 days: deleted once the table has merged.
      const merged = clickhouse("cg_admin", env.get("CG_CH_ADMIN_PASSWORD"), "OPTIMIZE TABLE cyberguard.cges_events FINAL");
      assert.equal(merged.status, 0, merged.stderr);
      assert.equal(count(), "0");

      set(20, 21, 22, 23);
      up();
      assert.deepEqual({ cges_events: ttlOf("cges_events"), heartbeats: ttlOf("heartbeats") }, expected(20, 21, 22, 23));
    });

    test("a period of 6 or 3651 days stops ingest-migrate before it sets a time-to-live, and stops ingest, each naming the variable", () => {
      const before = { cges_events: ttlOf("cges_events"), heartbeats: ttlOf("heartbeats") };
      for (const [name, value] of [["CG_RETENTION_PROCESS_DAYS", "6"], ["CG_RETENTION_HEARTBEAT_DAYS", "3651"]]) {
        const variable = name.replace(/^CG_/, "INGEST_");
        const previous = readEnv(dir).get(name);
        setEnv(dir, { [name]: value });
        for (const service of ["ingest-migrate", "ingest"]) {
          const r = compose(["run", "--rm", "--no-deps", "-T", service], { timeout: 120_000 });
          assert.equal(r.status, 1, `${service} with ${name}=${value}: ${r.stdout}${r.stderr}`);
          assert.match(r.stderr, new RegExp(`fatal: invalid configuration: ${variable}: `), `${service}: ${r.stderr}`);
        }
        assert.deepEqual({ cges_events: ttlOf("cges_events"), heartbeats: ttlOf("heartbeats") }, before);
        setEnv(dir, { [name]: previous });
      }
    });
  });
  // Last: it replaces the stack by a new one, restored.
  describe("deploy_ac_012: backup and restore", { skip: !AGENT && "set CG_TEST_AGENT to a Linux debug build of cg-agent" }, () => {
    const scripts = { ...process.env, COMPOSE_PROJECT_NAME: project, COMPOSE_FILE: "docker-compose.yml:docker-compose.test.yml" };
    let files;

    test("backup.sh writes two encrypted files, mode 0700 directory, and leaves no archive on ClickHouse's backup disk", () => {
      assert.ok(prior.agentId && prior.incident, "an agent enrolled (deploy_ac_009) and an incident exists (deploy_ac_010)");
      scripts.COMPOSE_PROJECT_NAME = project;
      const r = run("sh", ["backup.sh"], { cwd: dir, env: scripts });
      assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
      const backups = join(dir, "backups");
      assert.equal(statSync(backups).mode & 0o777, 0o700);
      const names = readdirSync(backups).sort();
      assert.equal(names.length, 2, names.join(", "));
      const pg = names.find((n) => /^cyberguard-postgres-\d{8}T\d{6}Z\.dump\.gpg$/.test(n));
      const ch = names.find((n) => /^cyberguard-clickhouse-\d{8}T\d{6}Z\.zip\.gpg$/.test(n));
      assert.ok(pg && ch, names.join(", "));
      files = { pg: join(backups, pg), ch: join(backups, ch) };
      // No pg_dump header, no zip header: encrypted.
      assert.notEqual(readFileSync(files.pg).subarray(0, 5).toString("latin1"), "PGDMP");
      assert.notEqual(readFileSync(files.ch).subarray(0, 2).toString("latin1"), "PK");
      for (const file of [files.pg, files.ch]) {
        const other = run("gpg", ["--batch", "--quiet", "--no-symkey-cache", "--pinentry-mode", "loopback", "--passphrase", "not-the-passphrase", "--decrypt", "--output", "/dev/null", file]);
        assert.notEqual(other.status, 0, `${file} opened with another passphrase`);
      }
      // It prints no passphrase.
      assert.doesNotMatch(`${r.stdout}${r.stderr}`, new RegExp(env.get("CG_BACKUP_PASSPHRASE")));
      assert.equal(ok("docker", composeArgs(["exec", "-T", "clickhouse", "ls", "-A", "/backups"]), { cwd: dir }), "");
    });

    test("restore.sh refuses, before it changes anything, a manipulated archive and a stack whose stores are not empty", () => {
      assert.ok(files, "backup.sh ran");
      const running = () =>
        ["proxy", "dashboard", "api", "ingest"].every((name) => states().get(name)?.State === "running");
      // One byte changed in the middle of the encrypted archive: gpg finds it manipulated.
      const tampered = join(dirname(files.ch), "tampered.zip.gpg");
      const bytes = readFileSync(files.ch);
      bytes[bytes.length >> 1] ^= 0x01;
      writeFileSync(tampered, bytes);
      try {
        const bad = run("sh", ["restore.sh", files.pg, tampered], { cwd: dir, env: scripts });
        assert.equal(bad.status, 1, `${bad.stdout}${bad.stderr}`);
        assert.match(bad.stdout, /tampered\.zip\.gpg does not decrypt with this \.env's passphrase, or gpg finds it manipulated/);
        assert.ok(!`${bad.stdout}${bad.stderr}`.includes(env.get("CG_BACKUP_PASSPHRASE")), "restore.sh printed the passphrase");
        assert.ok(running(), "a service was stopped");
      } finally {
        rmSync(tampered, { force: true });
      }
      // The installation that was backed up, still running: its stores are not empty.
      const live = run("sh", ["restore.sh", files.pg, files.ch], { cwd: dir, env: scripts });
      assert.equal(live.status, 1, `${live.stdout}${live.stderr}`);
      assert.match(live.stdout, /Postgres is not empty \(\d+ tables in ingest and api\): restore into a new stack/);
      assert.ok(running(), "a service was stopped");
    });

    test("a new stack with the same .env and restore.sh: the services start, the fingerprints are those printed before, the agent's heartbeats are accepted, the incident is listed", async () => {
      assert.ok(files, "backup.sh ran");
      const down = compose(["down", "-v", "--remove-orphans"]);
      assert.equal(down.status, 0, down.stderr);
      const r = run("sh", ["restore.sh", files.pg, files.ch], { cwd: dir, env: scripts });
      assert.equal(r.status, 0, `${r.stdout}${r.stderr}${compose(["logs", "--no-color", "--tail", "40"]).stdout}`);
      assert.ok(!`${r.stdout}${r.stderr}`.includes(env.get("CG_BACKUP_PASSPHRASE")), "restore.sh printed the passphrase");
      assert.equal(ok("docker", composeArgs(["exec", "-T", "clickhouse", "ls", "-A", "/backups"]), { cwd: dir }), "", "an archive left on the backup disk");

      const s = states();
      for (const name of ["proxy", "dashboard", "api", "ingest", "postgres", "clickhouse", "redis"]) {
        assert.equal(s.get(name)?.Health, "healthy", name);
      }
      for (const name of ["ingest-migrate", "api-migrate"]) assert.equal(s.get(name)?.ExitCode, 0, name);

      const forensic = ok("docker", composeArgs(["exec", "-T", "api", "node", "dist/cli/forensic-key.js"]), { cwd: dir }).split("\n")[1];
      assert.equal(forensic, prior.forensicFingerprint);
      const ca = ok("docker", composeArgs(["exec", "-T", "ingest", "node", "dist/cli/export-ca.js"]), { cwd: dir }).split("\n").pop();
      assert.equal(ca, prior.caFingerprint);

      // The enrolled agent, with its certificate and no new enrollment.
      const heartbeats = () =>
        Number(clickhouse("cg_admin", env.get("CG_CH_ADMIN_PASSWORD"), `SELECT count() FROM cyberguard.heartbeats WHERE agent_id = toUUID('${prior.agentId}')`).stdout.trim() || 0);
      const restored = heartbeats();
      assert.ok(restored > 0, "the agent's heartbeats were restored");
      const agent = startAgent(join(prior.agentHome, "agent.toml"));
      try {
        const start = Date.now();
        while (Date.now() - start < 60_000 && heartbeats() <= restored) await new Promise((res) => setTimeout(res, 1000));
        assert.ok(heartbeats() > restored, `no heartbeat accepted after the restore: ${agent.stderr}`);
      } finally {
        await agent.stop();
      }

      const signedIn = await login();
      assert.equal(signedIn.status, 200, signedIn.body);
      const list = apiGet("/v1/incidents", cookiesOf(signedIn).get("__Host-cgsess"));
      assert.equal(list.status, 200, list.body);
      assert.ok(list.body.includes(prior.incident), `${prior.incident} not listed`);
    });
  });
});
