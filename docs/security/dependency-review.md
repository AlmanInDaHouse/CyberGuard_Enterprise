# Dependency review

The runbook the threat model names for its build dependencies
([threat-model.md](threat-model.md), *Scope* and *Dependency review*). It
covers the third-party packages the repository locks: `pnpm-lock.yaml` for
`services/ingest`, `services/api` and `dashboard`, and `Cargo.lock` for
`cg-agent`. It was written when debt #93 closed, in Session 41.

## The audit in CI

[`dependency-audit.yml`](../../.github/workflows/dependency-audit.yml) runs two
jobs:

- `pnpm audit --prod --audit-level=high` reads `pnpm-lock.yaml`. It audits the
  production dependencies of the three workspaces. A high or critical advisory
  fails the run; a moderate or low one is counted in the summary line and
  passes.
- `cargo audit` (cargo-audit 0.22.2) reads `Cargo.lock`, every crate the
  workspace locks, the agent's development dependencies with them. Any
  vulnerability advisory fails the run, whatever its severity; an unmaintained,
  unsound or yanked crate is printed as a warning and passes.

It runs on a push to `main` or a pull request that changes a lockfile, a
`package.json`, a Cargo manifest or the workflow itself; every Monday at
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

- its licence is compatible with the workspace's Apache-2.0;
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

- **The pnpm development dependencies** (the test tooling, the linters) are
  not audited in CI; `pnpm audit` without `--prod` lists them (debt #110).
  Since Session 42 (vitest 4, testcontainers 12) what it lists lies in
  markdownlint-cli2, whose version follows CI's markdownlint action, and in
  tailwindcss 3's postcss-selector-parser, fixed in tailwindcss's next major.
  The agent's development crates are: `cargo audit` reads the whole
  `Cargo.lock`.
- **A malicious release with no advisory.** The lockfiles pin each version
  with its integrity hash (`integrity` in `pnpm-lock.yaml`, `checksum` in
  `Cargo.lock`), and CI installs from them (`--frozen-lockfile`, `--locked`),
  so a release enters only through a lockfile change someone reviews. Nothing
  checks a release's provenance.
- **Licences** are checked when a dependency is added, by hand. No tool checks
  the licences of transitive dependencies, and no SBOM is produced (debt
  #112).
- **The warnings `cargo audit` lets pass**: an unmaintained crate the agent
  reads its certificates with (rustls-pemfile, debt #111). A crate yanked
  later passes the same way, as chacha20 0.10.0, in the agent's development
  dependencies, did until Session 42.
- **The GitHub Actions the workflows use** are pinned by major tag, not by
  commit (debt #113).
