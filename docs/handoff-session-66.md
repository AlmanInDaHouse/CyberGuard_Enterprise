# Handoff — Session 66

Session 66, 2026-11-12, did steps f and g of C5's plan
(`docs/handoff-session-61.md#how-session-62-resumes`, restated by
`docs/handoff-session-65.md#how-session-66-resumes`): the SCM test of code
10 on the adoption route with the real instance lock, and `package-ci`'s
host test, the real `cg-setup` `preflight` then `reverse` on a hosted
runner under a token with no privilege to enable. With them C5 is done. No
contract changed.

Manuel's prompt was Session 64's (steps d and e, with #152 and #153 to
settle with him). The repo was at Session 65's close, with d and e landed
and #152 and #153 closed; the session reported the mismatch and went on
with what the prompt left, f and g (CLAUDE.md *Session protocol*, step 6).

## Start and resulting state

- Start:
  - `main` = `origin/main` = `8597f57`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest skipped by path);
  - 22 ADRs / 23 SPECs, 55 open debts (55 rows) and no Known CI debt. The
    repo agreed with Session 65's handoff.
- Before the first gate, 4.1 GB of 15.8 GB of memory was free and `C:` had
  44.3 GB free (`target\` held 8.0 GB). Docker Desktop was down; nothing
  this session needed it. The gates ran with `-j 2`, `CARGO_INCREMENTAL=0`
  and the profiles' debug info off.
- Resulting `main`, in order:
  - `976d1c5`, the adoption's code 10 with the real lock (pull request
    #115, its two commits squashed);
  - `83402db`, `cg-setup`'s host test (pull request #120, its two commits
    squashed);
  - this close's commit.

  Pull requests #115 and #120 are closed and their branches deleted, local
  and on origin. The probe pull requests #114, #116, #117, #118 and #119
  are closed without landing and their branches deleted. No other branch
  is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### The adoption's code 10 with the real lock (`976d1c5`, PR #115)

`package_adoption_service` gains
`package_adoption_a_held_lock_is_code_10_before_any_change`, the SCM test
that S59's self-review left weak (`docs/handoff-session-59.md`, "6.2 the
lock, code 10 ... weak: an injected closure"). With the runner's
Administrator holding the instance lock (`instance_lock::acquire`), a start
with `--adopt-staged-config` and the anchor's fingerprints stops with code
10 and SPEC-024 §1's line during `START_PENDING`, before `RUNNING`. The
staged files keep their bytes, owner and DACL; `<P>` and the data root
keep their owner, DACL and names; no `agent.toml` or `server-ca.pem` is
made, `logs` is not created and nothing enrolls. With the lock released,
the same start reaches `RUNNING` with the staged files adopted, so the
refusal was the lock's.

### `cg-setup` against the host (`83402db`, PR #120)

- `agent/cg-setup/tests/package_setup_host.rs`, built only with
  `cg-setup`'s new test-only `host-tests` feature (`required-features`):
  - its guard fails without acting unless `GITHUB_ACTIONS` is `true` and
    `RUNNER_ENVIRONMENT` is `github-hosted`, no `CyberGuardAgent` service
    exists and neither `%ProgramData%\CyberGuard` nor
    `%ProgramFiles%\CyberGuard` exists; `Host` removes `<P>` on every exit
    path after it;
  - it creates the anchor's source and a bootstrap file owned by
    Administrators with a protected DACL for SYSTEM and Administrators;
  - it runs `cg-setup.exe preflight --bootstrap=<file>` and then `reverse`
    through `CreateProcessAsUserW` under the runner's primary token with
    every privilege but `SeChangeNotifyPrivilege` deleted
    (`CreateRestrictedToken`'s `PrivilegesToDelete`), asserted to keep that
    one privilege;
  - after preflight (exit 0): `<P>`, the data root, the staged anchor, the
    staged configuration and `setup-attempt.json` each owned by
    Administrators with a protected DACL of allow entries naming SYSTEM and
    Administrators alone; the staged anchor's certificate; the token in the
    staged configuration and no `setup` table; the record's four keys and
    values (`format` 1, the four created names, the fingerprint,
    `service_running` false);
  - after reverse (exit 0): none of the five names exists, and no service
    was created.
- `package-ci.yml`: a `host` job on `windows-latest` runs
  `cargo test -p cg-setup --features host-tests --no-fail-fast --test
  package_setup_host -- --test-threads=1`, with
  `actions-rust-lang/setup-rust-toolchain` pinned by its commit
  (`166cdcf…`, `v1.17.0`); the workflow's paths gain `agent/**`,
  `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and `.cargo/**`. That
  is also C6's "`agent/cg-setup/**` in its paths".
- `CLAUDE.md` *Environment facts* and the *Windows package* bullet of the
  *Local pre-commit gate* state the feature, the job and the wider paths
  (facts only; no rule changed).

## Red evidence (CLAUDE.md compensating control 1)

Each probe ran the new test alone, in a step with `continue-on-error`, and
was closed without landing.

- **#114** (`e237bbe`, M1: the adoption without the lock): the test failed
  at the staged files' check, `open C:\ProgramData\CyberGuard\agent\
  agent.toml.setup to read it: Win32 error 2`, after the start stopped with
  10. M1 still exits 10, by the lock the start takes after the adoption
  (`service/windows.rs`), so the code alone does not tell the two apart;
  the "before any change" checks do.
- **#116** (`baa00cb`, M2: a name created in the data root before the
  lock, the review's fix): `a held lock: C:\ProgramData\CyberGuard\agent
  keeps its names`, `left: ["agent.toml.setup", "s66-probe-extra",
  "server-ca.pem.setup"]`.
- **#117** (`5d482d5`, M-a: staged files owned by SYSTEM): preflight
  `code: 40`, `create C:\ProgramData\CyberGuard\agent\setup-attempt.json:
  Win32 error 1307`, with no privilege to set that owner.
- **#118** (`feed3e7`, M-b: a reversal that changes nothing): `the reversal
  removes everything: ["C:\\ProgramData\\CyberGuard", ...]`. Its preflight
  passed: the staging itself needs no privilege on a hosted runner.
- **#119** (`1916f42`, the guard): with `RUNNER_ENVIRONMENT` set to
  `self-hosted` in the step's own process, `refusing to run cg-setup
  against the host outside a GitHub-hosted runner`, then `PROBE:
  C:\ProgramData\CyberGuard absent`.
- The control runs: `0e8f23d`'s rust-ci (the SCM step green with the new
  test), `1ca5ad7`'s package-ci (`test
  package_setup_host_preflight_then_reverse_with_no_privilege ... ok`).

## Decisions

### Anticipated (Claude Code)

- **f holds the lock in the test process** with the agent's
  `instance_lock::acquire`, as install_ac_007's code-10 SCM test does.
  Discarded: a real console agent, as the migration's test has, which
  needs an enrolled console host for the same mutex.
- **f's control releases the lock and repeats the start**, which adopts:
  without it a refusal for another reason with code 10 would pass.
- **g is a `cg-setup` test target behind its own `host-tests` feature, in
  its own `package-ci` job.** Discarded: `scm-tests` (rust-ci runs it, and
  CLAUDE.md keeps tests that run `cg-setup` against the host to
  `package-ci`); a Node test (it cannot build the token); a step of the
  `pins` job (serial with the .NET restore).
- **The toolchain action pinned by its commit** (`v1.17.0`), as the job's
  `setup-dotnet` is. Discarded: `@v1`, the major tag of debt #113.
- **g's guard adds the product directories** to
  `refuse_unless_disposable_runner`'s checks: the test writes under the
  machine's `%ProgramData%`.

### Reactive corrections

- **The review of `0e8f23d`** (fixed prompt): the "before any change"
  check was weak in breadth (no listing of `<P>` and the data root), and
  the doc said the directories keep bytes; `07e1bab` lists them, probe
  #116. The fresh pass on that fix found nothing to fix.
- **The review of `1ca5ad7`** (fixed prompt): the test pinned the setup's
  access masks and inheritance flags (`OICI`, `FA`), which the SPEC does not
  state, under a header saying the SDDLs were the SPEC's; it now asserts the
  SPEC's words. A failed setup-table check printed the configuration.
- **The fresh pass on that fix:** three neighbouring failure paths still
  printed the configuration (the UTF-8 and TOML errors, the token's
  `assert_eq!`); none does now. A sentence `CLAUDE.md` had gained (which
  local steps an agent-only change runs) could be read as narrowing the
  local gate, a rule on Manuel's list; it was removed before landing.
- **Probe #117's guard step proved nothing:** a step's `env:` did not
  replace `RUNNER_ENVIRONMENT` (the test ran preflight), and the
  `Test-Path` lost its backslash in the generated YAML. #119 set the
  variable in the step's script and joined the path with `Join-Path`.

### Manuel's decisions

None this session.

## Self-review (CLAUDE.md compensating control 5)

Contracts: SPEC-024 Amendment 2026-11-09, item 6.2 and *Tests*, with the
defect of `docs/handoff-session-59.md`; the Amendment 2026-11-10, *Tests*,
read with its items 5 and 7 and the Amendment 2026-11-09's item 3 and
*Tests*.

| Criterion | Test | Verdict |
|---|---|---|
| 6.2 the real lock, code 10 and §1's line, during `START_PENDING`, before `RUNNING` | `package_adoption_service.rs` `..._a_held_lock_is_code_10_before_any_change` (`refused_with_arguments`) | covered |
| 6.2 before any change | the same: staged files, directories' owner, DACL and names, no `agent.toml`, no `logs`, no enrollment | covered; transient names, a file ID and streams not compared |
| The refusal is the lock's | the same, its control | covered |
| 6.2's order against steps 1 and 3 with the real lock | `package_adoption.rs` (injected closure) | weak: the SCM test runs on a fresh host only |
| The real `cg-setup`, `preflight` then `reverse`, on a fresh hosted runner | `package_setup_host.rs` `..._preflight_then_reverse_with_no_privilege`, the guard | covered |
| A token with every privilege but `SeChangeNotifyPrivilege` deleted | the same, `token_without_privileges` | covered |
| The staged files' security; `<P>`, the data root and the record | the same, `assert_security` | covered (read as allow entries naming both, no other entry) |
| The record and its removal | the same | covered; the 64 KiB bound not asserted |
| The staging needs no privilege | the same (preflight 0 under that token; #117) | covered |
| The reversal starts nothing with `service_running` false | the same (exit 0, no service) | weak: no service exists here; the SCM verbs tests hold the `true` branch |

Dispositions that change no code:

- **f's holder is the agent's own `acquire`:** a fault common to the
  holder and the service (the mutex losing exclusivity) would not show
  here, as in install_ac_007's and install_ac_009's code-10 tests, which
  hold the lock the same way.
- **f does not show the lock held for the rest of the adopted start:** the
  service keeps the one lock it took (`_instance_lock`, `service/windows.rs`);
  outside the defect.
- **g requires both trustees and no deny entry** as its reading of "a
  protected DACL for SYSTEM and Administrators alone"; the service's
  custody check, which allows deny entries and does not require both, is
  not what the setup creates.
- **`package-ci` now runs on every agent change**, its `pins` job
  included, which reads nothing under `agent/`: the cost of one workflow
  for the package's tests.

## Gates and CI

- Local Rust gate, main checkout, `-j 2`, debug off:
  - `0e8f23d` (f): `FMT_OK`; clippy clean; "binaries 95 passed 541 failed
    0 ignored 11"; matrix "all steps passed"; `07e1bab` changed only the
    SCM test target (fmt and clippy rerun);
  - `dc75e3a` (g, tree `db76064`): `FMT_OK`; clippy clean; "binaries 95
    passed 541 failed 0 ignored 11"; matrix "all steps passed".
- Windows package, local (g touches `package-ci.yml`): `node --test
  "deploy/windows/test/*.test.mjs"` 19 passed; `dotnet restore`,
  `verify-pins.mjs` "pins verified: 4 packages at 7.0.1", `dotnet tool
  restore`, `verify-pins.mjs --tool`.
- No `Cargo.lock` or dependency changed (`host-tests` is a feature with
  no dependency); `cargo audit` did not run.
- The elevated gate does not apply: the diffs touch none of its paths
  (test targets, `agent/cg-setup/Cargo.toml`, workflows, `CLAUDE.md`).
- CI:
  - PR #115: `0e8f23d` and `07e1bab`, every workflow `success`; the
    squashed `976d1c5` on its branch, every workflow `success`;
  - PR #120: `1ca5ad7` and `dc75e3a`, every workflow `success`; the
    squashed `83402db`, rebased onto `976d1c5` with the same diff, on its
    branch, every workflow `success` (the host test "1 passed");
  - `main`: `976d1c5` rust-ci and ts-ci `success`, ALL GREEN; `83402db`
    markdown-lint, dependency-audit, package-ci, ts-ci and rust-ci
    `success`, ALL GREEN; this close's commit as its post-push report gives
    it.

## Test baselines at close

- Non-elevated `cargo test --all`: 541 passed, 0 failed, 11 ignored, in 95
  binaries.
- The SCM suites in CI: cg-agent's 36 (35 + 1); cg-setup's 32, unchanged.
- `package-ci`'s host test: 1.
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: none.
- Opened: none.
- Open at close: **55**.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays.

## How Session 67 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 55 open debts and 22 ADRs / 23
   SPECs.
2. **C6, the MSI and `package-ci`'s install tests**, as
   `docs/handoff-session-59.md#how-session-60-resumes` orders it: the
   product service's DACL and `SERVICE_START`, lock files on the
   `.wixproj`s; `package-ci` already runs on `agent/**`. Its tests run only
   in `package-ci`, with their red run on a probe pull request; a probe's
   variable for the runner goes in the step's script, not its `env:`.
3. Then C7 (the bundle, with the wizard), C8, C9 and C10 in that order.
4. Keep the release freeze.

## Environment and memory

- Reviewer worktrees under `.claude\worktrees\` were removed by the Agent
  tool; probe worktrees under `C:\tmp\` were removed with `git worktree
  remove` (one with `--force` after a permission error, then
  `git worktree prune`).
- A step's `env:` in GitHub Actions did not override `RUNNER_ENVIRONMENT`
  for the test it ran; setting it in the step's PowerShell script did.
