import { readFileSync, readdirSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

// Debt #3 (docs/handoff-session-26.md): the suite's files share one Postgres and one
// ClickHouse, so an id written by hand in two files can name the same agent, alert or
// event in both. The agents these files write to the shared stores are drawn at
// random per run, and an id a file builds from a sequence starts with a prefix drawn
// per run (ID_PREFIX). The agent ids still written by hand live in a throwaway
// database, in memory, in a heartbeat the server refuses, or in a config the
// enrollment overwrites. No literal may repeat another file's, and no id is built on a
// fixed prefix.

const TEST_DIR = path.dirname(fileURLToPath(import.meta.url));
const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
/** A template that starts a UUID with fixed hexadecimal digits and fills the rest. */
const FIXED_PREFIX_TEMPLATE =
  /`[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{0,11}\$\{/gi;
/** The nil UUID is a value no row is written under, not a row's id. */
const NIL = "00000000-0000-0000-0000-000000000000";

function testFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return testFiles(full);
    return entry.name.endsWith(".ts") ? [full] : [];
  });
}

const files = testFiles(TEST_DIR).map((file) => ({
  name: path.relative(TEST_DIR, file).replaceAll("\\", "/"),
  text: readFileSync(file, "utf8"),
}));

test("the scan reads the suite", () => {
  expect(files.length).toBeGreaterThan(50);
  expect(files.some((f) => f.name === "harness-uuid-literals.test.ts")).toBe(true);
});

test("no UUID literal appears in two test files", () => {
  const firstSeen = new Map<string, string>();
  const clashes: string[] = [];
  for (const { name, text } of files) {
    const literals = new Set((text.match(UUID) ?? []).map((id) => id.toLowerCase()));
    for (const id of literals) {
      if (id === NIL) continue;
      const other = firstSeen.get(id);
      if (other === undefined) firstSeen.set(id, name);
      else clashes.push(`${id}: ${other} and ${name}`);
    }
  }
  expect(clashes).toEqual([]);
});

test("no test file builds an id on a fixed prefix", () => {
  const fixed = files.flatMap(({ name, text }) =>
    (text.match(FIXED_PREFIX_TEMPLATE) ?? []).map((m) => `${name}: ${m}`),
  );
  expect(fixed).toEqual([]);
});
