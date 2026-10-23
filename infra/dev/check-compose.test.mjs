// check-compose.mjs against compose renders it must refuse, and one it must accept.
// Run with `node --test infra/dev/check-compose.test.mjs` (infra-ci.yml runs it).
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const CHECK = path.join(path.dirname(fileURLToPath(import.meta.url)), "check-compose.mjs");
const CLICKHOUSE = "clickhouse/clickhouse-server:24.8.14.39";
const VALKEY = "valkey/valkey:9.1.2-alpine";
const dir = mkdtempSync(path.join(tmpdir(), "check-compose-"));
after(() => rmSync(dir, { recursive: true, force: true }));

/** A render in the shape `docker compose config --format json` writes. */
function render() {
  return {
    services: {
      clickhouse: {
        image: CLICKHOUSE,
        ports: [{ mode: "ingress", host_ip: "127.0.0.1", target: 8123, published: "8123", protocol: "tcp" }],
      },
      redis: {
        image: VALKEY,
        ports: [{ mode: "ingress", host_ip: "127.0.0.1", target: 6379, published: "16379", protocol: "tcp" }],
      },
      ingest: { build: { context: "." } },
    },
  };
}

let n = 0;
const BACKEND = `new GenericContainer("${CLICKHOUSE}"); new GenericContainer("${VALKEY}")`;

function check(compose, backendText = BACKEND) {
  n += 1;
  const composePath = path.join(dir, `compose-${n}.json`);
  const backendPath = path.join(dir, `backends-${n}.ts`);
  writeFileSync(composePath, JSON.stringify(compose));
  writeFileSync(backendPath, backendText);
  const r = spawnSync(process.execPath, [CHECK, composePath, backendPath], { encoding: "utf8" });
  return { status: r.status, stderr: r.stderr, stdout: r.stdout };
}

function refuses(compose, message, backendText) {
  const r = check(compose, backendText);
  assert.equal(r.status, 1, r.stdout);
  assert.match(r.stderr, message);
}

test("the render as it stands passes", () => {
  const r = check(render());
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /compose ok: 3 services, 2 published ports on 127\.0\.0\.1/);
  assert.match(r.stdout, /valkey\/valkey:9\.1\.2-alpine in the compose and in 1 test backends/);
});

test("a port on every interface, or with no host address, is refused", () => {
  const open = render();
  open.services.redis.ports[0].host_ip = "0.0.0.0";
  refuses(open, /redis: port 16379->6379 is published on 0\.0\.0\.0/);
  const bare = render();
  delete bare.services.redis.ports[0].host_ip;
  refuses(bare, /redis: port 16379->6379 is published on every interface/);
});

test("a service on the host's network is refused", () => {
  const c = render();
  c.services.redis.network_mode = "host";
  delete c.services.redis.ports;
  refuses(c, /redis: network_mode host publishes every port/);
});

test("an image with no tag, or tagged latest, is refused; a digest is not", () => {
  const untagged = render();
  untagged.services.redis.image = "redis";
  refuses(untagged, /redis: the image redis names no tag or digest/);
  const latest = render();
  latest.services.redis.image = "redis:latest";
  refuses(latest, /redis: the image redis:latest is tagged latest/);
  const pinned = render();
  pinned.services.ingest = { image: `cyberguard-ingest:latest@sha256:${"a".repeat(64)}` };
  assert.equal(check(pinned).status, 0);
});

test("NATS or MinIO, by service name or by image, is refused", () => {
  const nats = render();
  nats.services.nats = { image: "nats:2.10-alpine" };
  refuses(nats, /nats: NATS and MinIO stay out of the dev stack until a SPEC uses them/);
  const minio = render();
  minio.services.objects = { image: "minio/minio:RELEASE.2024-10-13T13-34-11Z" };
  refuses(minio, /objects: NATS and MinIO stay out of the dev stack/);
  const named = render();
  named.services.minio = { build: { context: "." } };
  refuses(named, /minio: NATS and MinIO stay out of the dev stack/);
});

test("a service with neither an image nor a build is refused", () => {
  const c = render();
  c.services.ingest = {};
  refuses(c, /ingest: neither an image nor a build/);
});

test("ClickHouse off a patch release, or apart from a backend, is refused", () => {
  const floating = render();
  floating.services.clickhouse.image = "clickhouse/clickhouse-server:24.8";
  refuses(floating, /clickhouse: the image clickhouse\/clickhouse-server:24\.8 is not pinned/);
  refuses(render(), /starts clickhouse\/clickhouse-server:24\.3\.1\.1, the dev compose/, `image("clickhouse/clickhouse-server:24.3.1.1"); image("${VALKEY}")`);
  refuses(render(), /names no ClickHouse image/, `image("${VALKEY}")`);
});

