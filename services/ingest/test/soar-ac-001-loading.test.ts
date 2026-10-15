import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { type AddressInfo, type Server, createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, inject, test } from "vitest";
import { startIngest } from "../src/server.js";
import { PlaybookLoadError, loadPlaybooks } from "../src/soar/loader.js";

// soar_ac_001 — SPEC-022 §Operational §1: the loader reads *.yml / *.yaml from the
// playbooks directory, validates each against schemas/playbooks/v0.1 and the policy
// (no live agent.isolate, unique ids, the closed placeholder set), refuses a bad
// file by name, and leaves SOAR idle on an absent or empty directory.

const here = dirname(fileURLToPath(import.meta.url));
const REPO_PLAYBOOKS = join(here, "..", "..", "..", "playbooks");

const VALID = `id: pb_test
version: 1
name: Test playbook
trust_sources: [rule]
trigger:
  on: incident.created
  when:
    severity_id_gte: 4
steps:
  - id: isolate
    action: agent.isolate
  - id: notify
    action: notify.email
    mode: live
    params:
      subject: "Pending: {{incident.title}}"
      body: "Incident {{incident.id}}\\n{{steps}}"
`;

/** A fresh directory holding the given files. */
function dirWith(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), "cg-pb-"));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  return dir;
}

test("the repository's playbooks load, and its README is ignored", () => {
  const loaded = loadPlaybooks(REPO_PLAYBOOKS);
  expect(loaded.idle).toBeUndefined();
  expect(loaded.playbooks.map((p) => p.id)).toEqual(["pb_high_severity_containment"]);
  const [pb] = loaded.playbooks;
  expect(pb?.severityIdGte).toBe(4);
  expect(pb?.trustSources).toEqual(["rule"]);
  expect(pb?.steps.map((s) => [s.id, s.action, s.mode])).toEqual([
    ["isolate", "agent.isolate", "dry_run"],
    ["notify", "notify.email", "live"],
  ]);
});

test("a step with no mode is dry_run", () => {
  const loaded = loadPlaybooks(dirWith({ "a.yml": VALID }));
  expect(loaded.playbooks[0]?.steps[0]?.mode).toBe("dry_run");
});

// [what, files, the reason the refusal must give]
const REFUSED: [string, Record<string, string>, RegExp][] = [
  ["an unknown field", { "bad.yml": `${VALID}extra: 1\n` }, /additional properties \(extra\)/],
  [
    "11 steps",
    {
      "bad.yml": VALID.replace(
        "steps:\n",
        `steps:\n${Array.from({ length: 9 }, (_, i) => `  - id: s${i}\n    action: agent.isolate\n`).join("")}`,
      ),
    },
    /\/steps must NOT have more than 10 items/,
  ],
  [
    "a malformed id",
    { "bad.yml": VALID.replace("id: pb_test", "id: Test-Playbook") },
    /\/id must match pattern/,
  ],
  [
    "params on agent.isolate",
    {
      "bad.yml": VALID.replace(
        "    action: agent.isolate\n",
        "    action: agent.isolate\n    params: { subject: a, body: b }\n",
      ),
    },
    /\/steps\/0 must NOT have additional properties \(params\)/,
  ],
  [
    "a live agent.isolate",
    {
      "bad.yml": VALID.replace(
        "    action: agent.isolate\n",
        "    action: agent.isolate\n    mode: live\n",
      ),
    },
    /agent\.isolate runs in dry_run only/,
  ],
  [
    "an unknown action",
    { "bad.yml": VALID.replace("action: agent.isolate", "action: process.kill") },
    /\/steps\/0\/action must be equal to constant/,
  ],
  [
    "a duplicate playbook id",
    { "a.yml": VALID, "bad.yml": VALID },
    /pb_test is not unique \(also in a\.yml\)/,
  ],
  [
    "a duplicate step id",
    { "bad.yml": VALID.replace("  - id: notify", "  - id: isolate") },
    /step id isolate is not unique/,
  ],
  [
    "an unknown placeholder",
    { "bad.yml": VALID.replace("{{incident.title}}", "{{incident.secret}}") },
    /not allowed there: \{\{incident\.secret\}\}/,
  ],
  [
    "{{steps}} in a subject",
    { "bad.yml": VALID.replace("{{incident.title}}", "{{steps}}") },
    /subject holds a placeholder that is not allowed there: \{\{steps\}\}/,
  ],
  ["text that is not YAML", { "bad.yml": "id: [unclosed\n  - : :" }, /is not YAML/],
  [
    "a placeholder written with spaces",
    { "bad.yml": VALID.replace("{{incident.title}}", "{{ incident.title }}") },
    /not allowed there: \{\{ incident\.title \}\}/,
  ],
];

