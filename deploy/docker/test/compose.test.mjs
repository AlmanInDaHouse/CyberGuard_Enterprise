// SPEC-023 deploy_ac_001, the compose's contract, over `docker compose config
// --no-interpolate` of the bundle as the repository holds it (§Operational §1, §Data
// contracts §2), with its Amendment 2026-10-25 (items 5 and 6). The published bundle's
// digests are the release dry run's; the Dockerfiles' pins are images.test.mjs's.
import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "node:test";
import { BUNDLE, SECRETS, ok } from "./lib.mjs";

const config = JSON.parse(
  ok("docker", ["compose", "-f", join(BUNDLE, "docker-compose.yml"), "config", "--no-interpolate", "--format", "json"], {
    cwd: BUNDLE,
  }),
);
const services = config.services;

const LONG_RUNNING = ["proxy", "dashboard", "api", "ingest", "postgres", "clickhouse", "redis"];
const ONE_SHOTS = ["ingest-migrate", "api-migrate"];
const HARDENED = ["proxy", "dashboard", "api", "ingest", ...ONE_SHOTS];
const STORES = ["postgres", "clickhouse", "redis"];

/** A port's published form, `host_ip:published:target`, as the compose writes it. */
function ports(name) {
  return (services[name].ports ?? []).map((p) => {
    if (typeof p === "string") {
      // A `${…}` holds colons of its own (`:-`), so the three parts are read whole.
      const m = /^(\$\{[^}]*\}|[^:]*):(\$\{[^}]*\}|[0-9]+):([0-9]+)$/.exec(p);
      assert.ok(m, `a port the test cannot read: ${p}`);
      return { host_ip: m[1], published: m[2], target: m[3] };
    }
    return { target: String(p.target), published: String(p.published), host_ip: p.host_ip };
  });
}

/** Every environment value of a service, as one map. */
const env = (name) => services[name].environment ?? {};

test("the project is cyberguard-enterprise (Amendment 2026-10-25, item 5)", () => {
  assert.equal(config.name, "cyberguard-enterprise");
});

test("the seven services and the two one-shots, and no other", () => {
  assert.deepEqual(Object.keys(services).sort(), [...LONG_RUNNING, ...ONE_SHOTS].sort());
});

test("published ports: the proxy's two and ingest's two, each on CG_BIND_ADDRESS", () => {
  const published = Object.keys(services).flatMap((name) => ports(name).map((p) => ({ name, ...p })));
  assert.deepEqual(
    published.map((p) => `${p.name} ${p.published}->${p.target}`).sort(),
    [
      "ingest ${CG_ENROLL_PORT:-8444}->8080",
      "ingest ${CG_HEARTBEAT_PORT:-8443}->8443",
      "proxy ${CG_HTTPS_PORT:-443}->8443",
      "proxy ${CG_HTTP_PORT:-80}->8080",
    ],
  );
  for (const p of published) assert.equal(p.host_ip, "${CG_BIND_ADDRESS:-0.0.0.0}", `${p.name}`);
});

test("edge, app and data are internal; public and agents are not", () => {
  for (const n of ["edge", "app", "data"]) assert.equal(config.networks[n]?.internal, true, n);
  for (const n of ["public", "agents"]) assert.notEqual(config.networks[n]?.internal, true, n);
});

test("the api on no non-internal network, the dashboard not on data, the one-shots on data alone", () => {
  const nets = (name) => Object.keys(services[name].networks ?? {});
  for (const n of nets("api")) assert.equal(config.networks[n].internal, true, `api on ${n}`);
  assert.ok(!nets("dashboard").includes("data"));
  for (const name of ONE_SHOTS) assert.deepEqual(nets(name), ["data"], name);
  assert.deepEqual(nets("proxy").sort(), ["edge", "public"]);
  assert.deepEqual(nets("ingest").sort(), ["agents", "data"]);
});

test("the dashboard's fixed address on app is the one API_TRUSTED_PROXIES names, outside the range Docker hands out", () => {
  const address = services.dashboard.networks.app?.ipv4_address;
  assert.ok(address);
  assert.equal(env("api").API_TRUSTED_PROXIES, address);
  // In the subnet, and not in its dynamic range, which the api, joining first, draws from.
  const [{ subnet, ip_range: range }] = config.networks.app.ipam.config;
  const toInt = (ip) => ip.split(".").reduce((n, o) => n * 256 + Number(o), 0);
  const within = (ip, cidr) => {
    const [base, bits] = cidr.split("/");
    const size = 2 ** (32 - Number(bits));
    return toInt(ip) >= toInt(base) && toInt(ip) < toInt(base) + size;
  };
  assert.ok(within(address, subnet), `${address} not in ${subnet}`);
  assert.ok(range, "app has no ip_range");
  assert.ok(!within(address, range), `${address} in the dynamic range ${range}`);
});

test("ingest, the api, the dashboard, the proxy and the one-shots: non-root, read-only, no capability, no new privileges", () => {
  for (const name of HARDENED) {
    const s = services[name];
    assert.match(s.user ?? "", /^[1-9][0-9]*(:[1-9][0-9]*)?$/, `${name} user`);
    assert.equal(s.read_only, true, `${name} read_only`);
    assert.deepEqual(s.cap_drop, ["ALL"], `${name} cap_drop`);
    assert.ok((s.security_opt ?? []).includes("no-new-privileges:true"), `${name} security_opt`);
    assert.ok((s.tmpfs ?? []).includes("/tmp"), `${name} tmpfs /tmp`);
  }
});