// SPEC-023 deploy_ac_022: the key-value store runs Valkey (ADR-0003 Amendment
// 2026-10-23), one image pinned to a patch release, in the compose and the backends.
test("deploy_ac_022: the key-value store off Valkey, off a patch release, or apart from a backend, is refused", () => {
  const redisImage = render();
  redisImage.services.redis.image = "redis:7.4-alpine";
  refuses(redisImage, /redis: the key-value store's image redis:7\.4-alpine is not valkey\/valkey:<major>\.<minor>\.<patch>-alpine/);
  const floating = render();
  floating.services.redis.image = "valkey/valkey:9.1-alpine";
  refuses(floating, /redis: the key-value store's image valkey\/valkey:9\.1-alpine is not valkey\/valkey:<major>\.<minor>\.<patch>-alpine/);
  const missing = render();
  delete missing.services.redis;
  refuses(missing, /the dev compose has no redis service/);
  refuses(render(), /backends-\d+\.ts: starts redis:7\.4-alpine, an image of the redis repository/, `image("${CLICKHOUSE}"); image("${VALKEY}"); image("redis:7.4-alpine")`);
  refuses(render(), /backends-\d+\.ts: starts valkey\/valkey:9\.0\.6-alpine, the dev compose valkey\/valkey:9\.1\.2-alpine/, `image("${CLICKHOUSE}"); image("valkey/valkey:9.0.6-alpine")`);
  refuses(render(), /backends-\d+\.ts: names no Valkey image/, `image("${CLICKHOUSE}")`);
  const elsewhere = render();
  elsewhere.services.cache = { image: "redis:7-alpine" };
  refuses(elsewhere, /cache: the image redis:7-alpine is of the redis repository/);
});

test("deploy_ac_022: other spellings of the two repositories are seen; a digest and a bare alias are not refused", () => {
  for (const image of ["mirror.gcr.io/library/redis:7.4-alpine", "redis/redis-stack-server:7.4.0-v1", "bitnami/redis:7.4"]) {
    refuses(render(), new RegExp(`backends-\\d+\\.ts: starts ${image.replace(/[./]/g, "\\$&")}, an image of the redis repository`), `image("${CLICKHOUSE}"); image("${VALKEY}"); image("${image}")`);
  }
  refuses(render(), /backends-\d+\.ts: starts docker\.io\/valkey\/valkey:8\.0\.0-alpine, not valkey\/valkey:<major>/, `image("${CLICKHOUSE}"); image("${VALKEY}"); image("docker.io/valkey/valkey:8.0.0-alpine")`);
  refuses(render(), /backends-\d+\.ts: starts valkey\/valkey:9\.1-alpine, not valkey\/valkey:<major>/, `image("${CLICKHOUSE}"); image("valkey/valkey:9.1-alpine")`);
  refuses(render(), /backends-\d+\.ts: starts redis:7\.4-alpine/, `image("${CLICKHOUSE}"); image("${VALKEY}"); don't("redis:7.4-alpine")`);
  assert.equal(check(render(), `image("${CLICKHOUSE}"); image("${VALKEY}").withNetworkAliases("redis")`).status, 0);
  refuses(render(), /backends-\d+\.ts: starts redis, an image of the redis repository/, `image("${CLICKHOUSE}"); image("${VALKEY}"); new GenericContainer("redis")`);
  refuses(render(), /backends-\d+\.ts: starts ghcr\.io\/valkey-io\/valkey:7\.2, not valkey\/valkey:<major>/, `image("${CLICKHOUSE}"); image("${VALKEY}"); new GenericContainer("ghcr.io/valkey-io/valkey:7.2")`);
  const bothFloating = render();
  bothFloating.services.redis.image = "valkey/valkey:9.1-alpine";
  refuses(bothFloating, /backends-\d+\.ts: starts valkey\/valkey:9\.1-alpine, not valkey\/valkey:<major>/, `image("${CLICKHOUSE}"); image("valkey/valkey:9.1-alpine")`);
  assert.equal(check(render(), `image("${CLICKHOUSE}"); image("${VALKEY}"); import x from "./lib/redis"`).status, 0);
  const digest = render();
  const pinned = `${VALKEY}@sha256:${"b".repeat(64)}`;
  digest.services.redis.image = pinned;
  assert.equal(check(digest, `image("${CLICKHOUSE}"); image("${pinned}")`).status, 0);
});
