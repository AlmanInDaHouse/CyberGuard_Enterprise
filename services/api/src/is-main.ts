import { realpathSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Whether the module at `moduleUrl` is the program Node was asked to run, so that a command
 * runs as a program and not when a test or another module imports it. The path given may
 * be a symlinked bin, or written without its extension: Node runs `node dist/db/migrate` as
 * `migrate.js`, and a check that compared the path as given would skip the command and exit
 * 0, a one-shot's silent success.
 */
export function isMain(moduleUrl: string): boolean {
  const target = process.argv[1];
  if (!target) return false;
  const self = realpathSync(fileURLToPath(moduleUrl));
  for (const candidate of [target, `${target}.js`, `${target}.ts`]) {
    try {
      if (realpathSync(path.resolve(candidate)) === self) return true;
    } catch {
      // Not a file by that name; try the next spelling.
    }
  }
  return false;
}
