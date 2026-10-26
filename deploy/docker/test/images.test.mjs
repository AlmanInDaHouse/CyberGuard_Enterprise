// SPEC-023 §Data contracts §2 and deploy_ac_019's image half, with deploy_ac_001's clause
// on the Dockerfiles: the four product images, built from the commit (deploy-ci.yml builds
// them as cyberguard-<name>:test before this file runs; CG_TEST_IMAGE_TAG names another tag).
// Each runs as its non-root uid, carries LICENSE, NOTICE, the labels of §Data contracts §2
// and a health check, and its Dockerfile pins every base image by digest; in the ingest image
// uid 1000 cannot write the rules or the playbooks; the proxy's Caddy binary carries no file
// capability. The dashboard and the proxy are also started as the bundle runs them
// (read-only, every capability dropped, no new privileges) until their health check passes.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { request } from "node:http";
import { request as httpsRequest } from "node:https";
import { dirname, join } from "node:path";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const TAG = process.env.CG_TEST_IMAGE_TAG || "test";
const SOURCE = "https://github.com/AlmanInDaHouse/CyberGuard_Enterprise";
const DOC_DIR = "/usr/share/doc/cyberguard";
const HARDENED = ["--cap-drop", "ALL", "--security-opt", "no-new-privileges:true"];
/** The labels of §Data contracts §2 (SPEC-023 Amendment 2026-10-26, item 1). */
const OCI_LABELS = [
  "source",
  "revision",
  "version",
  "licenses",
  "title",
  "description",
  "url",
  "documentation",
  "vendor",
].map((k) => `org.opencontainers.image.${k}`);

const IMAGES = [
  { name: "ingest", uid: "1000", dockerfile: "services/ingest/Dockerfile" },
  { name: "api", uid: "1000", dockerfile: "services/api/Dockerfile" },
  { name: "dashboard", uid: "1000", dockerfile: "dashboard/Dockerfile" },
  { name: "proxy", uid: "10001", dockerfile: "deploy/docker/proxy/Dockerfile" },
];

const imageOf = (name) => `cyberguard-${name}:${TAG}`;

/** Runs `docker <args>`; returns its status and output. */
function docker(args, encoding = "utf8") {
  const r = spawnSync("docker", args, { encoding, maxBuffer: 64 * 1024 * 1024 });
  if (r.error) throw r.error;
  return r;
}

/** Runs `docker <args>` and returns its trimmed standard output, failing on a non-zero exit. */
function dockerOk(args) {
  const r = docker(args);
  assert.equal(r.status, 0, `docker ${args.join(" ")}\n${r.stdout}\n${r.stderr}`);
  return r.stdout.trim();
}

function inspect(image) {
  return JSON.parse(dockerOk(["image", "inspect", image]))[0];
}

/** Waits until a container's health check reports healthy; fails when it reports otherwise. */
async function healthy(id, timeoutMs = 120_000) {
  const start = Date.now();
  for (;;) {
    const status = dockerOk(["inspect", "-f", "{{.State.Health.Status}}", id]);
    if (status === "healthy") return;
    if (status === "unhealthy" || Date.now() - start > timeoutMs) {
      const logs = docker(["logs", "--tail", "40", id]);
      assert.fail(`container ${id} is ${status}\n${logs.stdout}\n${logs.stderr}`);
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
}

/** The host port a container publishes `port` on. */
function hostPort(id, port) {
  const line = dockerOk(["port", id, `${port}/tcp`]).split(/\r?\n/)[0];
  return Number(line.slice(line.lastIndexOf(":") + 1));
}

function get(port, path, headers = {}, tls = null) {
  return new Promise((resolve, reject) => {
    const send = tls ? httpsRequest : request;
    const req = send({ host: "127.0.0.1", port, path, headers, ...tls }, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (c) => {
        body += c;
      });
      res.on("end", () => resolve({ status: res.statusCode, headers: res.headers, body }));
    });
    req.on("error", reject);
    req.end();
  });
}

test("deploy_ac_001 (Dockerfiles): every base image is pinned by digest", () => {
  for (const { dockerfile } of IMAGES) {
    const stages = new Set();
    const lines = readFileSync(join(REPO, dockerfile), "utf8").split(/\r?\n/);
    const froms = lines.filter((l) => /^\s*FROM\s/i.test(l));
    assert.ok(froms.length > 0, `${dockerfile} has no FROM`);
    for (const line of froms) {
      const words = line.trim().split(/\s+/).slice(1);
      const image = words.find((w) => !w.startsWith("--"));
      const as = words.findIndex((w) => w.toLowerCase() === "as");
      // A stage built from an earlier stage names no base image.
      if (!stages.has(image)) {
        assert.match(image, /^[^\s@]+@sha256:[0-9a-f]{64}$/, `${dockerfile}: ${line.trim()}`);
      }
      if (as !== -1) stages.add(words[as + 1]);
    }
    // An image pulled otherwise is pinned too: a frontend named by a syntax directive, and an
    // image a COPY --from or a RUN --mount=from= names (a stage's name excepted).
    for (const line of lines) {
      const syntax = /^#\s*syntax\s*=\s*(\S+)/i.exec(line);
      if (syntax) assert.match(syntax[1], /@sha256:[0-9a-f]{64}$/, `${dockerfile}: ${line.trim()}`);
      for (const m of line.matchAll(/--(?:mount=[^\s]*?)?from=([^\s,]+)/gi)) {
        if (!stages.has(m[1])) {
          assert.match(m[1], /@sha256:[0-9a-f]{64}$/, `${dockerfile}: ${line.trim()}`);
        }
      }
    }
  }
});

