# Handoff — Session 86

Session 86, 2026-11-20, landed SPEC-024 Amendment 2026-11-20's point 2 code, the roadmap's step 1:
no migration starts or continues, every refusal line that offered it reworded, and the runbooks
revised with it, R18's lines among them (`8ea9a8e`, `f25387b`, `9bd8f83`, through PR #151); then the
fixes of #167 for every installation in a change of their own (`7f1b449`, PR #153), and deleted the
two docs branches and their worktrees. F3's and F4's contracts did not start: a read-only
investigation of them is under `C:\tmp\s86-research\` for Session 87 (*How Session 87 resumes*).
No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `1d6eae9`, clean, no stash; `docs/c10-migration` (`1cf1b12`) and
  `docs/spec-024-amendment-2026-11-18` (`1f06f31`) pushed with no PR, their worktrees
  `C:\Users\manul\cg-s80-c4` and `C:\Users\manul\cg-s82-am`; no PR open; the tip's CI ALL GREEN
  (markdown-lint); 22 ADRs / 23 SPECs, 55 open debts (55 rows), no Known CI debt. The repo agreed
  with Session 85's handoff. 3.8 GB of 15.8 GB of memory free, 19 GB on `C:` (11 GB after the
  session's builds); Docker Desktop down.
- Resulting `main`: `8ea9a8e`, `f25387b`, `9bd8f83` (point 2's code and the runbooks), `7f1b449`
  (#167), then this close's commit.
- Branches: `feat/console-unsupported-code` and `docs/167-fixes` landed by fast-forward, one commit
  per push, the landed SHAs those CI ran on (so the tree guard holds by identity); PRs #151 and
  #153 closed as merged by the pushes; both branches deleted, local and on origin. The probe branch
  `probe/s86-console-unsupported` (PR #152) closed without landing and deleted.
  `docs/c10-migration` and `docs/spec-024-amendment-2026-11-18` deleted, local and on origin, and
  their worktrees removed (*The branches*).
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## The commits

1. **`feat(agent,setup): no migration starts or continues`** (`8ea9a8e`).
   - `cg-setup preflight` (`wizard::route`): a non-empty `CG_MIGRATE_LEGACY` or `CG_TRUST_ANCHORS`
     is 28 right after the untrusted state's 27, on every other state, before the bootstrap file is
     read and before `verbs::preflight` opens the SCM (26) and reads the installed version (29). The
     migration route (`Route::Migrate`, `Decision::Migrate`, `Plan::Migrate`) and its 24 are gone;
     `parse_migration`, `migration_arguments` and `scm::Route::Migration` stay as the migration's
     code, with the tests that call them.
   - `cg-setup configure`: either variable non-empty is 40, the verb's first act after the platform
     and the elevation, before it holds, reads or starts anything.
   - `cg-agent`: `parse_start` refuses any argument beginning `--migrate-legacy=` with code 2
     (`MIGRATE_LEGACY_REFUSED`); a `--trust-anchor-sha256=` without `--adopt-staged-config` stays
     2, its line naming the adoption's argument. The service's start goes through `prepare_start`,
     which reads the data root, the console layout and whether the staging or legacy directory
     counts, and refuses one that counts with code 5 before any change and without the lock
     (`migration_directory_refused`, which the adoption's refusal of a counting legacy directory
     shares); otherwise a console layout is checked as before (13) and refused later with its
     reason, and any other host is `plain`. `windows.rs` no longer calls `prepare` and no longer
     runs the old copies' pass; `prepare`, the route and `copies` stay, unreachable from the start.
   - Lines (point 3): the setup's 28 for a variable, a migration in progress (naming
     `agent.migrating`, or `agent.legacy` without `agent`, `names::migration_layout`) and a console
     layout (`DEFAULT_REFUSAL`, reworded); the purge's 28 (`cg-setup`) and 14 (`cg-agent`); the
     adoption's code 2 and its closing's 13, which no longer ends with the migration's
     `REPAIR_OWNER` (`close` now takes the repair); the legacy 27's reason in `layout.rs`. Each
     names the layout or the variable, says that the product neither adopts nor migrates it or that
     the migration is not supported, and sends to "the installation runbook, install.md".
   - Tests: `install_ac_013_unsupported.rs` (new); the setup's `package_preflight`,
     `install_ac_002_setup_wizard`, `package_verbs` and `package_service_form`; the purge's
     `assert_goes_on`, which now expects 5 beside a counting legacy directory (point 3: "a legacy
     directory beside `identity\` stays an enrolled host's, whose service then refuses it when it
     counts"); `package_adoption`; and the four CI-confined targets (*Red evidence*).
2. **`docs(operations): the runbooks after point 2's code`** (`f25387b`). `install.md`: the console
   installation a development layout; a section *No repair, and no removal by hand* that the setup's
   27 and 28, the service's 2, 5, 13 and 14 and the purge's 27, 28, 46 and 47 cite; R18's lines
   under *The Windows endpoint*; R1's citation replaced by the copies the threat model accepts.
   `upgrade.md`: 28 a row of its own, an agent run by hand a development host formatted then
   installed with the setup. `configuration.md` and `data-protection.md` lose the migration's
   clauses. The deploy tests assert the revised lines and pin the service's two new lines from
   `migration.rs`.
