import { readFileSync, readdirSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

// Debt #6 (docs/handoff-session-26.md): the api tests read the `cges_events` that
// ingest's own bootstrap creates (helpers/events-schema.ts), and keep no DDL of
// their own, so the table they read cannot drift from the one ingest writes.

const TEST_DIR = path.dirname(fileURLToPath(import.meta.url));
const DDL = /\b(?:CREATE|ALTER)\s+TABLE\b[^;`]*\b(?:cges_events|heartbeats)\b/i;

function testFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return testFiles(full);
    return entry.name.endsWith(".ts") ? [full] : [];
  });
}

test("no api test file defines ingest's ClickHouse tables", () => {
  const files = testFiles(TEST_DIR);
  expect(files.length).toBeGreaterThan(20);
  const defining = files
    .filter((file) => DDL.test(readFileSync(file, "utf8")))
    .map((file) => path.relative(TEST_DIR, file).replaceAll("\\", "/"));
  expect(defining).toEqual([]);
});
