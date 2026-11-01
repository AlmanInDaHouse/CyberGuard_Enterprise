// SPEC-024 §Data contracts §1, its Amendment 2026-10-31 and its Amendment 2026-11-01,
// point 3 (debt #139): the release's tag is v<X>.<Y>.<Z>, each component a SemVer numeric
// identifier (0, or digits without a leading zero) within MSI's ProductVersion limits
// (major and minor 0-255, patch 0-65535). release.yml's first job runs this on the tag, and
// every job that pushes, attests or publishes waits for it.
//
//   node .github/scripts/release-version.mjs <tag>
//
// prints the version and exits 0, or names the reason and exits 1.

import { pathToFileURL } from "node:url";

/** MSI's limits on ProductVersion's three fields. */
export const LIMITS = [255, 255, 65535];

const TAG = /^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;

/** The release version a tag names, or an Error saying why it names none. */
export function releaseVersion(tag) {
  const match = TAG.exec(tag);
  if (!match) {
    return new Error(
      `${JSON.stringify(tag)} is not v<X>.<Y>.<Z> with each component 0 or digits without a leading zero`,
    );
  }
  const parts = match.slice(1).map(Number);
  for (const [i, part] of parts.entries()) {
    if (part > LIMITS[i]) {
      return new Error(
        `${JSON.stringify(tag)}: ${["major", "minor", "patch"][i]} ${match[i + 1]} is above MSI's limit of ${LIMITS[i]}`,
      );
    }
  }
  return parts.join(".");
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const result = releaseVersion(process.argv[2] ?? "");
  if (result instanceof Error) {
    console.error(`release version: ${result.message}`);
    process.exit(1);
  }
  console.log(result);
}
