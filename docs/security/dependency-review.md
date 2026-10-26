# Dependency review

The runbook the threat model names for its build dependencies
([threat-model.md](threat-model.md), *Scope* and *Dependency review*). It
covers the third-party packages the repository locks: `pnpm-lock.yaml` for
`services/ingest`, `services/api` and `dashboard`, and `Cargo.lock` for
`cg-agent`. It was written when debt #93 closed, in Session 41.

## The audit in CI

[`dependency-audit.yml`](../../.github/workflows/dependency-audit.yml) runs four
jobs:

- `pnpm audit --prod --audit-level=high` reads `pnpm-lock.yaml`. It audits the
  production dependencies of the three workspaces. A high or critical advisory
  fails the run; a moderate or low one is counted in the summary line and
  passes.
- `pnpm audit --audit-level=critical` reads it too, over every dependency,
  development ones included: a critical advisory fails the run (debt #110).
- `cargo audit` (cargo-audit 0.22.2) reads `Cargo.lock`, every crate the
  workspace locks, the agent's development dependencies with them. Any
  vulnerability advisory fails the run, whatever its severity; an unmaintained,
  unsound or yanked crate is printed as a warning and passes.
- `check-licences.mjs` reads the licences of the production pnpm dependencies
  and of the agent's runtime crates; one that
  `.github/scripts/licences-allowed.json` does not admit fails the run, named.
  Red for a licence: replace the package, or add the licence or the package to
  the list, a reviewed change with its reason (debt #112).

It runs on a push to `main` or a pull request that changes a lockfile, a
`package.json`, a Cargo manifest, the licence checker or its list, or the
workflow itself; every Monday at
06:17 UTC, on `main`; and by hand (`workflow_dispatch`). The weekly run is
what catches an advisory published after a release: it turns `main` red with
no commit of ours.

## When the run is red

A red run on `main`'s tip is a red workflow under CLAUDE.md's *CI monitoring*:
the session that sees it fixes it before other work, or Manuel accepts it as
*Known CI debt*. The fixes, in the order to try them:

1. Move the package within the range its manifest declares:
   `pnpm update <package>` in the workspace that declares it, or
   `cargo update -p <crate>`. Check that only the intended packages moved
   (`git diff` of the lockfile).
2. Raise the declared range in the manifest, within the package's major.
3. For a transitive package its parent pins: a `pnpm.overrides` entry in the
   root `package.json`, scoped to the parent (`parent>child`) where it can be,
   within the child's major. Remove the entry when the parent's own release
   lifts it.
4. A major upgrade, as its own change, under the gates.

A lockfile change is code (CLAUDE.md *Integration path* §2): a branch, a pull
request, the local gate in each workspace it touches, CI green on each commit.
A change to a crate the agent links, or to a package the ingest server runs on
its capture or detection path, lands after the elevated gate too, as PR #23
did. `cargo update -p` can add crates to the lock: one the agent compiles
(`cargo tree -p cg-agent -e normal --target x86_64-pc-windows-msvc -i <crate>`)
is a new third-party dependency of `cg-agent`, which the next section gives to
Manuel. PR #23 added `rand_pcg`, which the agent does not compile.

No advisory is ignored (`cargo audit --ignore`, an `audit.toml`,
`pnpm audit --ignore`, or an `auditConfig` entry such as `ignoreGhsas` or
`ignoreCves`) unless Manuel accepts it as debt; the
debt names the advisory's id and lands in `docs/debts.md` in the same commit
as the ignore.

## A new dependency

Who decides is CLAUDE.md's *Decision authority*: Manuel decides any new
third-party dependency of `cg-agent`, which runs elevated, and a heavyweight
one of the server or the dashboard (a framework, a runtime, a data-store
client, or a library that does cryptography or networking or runs install
scripts or a native build); Claude Code decides the others. Either way the
change records the package's name, version, licence and reason. Before it is
added:

- its licence is compatible with the workspace's Apache-2.0, and, for a
  production dependency of the services, the dashboard or the agent, one that
  `.github/scripts/licences-allowed.json` admits: `dependency-audit.yml`'s
  licences job runs `.github/scripts/check-licences.mjs`, which fails, naming
  the package, on any other (SPEC-023 §Operational §12, debt #112). A licence
  or a package added to that list is a reviewed change, with its reason;
- its maintainers and releases are active;
- `pnpm audit` reports nothing against it, run locally, and neither does
  `cargo audit`, run locally (cargo-audit 0.22.2, on CLAUDE.md's *Approved
  local toolchain* since Session 42) and in the pull request's CI;
- an install script runs only if the package is listed in the root
  `package.json`'s `pnpm.onlyBuiltDependencies`; a crate's build script
  (`build.rs`) always runs when the agent builds, so a new crate's is read
  before it is added.

A package whose private fields the code reads is pinned to one release, and a
test asserts that release (nodemailer, soar_ac_006, debt #75).

## What is not covered

- **The pnpm development dependencies** (the test tooling, the linters) fail
  CI only on a critical advisory; `pnpm audit` without `--prod` lists the
  rest (debt #110). Since Session 43 it lists three:
  - braces, high, with no fixed release, through the micromatch of
    markdownlint-cli2 and of the dashboard's tailwindcss 3 (its chokidar and
    fast-glob); it expands globs this repository writes;
  - katex, low, which markdownlint loads through micromark's math extension
    and whose renderer it never calls;
  - tailwindcss 3's postcss-selector-parser, moderate, fixed in tailwindcss's
    next major.

  markdownlint-cli2 follows the engine of CI's markdownlint action (0.23.2,
  under `markdownlint-cli2-action@v24`). Three of the children it pins
  exactly are overridden in the lock (js-yaml 5.4.1, markdown-it 14.3.1,
  smol-toml 1.9.0), which the root devDependency runs; CI's action bundles
  its own copies of them, unpatched, and so does a fresh `npx`. They parse
  only this repository's Markdown and configuration.
  The agent's development crates are: `cargo audit` reads the whole
  `Cargo.lock`.
- **A malicious release with no advisory.** The lockfiles pin each version
  with its integrity hash (`integrity` in `pnpm-lock.yaml`, `checksum` in
  `Cargo.lock`), and CI installs from them (`--frozen-lockfile`, `--locked`),
  so a release enters only through a lockfile change someone reviews. Nothing
  checks a release's provenance.
- **Licences** beyond the checked ones. `check-licences.mjs` reads the
  production pnpm dependencies as CI installs them (on Linux with glibc) and
  the agent's runtime crates for its Windows target. What an image ships
  beyond them is not checked: its base image's packages, and the musl build
  of a native package the image installs. Each released image's index carries BuildKit's SBOM attestation, which lists the operating-system and Node packages installed in the image, and each image carries its third-party notices (SPEC-023 Amendment 2026-10-26, items 2 and 4; debt #112); neither checks a licence.
- **The warnings `cargo audit` lets pass**: an unmaintained, unsound or
  yanked crate passes with a warning, as chacha20 0.10.0, in the agent's
  development dependencies, did until Session 42, and rustls-pemfile,
  unmaintained, which the agent read its certificates with, until Session 43
  (debt #111). Since then `cargo audit` prints none; a new one is a debt.
- **The GitHub Actions the workflows use** are pinned by major tag, not by
  commit (debt #113).