for (const { name, uid } of IMAGES) {
  describe(`the ${name} image`, () => {
    const image = imageOf(name);

    test(`runs as uid ${uid}`, () => {
      assert.equal(dockerOk(["run", "--rm", "--entrypoint", "id", image, "-u"]), uid);
      assert.notEqual(inspect(image).Config.User, "");
    });

    test("carries LICENSE and NOTICE", () => {
      for (const file of ["LICENSE", "NOTICE"]) {
        const r = docker(["run", "--rm", "--entrypoint", "cat", image, `${DOC_DIR}/${file}`], "buffer");
        assert.equal(r.status, 0, `${file}: ${r.stderr}`);
        assert.ok(r.stdout.equals(readFileSync(join(REPO, file))), `${file} differs from the repository's`);
      }
    });

    test("carries the labels of §Data contracts §2, and no other of their prefix", () => {
      const labels = inspect(image).Config.Labels ?? {};
      assert.equal(labels["org.opencontainers.image.source"], SOURCE);
      assert.equal(labels["org.opencontainers.image.licenses"], "Apache-2.0");
      const version = labels["org.opencontainers.image.version"];
      assert.ok(version, "no version label");
      // The build's own version, not a base image's (Caddy's carries its own).
      if (process.env.CG_TEST_VERSION) assert.equal(version, process.env.CG_TEST_VERSION);
      const revision = labels["org.opencontainers.image.revision"];
      assert.ok(revision, "no revision label");
      if (process.env.CG_TEST_REVISION) assert.equal(revision, process.env.CG_TEST_REVISION);
      // SPEC-023 Amendment 2026-10-26, item 1 (debt #128).
      assert.equal(labels["org.opencontainers.image.title"], `CyberGuard ${name}`);
      const description = labels["org.opencontainers.image.description"] ?? "";
      assert.match(description, /^CyberGuard's /);
      assert.ok(description.toLowerCase().includes(name), `the description names no ${name}: ${description}`);
      assert.equal(labels["org.opencontainers.image.url"], SOURCE);
      assert.equal(
        labels["org.opencontainers.image.documentation"],
        `${SOURCE}/tree/${revision}/docs/operations`,
      );
      assert.equal(labels["org.opencontainers.image.vendor"], "The CyberGuard Authors");
      // Every label of the prefix is one of these nine, so a base image's label (Caddy's
      // title, description, URL, documentation and vendor) cannot remain beside them.
      const oci = Object.keys(labels).filter((k) => k.startsWith("org.opencontainers.image."));
      assert.deepEqual(oci.sort(), [...OCI_LABELS].sort());
    });

    test("has a health check", () => {
      const check = inspect(image).Config.Healthcheck?.Test ?? [];
      assert.ok(["CMD", "CMD-SHELL"].includes(check[0]), `health check: ${JSON.stringify(check)}`);
    });
  });
}

describe("the ingest image's files", () => {
  const image = imageOf("ingest");
  const sh = (script) => docker(["run", "--rm", "--entrypoint", "sh", image, "-c", script]);

  test("uid 1000 can write none of the rules, the playbooks or their schema", () => {
    const r = sh(
      'for p in /app $(find /app/rules /app/playbooks /app/schemas /app/playbooks-none); do if [ -w "$p" ]; then echo "$p"; fi; done; if touch /app/rules/x 2>/dev/null; then echo created; fi',
    );
    assert.equal(r.status, 0, r.stderr);
    assert.equal(r.stdout.trim(), "", `writable by uid 1000:\n${r.stdout}`);
    assert.equal(sh("ls /app/rules/windows | grep -q .").status, 0, "no rules in /app/rules/windows");
  });

  test("/app, the rules, the playbooks and their schema are owned by root", () => {
    const r = sh("find /app -maxdepth 0 ! -user 0; find /app/rules /app/playbooks /app/schemas /app/playbooks-none ! -user 0");
    assert.equal(r.status, 0, r.stderr);
    assert.equal(r.stdout.trim(), "", `not owned by root:\n${r.stdout}`);
  });

  test("/app/playbooks-none is an empty directory", () => {
    const r = sh("test -d /app/playbooks-none && ls -A /app/playbooks-none");
    assert.equal(r.status, 0, r.stderr);
    assert.equal(r.stdout.trim(), "");
  });

  test("/certs is owned by uid 1000, so a new volume there is writable", () => {
    const r = sh("stat -c %u /certs && touch /certs/probe");
    assert.equal(r.status, 0, r.stderr);
    assert.equal(r.stdout.trim(), "1000");
  });
});

describe("the dashboard image", () => {
  const image = imageOf("dashboard");

  test("sets NEXT_TELEMETRY_DISABLED=1, PORT=3000 and HOSTNAME=0.0.0.0", () => {
    const env = inspect(image).Config.Env ?? [];
    for (const pair of ["NEXT_TELEMETRY_DISABLED=1", "PORT=3000", "HOSTNAME=0.0.0.0"]) {
      assert.ok(env.includes(pair), `${pair} not in ${JSON.stringify(env)}`);
    }
  });

  test("its command is Next's standalone server", () => {
    assert.deepEqual(inspect(image).Config.Cmd, ["node", "dashboard/server.js"]);
  });

  test("runs Next's standalone server read-only, and answers its health check", async () => {
    const id = dockerOk([
      "run",
      "-d",
      "--read-only",
      "--tmpfs",
      "/tmp",
      "--tmpfs",
      "/app/dashboard/.next/cache",
      ...HARDENED,
      "-e",
      "API_BASE_URL=http://127.0.0.1:9",
      "-p",
      "127.0.0.1::3000",
      image,
    ]);
    try {
      await healthy(id);
      const port = hostPort(id, 3000);
      const health = await get(port, "/api/health");
      assert.equal(health.status, 200);
      assert.deepEqual(JSON.parse(health.body), { status: "ok" });
      const login = await get(port, "/login");
      assert.equal(login.status, 200);
      assert.match(String(login.headers["content-security-policy"]), /'nonce-[^']+'/);
    } finally {
      docker(["rm", "-f", "-v", id]);
    }
  });

  test("refuses an empty API_BASE_URL with one line, exit 1", () => {
    const r = docker(["run", "--rm", ...HARDENED, "-e", "API_BASE_URL=", image]);
    assert.equal(r.status, 1, `${r.stdout}\n${r.stderr}`);
    const lines = r.stderr.trim().split(/\r?\n/);
    assert.equal(lines.length, 1, r.stderr);
    assert.match(lines[0], /^cg-dashboard: fatal: invalid configuration: API_BASE_URL: .+$/);
  });
});

describe("the proxy image", () => {
  const image = imageOf("proxy");

  test("its Caddy binary carries no file capability, and runs with every capability dropped", () => {
    assert.equal(dockerOk(["run", "--rm", "--entrypoint", "getcap", image, "/usr/bin/caddy"]), "");
    const r = docker(["run", "--rm", ...HARDENED, image, "caddy", "version"]);
    assert.equal(r.status, 0, `${r.stdout}\n${r.stderr}`);
  });

  test("/data and /config are owned by uid 10001", () => {
    assert.equal(dockerOk(["run", "--rm", "--entrypoint", "stat", image, "-c", "%u", "/data", "/config"]), "10001\n10001");
  });

  test("answers its health check, redirects HTTP to HTTPS on the published port, and answers HTTPS with HSTS and no HTTP/3 advertised", async () => {
    const id = dockerOk([
      "run",
      "-d",
      "--read-only",
      "--tmpfs",
      "/tmp",
      // The bundle's named volumes proxy_data and proxy_config, here anonymous ones.
      "-v",
      "/data",
      "-v",
      "/config",
      ...HARDENED,
      "-e",
      "CG_DASHBOARD_HOSTNAME=cg.example.test",
      "-e",
      "CG_TLS_MODE=internal",
      "-e",
      "CG_HTTPS_PORT=4443",
      "-p",
      "127.0.0.1::8080",
      "-p",
      "127.0.0.1::8443",
      image,
    ]);
    try {
      await healthy(id);
      const res = await get(hostPort(id, 8080), "/incidents?x=1", { Host: "cg.example.test" });
      assert.equal(res.status, 308);
      assert.equal(res.headers.location, "https://cg.example.test:4443/incidents?x=1");
      // No dashboard behind it here, so the answer is an error, with HSTS all the same, and
      // no HTTP/3 advertised: the compose publishes TCP alone.
      const tls = await get(
        hostPort(id, 8443),
        "/login",
        { Host: "cg.example.test" },
        { servername: "cg.example.test", rejectUnauthorized: false },
      );
      assert.equal(tls.headers["strict-transport-security"], "max-age=31536000");
      assert.equal(tls.headers["alt-svc"], undefined);
    } finally {
      docker(["rm", "-f", "-v", id]);
    }
  });
});