test.each(REFUSED)("the loader refuses %s, naming the file and the reason", (_what, files, why) => {
  const dir = dirWith(files);
  expect(() => loadPlaybooks(dir)).toThrow(PlaybookLoadError);
  expect(() => loadPlaybooks(dir)).toThrow(/^playbook bad\.yml: /);
  expect(() => loadPlaybooks(dir)).toThrow(why);
});

test("an empty directory, or one with only a README or an upper-case extension, leaves SOAR idle at info", () => {
  for (const dir of [
    dirWith({}),
    dirWith({ "README.md": "# playbooks\n" }),
    dirWith({ "UPPER.YML": VALID }),
  ]) {
    const loaded = loadPlaybooks(dir);
    expect(loaded.playbooks).toEqual([]);
    expect(loaded.idle).toEqual({ reason: "no_playbooks", dir, level: "info" });
  }
});

test("a set directory that does not exist leaves SOAR idle, at warn", () => {
  const dir = join(tmpdir(), "cg-pb-does-not-exist-022");
  const loaded = loadPlaybooks(dir);
  expect(loaded.playbooks).toEqual([]);
  expect(loaded.idle).toEqual({ reason: "directory_absent", dir, level: "warn" });
});

test("startIngest rejects on a refused playbook before it opens any client", async () => {
  const config = inject("ingestConfig");
  const bad = dirWith({ "bad.yml": `${VALID}extra: 1\n` });
  // Postgres, ClickHouse and Redis all point at a listener that counts the
  // connections it gets. A rejection that names the file, with no connection made,
  // shows the loader ran before any migration and before any client was built
  // (ioredis connects as soon as it is constructed).
  let connections = 0;
  const counter: Server = await new Promise((resolve, reject) => {
    const server = createServer((socket) => {
      connections += 1;
      socket.destroy();
    });
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
  });
  const port = (counter.address() as AddressInfo).port;
  try {
    await expect(
      startIngest({
        ...config,
        INGEST_PG_URL: `postgres://u:p@127.0.0.1:${port}/none`,
        INGEST_CH_URL: `http://127.0.0.1:${port}`,
        INGEST_REDIS_URL: `redis://127.0.0.1:${port}`,
        INGEST_SOAR_PLAYBOOKS_DIR: bad,
      }),
    ).rejects.toThrow(/bad\.yml/);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(connections).toBe(0);
  } finally {
    await new Promise<void>((resolve) => counter.close(() => resolve()));
  }
});

test("the boot logs soar_idle for an empty directory", async () => {
  const config = inject("ingestConfig");
  const lines: { level: string; obj: Record<string, unknown>; msg: string }[] = [];
  const log = {
    info: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "info", obj, msg }),
    warn: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "warn", obj, msg }),
    error: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "error", obj, msg }),
  };
  const empty = mkdtempSync(join(tmpdir(), "cg-pb-empty-"));
  mkdirSync(join(empty, "sub"));
  const server = await startIngest({ ...config, INGEST_SOAR_PLAYBOOKS_DIR: empty }, { log });
  try {
    expect(lines).toContainEqual({
      level: "info",
      obj: { event: "soar_idle", reason: "no_playbooks", dir: empty },
      msg: "SOAR is idle",
    });
  } finally {
    await server.close();
  }
});

test("the boot logs soar_idle at warn for a set directory that does not exist", async () => {
  const config = inject("ingestConfig");
  const lines: { level: string; obj: Record<string, unknown>; msg: string }[] = [];
  const log = {
    info: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "info", obj, msg }),
    warn: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "warn", obj, msg }),
    error: (obj: Record<string, unknown>, msg: string) => lines.push({ level: "error", obj, msg }),
  };
  const absent = join(tmpdir(), "cg-pb-mistyped-022");
  const server = await startIngest({ ...config, INGEST_SOAR_PLAYBOOKS_DIR: absent }, { log });
  try {
    expect(lines).toContainEqual({
      level: "warn",
      obj: { event: "soar_idle", reason: "directory_absent", dir: absent },
      msg: "SOAR is idle",
    });
  } finally {
    await server.close();
  }
});
