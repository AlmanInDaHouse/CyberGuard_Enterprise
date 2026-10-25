// Next's standalone output (`output: "standalone"`, SPEC-023 §Data contracts §2) holds the
// server and the files it traced, not the static assets the browser loads: copy them beside
// the server, which serves them from there. The build runs this after `next build`, so the
// standalone tree the tests start and the image copies is complete. A Windows checkout builds
// without the standalone output (next.config.mjs), and there is nothing to copy.
import { cpSync, existsSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dashboard = join(dirname(fileURLToPath(import.meta.url)), "..");
// The tracing root is the repository's (next.config.mjs), so the server sits under dashboard/.
const server = join(dashboard, ".next", "standalone", "dashboard");
if (process.platform === "win32") process.exit(0);
if (!existsSync(join(server, "server.js"))) {
  throw new Error(`no standalone server at ${server}: is output "standalone" set?`);
}
const assets = [
  [join(dashboard, ".next", "static"), join(server, ".next", "static")],
  [join(dashboard, "public"), join(server, "public")],
];
for (const [from, to] of assets) {
  rmSync(to, { recursive: true, force: true });
  if (existsSync(from)) cpSync(from, to, { recursive: true });
}
