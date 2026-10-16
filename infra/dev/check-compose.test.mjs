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
        image: "redis:7.4-alpine",
        ports: [{ mode: "ingress", host_ip: "127.0.0.1", target: 6379, published: "16379", protocol: "tcp" }],
      },
      ingest: { build: { context: "." } },
    },
  };
}

let n = 0;
function check(compose, backendText = `new GenericContainer("${CLICKHOUSE}")`) {
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
  pinned.services.redis.image = `redis:latest@sha256:${"a".repeat(64)}`;
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
  refuses(render(), /starts clickhouse\/clickhouse-server:24\.3\.1\.1, the dev compose/, 'image("clickhouse/clickhouse-server:24.3.1.1")');
  refuses(render(), /names no ClickHouse image/, "no image here");
});
