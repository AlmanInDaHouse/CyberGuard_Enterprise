// Debt #68 (docs/handoff-session-38.md) — what CI asserts of the dev compose, read
// from `docker compose ... --profile '*' config --format json` (every service, those
// under a profile included), so that a change to docker-compose.dev.yml runs a check
// (infra-ci.yml):
//
// - every published port binds 127.0.0.1 (debt #18), and no service shares the
//   host's network, which would publish every port it listens on;
// - every service names its image by a digest, or by a tag other than `latest`, or
//   is built from the repository;
// - ClickHouse is pinned to a patch release (debt #36), and each test backend named
//   on the command line starts the same image.
//
// check-compose.test.mjs runs it against compose files it breaks on purpose.
//
// Usage: node infra/dev/check-compose.mjs <compose.json> [<backends.ts> ...]
import { readFileSync } from "node:fs";

const [composePath, ...backendPaths] = process.argv.slice(2);
if (!composePath) {
  console.error("usage: node infra/dev/check-compose.mjs <compose.json> [<backends.ts> ...]");
  process.exit(2);
}

const compose = JSON.parse(readFileSync(composePath, "utf8"));
const services = Object.entries(compose.services ?? {});
const failures = [];
let published = 0;

if (services.length === 0) failures.push("the compose defines no service");

for (const [name, service] of services) {
  if (typeof service.network_mode === "string" && service.network_mode.startsWith("host")) {
    failures.push(`${name}: network_mode ${service.network_mode} publishes every port it listens on`);
  }
  for (const port of service.ports ?? []) {
    published += 1;
    if (port.host_ip !== "127.0.0.1") {
      failures.push(
        `${name}: port ${port.published ?? "?"}->${port.target} is published on ${port.host_ip ?? "every interface"}, not 127.0.0.1`,
      );
    }
  }
  if (service.image === undefined) {
    if (service.build === undefined) failures.push(`${name}: neither an image nor a build`);
    continue;
  }
  const image = /^(?<name>[^@]+?)(?::(?<tag>[\w][\w.-]{0,127}))?(?:@(?<digest>sha256:[0-9a-f]{64}))?$/.exec(
    service.image,
  );
  if (image === null) {
    failures.push(`${name}: cannot read the image reference ${service.image}`);
  } else if (image.groups.digest !== undefined) {
    // A digest names one image, whatever the tag beside it says.
  } else if (image.groups.tag === undefined) {
    failures.push(`${name}: the image ${service.image} names no tag or digest`);
  } else if (image.groups.tag === "latest") {
    failures.push(`${name}: the image ${service.image} is tagged latest`);
  }
}

const clickhouse = compose.services?.clickhouse?.image;
if (typeof clickhouse !== "string" || !/^clickhouse\/clickhouse-server:\d+\.\d+\.\d+\.\d+$/.test(clickhouse)) {
  failures.push(`clickhouse: the image ${clickhouse ?? "(none)"} is not pinned to a patch release`);
}
for (const path of backendPaths) {
  const named = [...readFileSync(path, "utf8").matchAll(/["'`](clickhouse\/clickhouse-server[^"'`]*)["'`]/g)].map(
    (m) => m[1],
  );
  if (named.length === 0) failures.push(`${path}: names no ClickHouse image`);
  for (const image of named) {
    if (image !== clickhouse) {
      failures.push(`${path}: starts ${image}, the dev compose ${clickhouse ?? "(none)"}`);
    }
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.error(failure);
  process.exit(1);
}
console.log(
  `compose ok: ${services.length} services, ${published} published ports on 127.0.0.1, ${clickhouse} in the compose and in ${backendPaths.length} test backends`,
);