test("log rotation on every service; a health check and a restart policy on every long-running one", () => {
  for (const [name, s] of Object.entries(services)) {
    assert.equal(s.logging?.driver, "json-file", `${name} logging`);
    assert.equal(s.logging?.options?.["max-size"], "10m", `${name} max-size`);
    assert.equal(s.logging?.options?.["max-file"], "5", `${name} max-file`);
  }
  for (const name of LONG_RUNNING) {
    assert.ok(services[name].healthcheck?.test?.length, `${name} health check`);
    assert.equal(services[name].restart, "unless-stopped", `${name} restart`);
  }
  for (const name of ONE_SHOTS) {
    assert.equal(services[name].restart, "no", `${name} restart`);
    // The image's health check, the service's, is turned off.
    assert.equal(services[name].healthcheck?.disable, true, `${name} health check`);
  }
});

test("the stores drop NET_RAW, and run their publishers' images pinned by digest", () => {
  for (const name of STORES) assert.ok((services[name].cap_drop ?? []).includes("NET_RAW"), name);
  assert.match(services.postgres.image, /^pgvector\/pgvector:pg16@sha256:[0-9a-f]{64}$/);
  assert.match(services.clickhouse.image, /^clickhouse\/clickhouse-server:24\.8\.14\.39@sha256:[0-9a-f]{64}$/);
  assert.match(services.redis.image, /^valkey\/valkey:9\.1\.2-alpine@sha256:[0-9a-f]{64}$/);
});

test("the product images are named by tag in the repository", () => {
  for (const [name, image] of [
    ["proxy", "proxy"], ["dashboard", "dashboard"], ["api", "api"], ["api-migrate", "api"],
    ["ingest", "ingest"], ["ingest-migrate", "ingest"],
  ]) {
    assert.match(services[name].image, new RegExp(`^ghcr\\.io/almanindahouse/cyberguard-${image}:[^@]+$`), name);
  }
});

test("no service sets INGEST_ENROLL_TLS=false", () => {
  for (const [name] of Object.entries(services)) {
    assert.notEqual(String(env(name).INGEST_ENROLL_TLS ?? ""), "false", name);
  }
});

test("every generated secret a container uses is required (${CG_…:?}); CG_BACKUP_PASSPHRASE is in no service", () => {
  // Every string of every service, its environment, command, entrypoint and health check.
  const strings = (v) =>
    typeof v === "string" ? [v] : v && typeof v === "object" ? Object.values(v).flatMap(strings) : [];
  const values = Object.values(services).flatMap(strings);
  for (const secret of SECRETS) {
    const uses = values.filter((v) => v.includes(`\${${secret}`));
    if (secret === "CG_BACKUP_PASSPHRASE") {
      assert.deepEqual(uses, [], secret);
      continue;
    }
    assert.ok(uses.length > 0, `${secret} is used by no container`);
    for (const v of uses) {
      const forms = [...v.matchAll(new RegExp(`\\$\\{${secret}([^}]*)\\}`, "g"))].map((m) => m[1]);
      for (const form of forms) assert.match(form, /^:\?/, `${secret} written \${${secret}${form}}`);
    }
  }
});

test("CG_SMTP_PASS reaches ingest as ${CG_SMTP_PASS}, with no default", () => {
  assert.equal(env("ingest").INGEST_SMTP_PASS, "${CG_SMTP_PASS}");
});

test("each service connects as its runtime credential, each one-shot as its owner (Amendment 2026-10-25, item 6)", () => {
  // Each user with its own password.
  const url = (user, secret) => new RegExp(`^[a-z]+://${user}:\\$\\{${secret}:\\?`);
  assert.match(env("ingest").INGEST_PG_URL, url("cg_ingest", "CG_PG_INGEST_PASSWORD"));
  assert.match(env("ingest-migrate").INGEST_PG_URL, url("cg_ingest_owner", "CG_PG_INGEST_OWNER_PASSWORD"));
  assert.match(env("api").API_PG_URL, url("cg_api", "CG_PG_API_PASSWORD"));
  assert.match(env("api-migrate").API_PG_URL, url("cg_api_owner", "CG_PG_API_OWNER_PASSWORD"));
  assert.equal(env("ingest").INGEST_CH_USER, "cg_ingest");
  assert.match(env("ingest").INGEST_CH_PASSWORD, /^\$\{CG_CH_INGEST_PASSWORD:\?/);
  assert.equal(env("ingest-migrate").INGEST_CH_USER, "cg_admin");
  assert.match(env("ingest-migrate").INGEST_CH_PASSWORD, /^\$\{CG_CH_ADMIN_PASSWORD:\?/);
  assert.equal(env("api").API_CH_USER, "cg_api");
  assert.match(env("api").API_CH_PASSWORD, /^\$\{CG_CH_API_PASSWORD:\?/);
  assert.match(env("ingest").INGEST_REDIS_URL, url("cg_ingest", "CG_REDIS_INGEST_PASSWORD"));
  assert.match(env("api").API_REDIS_URL, url("cg_api", "CG_REDIS_API_PASSWORD"));
  assert.equal(env("ingest").INGEST_PG_URL.includes("search_path%3Dingest%2Cpublic"), true);
  assert.equal(env("api").API_PG_URL.includes("search_path%3Dapi%2Cpublic"), true);
});