3. **`test(agent,setup),docs(operations): the self-review's fixes`** (`9bd8f83`): *Dispositions*.
4. **`docs(operations): #167's fixes for every installation`** (`7f1b449`), from `51ba811` and
   `2c9f74e` without their console clauses: the retries after 40, 41, 42, 45 and 1602 that run this
   version's registered setup again with a bootstrap file and without `-repair` marked not read;
   the uninstallation checks the inputs' directory first, a directory that fails taking the route of
   one that is gone; a `C:\CyberGuardSetup` that existed before is not removed but replaced by
   another new path. `install.test.mjs` pins them.

## Decisions

### Manuel's

None this session; the work is the roadmap's step 1 and the order he chose on 2026-11-20.

### Anticipated (Claude Code)

- **`prepare_start` beside `prepare`**; discarded: changing `prepare(None)`'s arms, which rewrites
  about 35 tests of code the amendment keeps ("Their code stays in the product, unreachable from
  the setup and from the service's start, with the tests that call it directly").
- **`Start::Migrate` removed** from `parse_start`; discarded: keeping the variant and refusing later
  in `windows.rs`, which leaves a parse that accepts what the service refuses.
- **The setup's migration route removed** (`Route`, `Decision`, `Plan`); discarded: leaving the
  variants unconstructed. `parse_migration`, `migration_arguments` and `scm::Route::Migration`
  stay, as the migration's code.
- **Configure's 40 in `verbs::configure`**, after the platform and the elevation; discarded: in
  `cli.rs`, before the elevation, as a command line the bundle does not form. SPEC-024's item 9
  ("Each verb checks item 1 and the elevation first"), a line Burn does form, and the amendment's
  *Tests*, which count a `cg-setup` service test for it, read for the verb. Decided after the
  first test edits went the other way, which were reverted.
- **One text sends every line to "the installation runbook, install.md"**, and the runbook gives one
  section for them all; discarded: a pointer per code.
- **The migration in progress named as "`agent.migrating`, or `agent.legacy` without `agent`"**, the
  completeness critic's point: `HostState::MigrationInProgress` does not say which; discarded:
  adding the name to `HostState`.
- **The adoption's closing takes its own repair** (the runbook) instead of `REPAIR_OWNER`, which
  told the operator to start the migration again and was reachable through the adoption.
- **The legacy 27's reason reworded** as a layout line, the conservative reading of point 3.
- **R1's citation in `install.md`** replaced by "the copies of the key outside the product's custody
  that the threat model accepts"; the threat model's accepted bullet, "the runbook discloses them
  (R1 and R5)", is left as it is, a record pointer (Session 85's round 12, list B, item 5).
- **`upgrade.md`'s 28 a row of its own**; discarded: one retry cell for two meanings.
- **#167's "entry-by-entry removal" of a `C:\CyberGuardSetup` that existed before not carried**:
  it cited the migration runbook's repair sections, never landed, and a runbook removal in a
  directory another account may own is what Session 82's lesson rules out; the runbook now says
  not to remove it and to use another new path.
- **The two branches deleted after #167's change landed**, as Session 85 deferred it.

### Reactive corrections

- A `\` line continuation in a Rust string was lost through a Bash heredoc (`layout.rs`); the
  purge's test caught it (spaces inside the line), fixed with Edit. The later edits went through
  scripts written with Write.
- The first fresh review's list 2: `install.md`'s row 2 named only `invalid config:`, and the new
  section said every listed code leaves the endpoint as found, which the adoption's 13 does not
  (`<P>` stays closed). Fixed in `9bd8f83`.
- The security pass's list B, items 6, 8 and 9: stale comments on a migrating start; "on an endpoint
  the setup installed" widened to any endpoint (R17's `<P>` made before any setup); the purge
  process's restart beside a counting migration directory is 5. Fixed in `9bd8f83` before its push.
- #167's review: "has not been read" was wider than the truth, since `package_bundle_host` runs the
  registered setup again with the wizard after 41 to 45; narrowed to a run with a bootstrap file.
  The uninstallation's directory check had no route for a directory that fails it; it now takes the
  route of one that is gone.

## Red evidence (CLAUDE.md compensating control 1)

- Local, the tests against `main`'s code with only a `prepare_start` seam (`prepare(None)`), then
  restored: `cg-agent` 9 failed (`install_ac_013_unsupported` 5 of 5, e.g. "a refusal was expected:
  Ok(Committed)"; `install_ac_012_purge` 3, "a plain start beside agent.legacy reads
  Ok(Committed)"; `package_adoption` 1), `cg-setup` 11 failed (wizard 2, e.g. "Ok(Migrate),
  expected Err(28)"; preflight 3; service form 1; verbs 5, e.g. "cg-setup: the installed agent is
  not this package's 0.1.0-dev: no installed agent" for configure). Logs: `C:\tmp\s86-red-agent.log`,
  `C:\tmp\s86-red-setup.log`.
- Probe PR #152 (`d096af8`, closed without landing), the four CI-confined targets against `main`,
  each step unable to fail its job:
  - rust-ci: `install_ac_013_migration` "0 passed; 6 failed" (the installed host beside
    `agent.legacy` "did not reach state 1 within 120s: state 4", that is RUNNING; the code-13 tail;
    the code-2 lines, `left: (1066, 5)`); `package_adoption_service` "5 passed; 1 failed" (its
    console-layout line); `package_setup_service` "40 passed; 1 failed" (configure's 40).
  - package-ci `bundle`: `package_bundle_a_migration_variable_is_28_on_a_console_layout` failed,
    "exit 42 after 3.3 s" (the migration route ran); the new row of the refusals' table passed, a
    fresh host's variable having been 28 before this change too.
- `install.test.mjs`'s #167 test failed on the text before it (`AssertionError: 40`).

## The reviews

### Claude Code's pass (code): criterion → test

| Criterion (the amendment's *Tests* and points 2 and 3) | Test | Verdict |
|---|---|---|
| configure with a variable: 40, nothing changed | `package_verbs.rs` `package_verbs_configure_with_a_migration_variable_is_40`; `setup_service/mod.rs` (CI) | covered |
| preflight: 28 on fresh, pending, enrolled, console layout, migration in progress, before any other input, nothing changed; 27 untrusted | `package_preflight.rs` `package_preflight_a_migration_variable_is_28_before_any_other_input` | covered |
| 28 before 26 and 29 | `package_verbs.rs` `..._is_28_before_the_downgrade` | covered for 29; weak for 26, which holds by order (`assess` before `Manager::open`) and needs a conflicting service, CI-only |
| malformed request: 28 | the preflight table and the new test's `malformed` row | covered |
| `--migrate-legacy=`: code 2 before any change | `install_ac_013_unsupported.rs`; `install_ac_013_migration.rs` (CI) | covered |
| no arguments beside a counting staging or legacy directory: 5 before any change | `install_ac_013_unsupported.rs` (nine hosts, no `Point` reached, trees equal); `install_ac_013_migration.rs` (CI, installed host) | covered |
| no refusal line offers the migration | the wizard's sweep, the preflight, verbs, purge, adoption and SCM assertions | covered |
| the adoption's service test asserts its line | `package_adoption_service.rs` (CI) | covered |
| the bundle case | `package_bundle_host.rs` (CI) | covered |
| the deploy tests assert the runbooks' lines | `install.test.mjs`, `upgrade.test.mjs` | covered |

### A reviewer with a fresh context

Each in an isolated worktree, given the fixed prompt for a code change and nothing else.

- **Point 2's change** (`1d6eae9..f25387b`). List 1: no *contradicted*; *weak*: 26's order, the
  adoption's legacy line, the legacy 27's line, the adoption's 13 text. List 2: configure's 40
  after the elevation (a decision, above); `install.md:489`'s "as they found it" against the
  adoption's 13; row 2's message column. List 3: unasked line changes, all within point 3; no
  change to authentication, authorization, cryptography, privileges or collected data.
- **The security pass after the fixes** (`1d6eae9..8df9751`, the commit `9bd8f83` amended), with a
  security lens (an ordinary local user planting names, junctions, links, transactions and handles):
  **list A empty**. List B: `counts()` reads a failure as not counting, harmless now that nothing
  after the reading acts on those names; no race to exploit; `custody::open_raw` opens without
  `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`, pre-existing (**#171**); the threat model's
  "names it in each refusal" is wider than the code (only the configuration's refusals add the
  reason), pre-existing; dead code with migration text, unreachable; stale comments, the R17 wording
  and the purge restart (fixed); configure's 40 order (the decision above); Burn's reversal after a
  refusal, as for any.
- **#167's change** (`9bd8f83..2102901`): the "not read" marks wider than the truth and the
  uninstallation's stop without a route (fixed in `7f1b449`); the removal's departure from Session
  85's step (a decision, above); the hard-coded `$dir = 'C:\CyberGuardSetup'` in the upgrade's and
  the removal's blocks, pre-existing and safe (the directory check stops them).

### Dispositions

Every *weak*, *contradicted* and list-2 point is fixed (`9bd8f83`, `7f1b449`) or recorded above with
its reason: 26's order (structural, its test CI-only), configure's 40 order (a decision), the dead
code (the amendment keeps it), the threat model's wording (Manuel's text; for the verb's
restatement), the hard-coded `$dir` (pre-existing and stopped by the check).

## The branches

`docs/c10-migration` (`1cf1b12`) and `docs/spec-024-amendment-2026-11-18` (`1f06f31`) deleted, local
(`git branch -D`) and on origin, after #167's change landed; their worktrees removed after checking
that neither held a reparse point (0 each, outside `node_modules`, which neither had) nor a change
(`git status --porcelain` empty), with `git worktree remove`. `1cf1b12`'s migration runbook is superseded by the
Amendment 2026-11-20; `1f06f31`'s threat-model text by its Amendment 2026-11-20 (SPEC-024).

## Gates and CI

- Local, on `feat/console-unsupported-code` before its first push: `cargo fmt --all -- --check` 0;
  `cargo clippy --all-targets --all-features -- -D warnings` 0; `cargo test --all` 0, "649 passed, 0
  failed, 11 ignored" summed; `node .github/scripts/agent-version-matrix.mjs` "agent version matrix:
  all steps passed"; the deploy bullet "ℹ tests 63 / ℹ pass 59 / ℹ fail 0 / ℹ skipped 4" and
  "licences admitted: 431 packages"; `node --test "deploy/windows/test/*.test.mjs"` "ℹ tests 86 /
  ℹ pass 77 / ℹ fail 0"; markdownlint "Summary: 0 issues in 0 files". The pins' restore and
  verification did not run: no `deploy/windows/` path changed. Commit 1 alone gave the deploy bullet
  "ℹ tests 61 / ℹ pass 57 / ℹ fail 0". After `9bd8f83`: fmt 0, clippy 0 (`cg-agent`, `cg-setup`),
  `package_adoption` 38 passed, `package_verbs` 27, `install_ac_002_setup_runbook` 2, the deploy
  bullet 59 of 63. `7f1b449`: the deploy bullet "ℹ tests 64 / ℹ pass 60 / ℹ fail 0 / ℹ skipped 4",
  `install_ac_002_setup_runbook` 2 passed, markdownlint 0.
- No elevated gate: no path of *Developer-local SPEC-005 marquee validation* §5 changed.
- CI, one commit per push:
  - PR #151: `8ea9a8e`, `f25387b`, `9bd8f83` each ALL GREEN (markdown-lint where it ran, ts-ci,
    deploy-ci, rust-ci, package-ci).
  - `main`, one push each by fast-forward, the SHA the PR ran on: `8ea9a8e` (ts-ci, rust-ci,
    package-ci), `f25387b` (markdown-lint, rust-ci), `9bd8f83` (markdown-lint, ts-ci, rust-ci,
    package-ci), each ALL GREEN before the next push.
  - PR #153: `7f1b449` (markdown-lint, deploy-ci, rust-ci) ALL GREEN; on `main`, by fast-forward, markdown-lint and
    rust-ci → success, ALL GREEN.
  - This close's commit: its post-push report.

## Test baselines at close

`cargo test --all` unelevated on this host: 649 passed, 0 failed, 11 ignored (642 at Session 83's
baseline; the new and rewritten tests of `8ea9a8e`).

## Debts

- Opened: **#171**, the custody module's opens (`custody::open_raw`,
  `agent/cg-agent/src/custody.rs:465-471`) set `FILE_FLAG_OPEN_REPARSE_POINT`, which leaves the last
  component's reparse point unfollowed, but no `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`,
  so a LocalSystem open whose earlier components pass through a junction an ordinary user planted
  in a `<P>` they made could reach a named pipe whose server identifies or impersonates SYSTEM,
  bounded by Windows' rules for mount points and by `SeImpersonatePrivilege`, which the threat model
  treats as administrative; another open of the module (`:1388`) and the setup's `read_source` set
  them. Its destination: the custody verb's restatement (roadmap step 5), which reads the same
  directories, or the release's pre-release sweep, whichever comes first.
- Re-routed: **#170**, its runbook part met by `f25387b` (R18's lines); open for F4's renewal SPEC
  and the readings on the isolated machines. **#167**, its retries and its uninstallation met by
  `7f1b449`; open for install_ac_016's run (M3).
- Closed: none.
- Open at close: **56**.

## Waiting on Manuel

As at Session 85's close (the roadmap's Part (b) §12): the Authenticode decision and its purchase; the
legal readings of #130; the product's commercial licence and terms; the security contact of
`SECURITY.md`; the isolated Windows 10, Windows 11 and Server 2019 machines (M3); a penetration
test; and the acceptance or control of #99, #124 and #125. F3's and F4's contracts will bring him
the questions *How Session 87 resumes* lists.

## How Session 87 resumes

1. Run the start protocol. `main` is this close's commit; no branch but `main`; 56 open debts; 22
   ADRs / 23 SPECs. Check memory and `C:` (11 GB at this close: builds with
   `CARGO_PROFILE_*_DEBUG=0`; `cargo clean --profile dev` if it falls under 8 GB).
2. **F3's and F4's contracts together** (the roadmap's step 3). Read the investigation first:
   `C:\tmp\s86-research\` holds four reports (`read_f3-dashboard.md`, `read_f3-security.md`,
   `read_f4-renewal.md`, `read_f3f4-coupling.md`) and a critic (`critic.md`) that checked five
   claims, found two wrong citations and nine contradictions between the reports, and listed what a
   contract writer still needs (restore semantics after renewal and revocation, the agent's
   no-restart exit for a revoked or superseded certificate as a SPEC-024 amendment, the heartbeat's
   hard-coded org, closed alerts that fire again, `audit_log.action`'s vocabulary and `detail`'s
   limit, token and revoked-agent retention, the renewal's source of truth for `notAfter`, the grant
   tests' red evidence in deploy-ci). Settle each contradiction against the repo before drafting.
3. **One message to Manuel first**, with options and a recommendation each, for what is his: a
   departure of F4 from ADR-0004 (a CSR or a server-generated key, the CA's lifetime, revocation
   in Postgres instead of Redis/Valkey, ADR-0004 §Compliance's "opens a superseding ADR"), as an
   ADR amendment drafted on an owner-review branch; the role × capability rows F3 adds (triage,
   the audit view, token issue, revocation, an assignee picker that shows emails), by SPEC-009's
   third amendment's precedent; storing the agents' hostnames (data about people) and a read view of
   `audit_log`; multi-use enrollment tokens; whether reading the audit view writes an audit row; the
   api's trust boundary if it issues tokens and revokes agents (R7). The grants themselves are
   inside ADR-0022 §4 ("a grant scoped to that table, in the SPEC that adds the route").
4. Then the F4 SPEC and the F3 SPEC (numbers in commit order: F4's server half first), contract
   before code, each through its two-half self-review.
