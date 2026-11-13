# Handoff — Session 71

Session 71, 2026-11-13, wrote the wizard's code against SPEC-024 Amendment 2026-11-13 in the three
commits Session 70 planned: the input and the decision with no dialog, the Win32 dialog, and the
setup passing its window to preflight with `package-ci`'s tests through the full window. Each
commit's tests were seen failing first, locally against stubs and mutations, and on two probe pull
requests for the tests no local terminal runs. Three fresh-context reviews and their fixes, then a
fourth pass over the whole change after them, preceded the landing. C8 did not start.

## Start and resulting state

- Start: `main` = `origin/main` = `d93a8d0`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 57 open debts (57 rows), no Known CI
  debt. The repo agreed with Session 70's handoff. 2.5 GB of 15.8 GB of memory free, 21.4 GB on
  `C:`; Docker Desktop up with Manuel's `aura_cache` and `aura_db` alone, not touched; builds with
  `-j 2`, `CARGO_INCREMENTAL=0` and debug info off.
- Resulting `main`, in order, each fast-forwarded from PR #129 one per push, the SHAs CI ran on:
  - `721e4d0`, the wizard's input and decision, with no dialog;
  - `2c20d07`, the wizard's dialog;
  - `3ce1ddc`, the setup passes its window to preflight, and the wizard's tests in `package-ci`;
  - this close's commit.
- PR #129 shows merged and `feat/s71-wizard` is deleted; the probe pull requests #130 and #131 are
  closed without landing and their branches deleted. No other branch and no worktree is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### The input and the decision (`721e4d0`)

- `cg-setup preflight` takes `--ui=`, at most once, Burn's `WixBundleUILevel`; configure and
  reverse refuse it (40). Only `4`, the full window, lets the wizard show (point 1).
- `src/wizard.rs`, on every platform: item 3's table with the wizard's states (`route`: a fresh or
  pending host, the full window, no bootstrap file, no migration variable), the second reading
  after the dialog (`route_again`, point 3: enrolled 30, untrusted 27, a migration or a console
  layout 28; keep needs the host still pending, fresh is 22), the fields (`Fields`, the token
  `Zeroizing`) validated by building the bootstrap file's document and passing it through the
  file's own parser (`inputs::parse_table`), what a closed dialog exits with (22, its cause named),
  which refusals the dialog shows and asks again (24 and 25), and the `Wizard` and `Starter`
  traits.
- `src/starter.rs`: the process that created `cg-setup`, by `NtQueryInformationProcess`, held by
  `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE)`, refused when it was
  created after `cg-setup` (an ID given again); 40 when it cannot be held.
- `src/preflight.rs`: `take_in` split into the bootstrap file's reading and `take_inputs`, which
  the wizard's `take_fields` shares (24, 25, the generated configuration by the service's checks);
  `decide` maps `HostState` to the portable table. `src/staged.rs` gains `Staged` and
  `stage_inputs`.
- `src/verbs.rs`: the wizard's route after 26 and 29: the starter held, the dialog, the starter
  still running (22), then the state, 26 and 29 read again, the record reading the second reading
  of the service. A route answer the code cannot take is 40, not a panic of the elevated process.
  The machine's wizard was none in this commit.

### The dialog (`2c20d07`)

`src/dialog.rs`: a dialog of `cg-setup`'s own from a template built in memory, with user32's
controls alone, titled "CyberGuard Agent enrollment" apart from the setup's "CyberGuard Agent
Setup": topmost, shown normally, asking for the foreground and flashing when refused; the five
fields, none pre-filled; the token's an `ES_PASSWORD` single-line edit; a refusal of 24 or 25
shown in it, the fields kept; any other refusal closing it with its code; cancel, the window's
close, 15 minutes from its showing and the starter's exit closing it with 22; the token's field
overwritten and emptied before it closes, and each control's text decoded into a string that never
grows, zeroed after use. The machine's wizard is the system's from this commit.

### The setup's window (`3ce1ddc`)

`deploy/windows/bundle/Bundle.wxs`: preflight's arguments gain `--ui="[WixBundleUILevel]"`;
`projects.test.mjs` holds the arguments; `package_bundle_host.rs` runs the setup with its full
window, driven by its controls' messages as probe #127 drove it, and the dialog by its own.
`package_bundle_host.rs` also gains `Host::run` and `Host::windowed` (a spawned setup with a
15-minute bound, which ends the setup and `cg-setup` when the test's driving fails, so that the
cleanup runs) and `Servers::anchor` and `bootstrap_to`.

## The probes (closed without landing)

| Probe | Run | Readings |
|---|---|---|
| #130, `7c401cb` then `9b06518` then `cbd39b0` (on `3c8b14a`, then `721e4d0`) | 43482833307, 43485918116 | the three new SCM tests against a mutation that reads the service neither before the dialog nor after it: the first build also skipped the downgrade, which `package_verbs` caught in `cargo test` before the SCM step (17 passed, 2 failed); rebuilt keeping it, the SCM step "32 passed; 3 failed": `a_conflicting_service_is_26_before_the_dialog` ("no dialog shows"), `records_the_service_as_read_after_the_dialog` (the record rewritten: `..., Write(Record), RewriteRecord]`), `a_service_conflicting_after_the_dialog_is_26` |
| #131, `f501b0c` (on `55a5329`) | 43487758219 | the wizard's two host tests with `Bundle.wxs` not passing `--ui`: "9 passed; 2 failed": `the_wizard_installs_and_its_cancellation_is_22` ("preflight ran with --ui=\"2\""), `the_wizard_keeps_or_replaces_a_pending_hosts_configuration` ("no window titled \"CyberGuard Agent enrollment\" within 180s", after `pending-unenrolled: exit 44 after 61.6 s`) |

## Decisions

### Anticipated (Claude Code)

- **The decision table in a portable module** (`wizard::Kind`, mapped from `HostState` on
  Windows), as Session 70's List B asked. Discarded: making `cg_agent::service::layout` portable,
  a change to the agent's crate.
- **The fields validated through the bootstrap file's own parser**: `Fields::inputs` builds the
  file's document and calls `parse_table`. Discarded: a second validation function, which could
  drift from the file's.
- **The fingerprints' field: one value per line, line breaks removed, empty lines skipped, every
  other line as typed.** Discarded: trimming spaces, which would take in the dialog a value the
  file refuses.
- **The starter: `NtQueryInformationProcess` for the parent's ID, its creation time before
  `cg-setup`'s, the handle opened after 26 and 29, right before the dialog.** Discarded: a Toolhelp
  snapshot (the same reading, more code); opening it before 26 and 29, where a 40 would hide them.
- **The machine's wizard an `Option`, none in the first commit**, so that each commit is true at
  its SHA. Discarded: a stub dialog in the first commit.
- **No common controls library version 6**: user32's controls, no manifest, nothing loaded
  outside the system directory. Discarded: version 6's visual styles, which need a manifest and a
  check of its loading under `/DEPENDENTLOADFLAG:0x800`. The dialog is classic in look and scaled
  by the system on high DPI, which the isolated machine reads with point 7.
- **A modeless dialog with its own loop, and a 250 ms timer that reads the starter and the
  deadline.** Discarded: waiting on the starter's handle (`MsgWaitForMultipleObjects`), which the
  `Starter` trait would have to expose, so that the tests' stand-in could not.
- **The dialog's title "CyberGuard Agent enrollment"**, apart from the setup's window. Discarded:
  "CyberGuard Agent setup", which the tests could not tell from Burn's "CyberGuard Agent Setup".
- **The token's field overwritten with as many characters, then emptied, before the dialog
  closes** (best effort on the control's buffer, which point 6 leaves to Windows).
- **SCM tests of the second reading** (26 before the dialog, the record's reading, 26 after it),
  which Session 70's plan did not list, with their own probe.
- **The pending host in `package-ci` made by an installation to an unreachable enrollment URL**
  (exit 44 read on probe #131). Discarded: writing a configuration under the data root by hand.
- **C8 not started**: the landing alone takes three pushes to `main` with CI green on each.

### Reactive corrections

- **The dialog's loop did not wake when a message sent from another thread closed it**:
  `GetMessageW` handles a sent message within itself; the dialog tests hung (a local run killed
  after ten minutes). `close` posts `WM_NULL` to the thread, and the loop reads the outcome after
  `GetMessageW`. A user's clicks are posted messages, which did wake it.
- **The refusal's text overlapped the buttons**, read on a screenshot of the dialog shown by a
  temporary example (not committed; screenshots cropped to the dialog and deleted): the buttons
  moved below a four-line message, and a test holds that no control overlaps the next.
- **Probe #130's first mutation also skipped the downgrade**, which a local test caught in the
  probe's `cargo test` step, before the SCM step; rebuilt keeping it (`cbd39b0`).
- **The first review's fixes** (`721e4d0`, amended from `3c8b14a`): the four `unreachable!` of
  the wizard's route are 40 with a line, not a panic (`package_verbs_wizard_an_answer_it_cannot_take_is_40`,
  red against `3c8b14a`: "entered unreachable code"); `wizard.rs`'s text named a module not yet
  there.
- **The second review's fixes** (`2c20d07`, amended from `55a5329`): each control's text decoded
  into a string of its largest size, so that no partial copy of the token is freed unzeroed (a
  non-ASCII or invalid token); a failure of the dialog's message loop is 40, not 22; tests that
  the dialog's controls are its five fields, its buttons and its texts alone, and that a token is
  read as typed or refused as invalid text (each red against a mutation).
- **The third review's fixes** (`3ce1ddc`, amended from `2d8a1cb`): the `ui-level` case holds that
  Burn passes `--ui="2"`; the cancelled wizard holds that the second exit is the reversal's and
  that the token typed is in none of the places; the replacement holds `--ui="4"`; the README's
  and `Bundle.wxs`'s texts as the amendment states them.
- **Clippy**: a complex type, a doc comment's placement, and `IsWindowEnabled`, which is not in
  the features `cg-setup` enables (the style's `WS_DISABLED` read instead).

### Manuel's decisions

- None in this session: the work was inside SPEC-024 Amendment 2026-11-13, which he approved on
  2026-11-13.

## Self-review (CLAUDE.md compensating control 5)

Contract: SPEC-024 Amendment 2026-11-13 and the threat model's Amendment 2026-11-13 (SPEC-024).

### Claude Code's matrix (criterion → test → verdict)

| Criterion | Test | Verdict |
|---|---|---|
| P1 `--ui` at most once; configure and reverse refuse it | `package_cli.rs` `package_cli_preflight_takes_the_setups_window_once`; `projects.test.mjs` (preflight's and configure's arguments) | covered |
| P1 only `4`; Burn's levels 2, 3, 4; a variable cannot set it | `install_ac_002_setup_wizard.rs` `..._only_the_full_window_lets_it_show`; `package_bundle_host.rs` the quiet, passive and windowed runs and the `ui-level` case | covered |
| P2 the states the wizard shows on, with and without the window | `install_ac_002_setup_wizard.rs` `..._shows_on_a_fresh_or_pending_host_...`; `package_verbs.rs` `..._shows_only_where_point_2_says` | covered |
| P2 pending: keep (start, nothing staged) or replace | `package_verbs.rs` `..._a_pending_host_kept_or_replaced`; `package_bundle_host.rs` `..._keeps_or_replaces_a_pending_hosts_configuration` | covered; "with no argument" weak (below) |
| P3 after 26 and 29; the starter held (40), still running (22), the second reading, the record's | `package_verbs.rs` (29, 40, 22, ten second readings, keep's 30 and 22); the SCM tests (26 before, 26 after, the record's reading); `..._the_starter_is_held_by_its_creation` | covered |
| P4 fields as a bootstrap file's; a refusal names the field, never the token; 24 and 25 asked again, others close | `install_ac_002_setup_wizard.rs` (equivalence, eleven refusals); `package_verbs.rs` `..._a_refused_field_or_anchor_is_asked_again`; `package_dialog.rs` `..._a_refused_field_is_shown_and_asked_again` | covered |
| P4 no pre-fill, no picker, no control that shows the token | `package_dialog.rs` (empty fields; the controls are the fields, the buttons and the texts alone) | covered |
| P5 cancel, 15 minutes, the starter's exit, a late submission: 22, nothing changed; the time not restarted; Burn's reversal after a cancellation | `package_dialog.rs` (four closes, the time); `package_verbs.rs` `..._closed_or_a_late_submission_is_22_...`; `package_bundle_host.rs` the cancelled run (`[22, 0]`, the rollback's `0x0`) | covered |
| P6 masked, single-line, no copy or cut; kept while asked again; cleared on close; no line carries it | `package_dialog.rs` (style, password character, cut, the clipboard's sequence number, the token's field at each close); `assert_clean` on every refusal; `canary_hits` after the cancelled, installed and replaced runs | covered; copy weak (below) |
| P7 topmost | `package_dialog.rs` (`WS_EX_TOPMOST`) | covered; the foreground, the flash and the visibility are the isolated machine's |
| *Tests*: seen red first on a probe pull request | probes #130 and #131 above | covered |

### The fresh-context reviewers (fixed code-change prompt)

- **`3c8b14a`** (the first commit): List 1, the dialog's and the bundle's criteria missing (the
  later commits); weak: the real starter's 40, the anchor path's refusal on every platform, a
  refusal other than 24 or 25 reaching its exit code. List 2: point 6's zeroing (the copies of
  `check_staged_config`); production's 22 for the full window while no dialog existed. List 3: four
  `unreachable!` in the elevated process; `wizard.rs`'s text. Dispositions: the panics and the
  text fixed (above); the rest below.
- **`55a5329`** (the dialog): List 1, the bundle's criteria missing; weak: no control showing the
  token, no picker, the copy's check without a positive control, "15 minutes from the showing",
  the foreground and the flash. List 2: the bundle not passing `--ui` (the third commit); UTF-16
  decoding freeing partial copies. List 3: a loop's failure as 22; the copy test touching the
  clipboard if it fails; `pub` test seams; `Win32_System_DataExchange` for the tests. Dispositions:
  the decoding, the loop's 40 and the controls' test fixed (above); the rest below.
- **`2d8a1cb`** (the setup's window): List 1, weak: the variable on the command line, the
  cancellation's rollback and canary, the replacement's level, keep's "no argument". List 2: the
  README's and the comment's texts. List 3: `Host::run`'s bound and `taskkill /IM`; the job's time;
  another process of the session driving the dialog. Dispositions: the first three and the texts
  fixed (above); the rest below.
- **`3ce1ddc`**, the whole change after the fixes: no criterion contradicted; weak: the copy's
  check, the foreground; missing: pasting and another process's `WM_GETTEXT` (Windows' behaviour
  for `ES_PASSWORD`, not in *Tests*). List 2: point 6's and the threat model's "zeroes its own copy
  after use" against `check_staged_config`'s and `toml::to_string`'s copies (as the first review).
  List 3: the new 40s, the dialog's texts, `Machine::wizard`, the harness's bound, the starter's
  `ntdll` and `user32` (a KnownDLL) among `cg-setup.exe`'s imports.

Recorded, not blocking:

- **Point 6's text and the copies** of the generated configuration's checks: **#160**. The copies
  predate the wizard (the bootstrap file's route since C4); their consequence is the residual
  point 6 discloses (the token may stay in `cg-setup`'s process until it exits, which SYSTEM and
  elevated administrators alone read); the text's precision, the threat model's included, is on
  Manuel's list (*Waiting on Manuel*).
- **The copy's check**: a passing test touches nothing outside the process; only a regression of
  the masking would put the test's fixed token, no secret, on the clipboard; the cut's check is
  the direct one.
- **Keep's "with no argument"** is not observable in `package-ci` (the agent logs no adoption);
  the SCM test `package_scm_the_start_passes_its_arguments_or_none` and the record's empty list
  cover it, and the kept configuration is read byte for byte.
- **The foreground, the flash and the visibility**: the isolated machine's (point 7).
- **`Host::run`'s 15-minute bound and `taskkill /IM cg-setup.exe`**: on a disposable runner where
  only the test's setups run; the bundle job took 18.1 minutes of its 90 before this change.
- **Another process of the session driving the dialog** (the test drives it as an elevated
  administrator): the threat model's accepted residual that UIPI is not a security boundary; what
  a process of lower integrity can do is the isolated machine's to read, with the keys.
- **The `pub` items the tests reach** (`dialog::ask`, `Settings`, `Observe`, the controls' IDs,
  `Machine::wizard`): the executable uses `Settings::SETUP` and the system's wizard alone.

## Gates and CI

- Local Rust gate, the first commit (on `3c8b14a`'s tree before the review's fixes): `cargo fmt
  --all -- --check` clean; clippy `--all-targets --all-features -D warnings` clean; `cargo test
  --all` "binaries 96 passed 559 failed 0 ignored 11"; "agent version matrix: all steps passed".
  After the fixes (`721e4d0`, `cg-setup` alone changed): fmt and clippy clean, `cargo test -p
  cg-setup` "14 binaries, 153 passed, 0 failed".
- The second commit (on `55a5329`'s tree before the title and the review's fixes): fmt and clippy
  clean; `cargo test --all` "binaries 97 passed 566 failed 0 ignored 11"; "agent version matrix:
  all steps passed". After them (`2c20d07`, `cg-setup` alone changed): fmt and clippy clean,
  `cargo test -p cg-setup` "15 binaries, 162 passed, 0 failed", `package_dialog` "9 passed".
- The third commit: `node --test "deploy/windows/test/*.test.mjs"` "pass 27, fail 0"; `dotnet
  restore pins/Pins.csproj`, `node pins/verify-pins.mjs`, `dotnet tool restore`, `node
  pins/verify-pins.mjs --tool`, each exit 0; with `CG_RELEASE_VERSION=1.2.3`, `cargo build
  --release -p cg-agent -p cg-setup`, the MSI's and the setup's builds, each exit 0; "bundle
  version matrix: all cases passed" (10 cases), again on `3ce1ddc` after its fixes with the setup
  rebuilt ("0 Errores"); clippy of `package_bundle_host` with `host-tests` clean; markdownlint
  clean. No test that installs anything ran on the development host.
- No elevated gate: the change touches no path of *Developer-local SPEC-005 marquee validation*
  §5 or SPEC-006's §6, and no lockfile (`Cargo.lock` unchanged; `windows-sys` features alone).
- CI on PR #129: `721e4d0` ALL GREEN; `2c20d07` ALL GREEN; `3ce1ddc` ALL GREEN, `package-ci`'s `bundle` job "11 passed; 0 failed" in 2765.77 s, the job 50.4 minutes of its 90 (18.1 before the wizard's tests). `main` after each
  fast-forward (push): `721e4d0` dependency-audit, ts-ci, package-ci, rust-ci, ALL GREEN;
  `2c20d07` the same four, ALL GREEN; `3ce1ddc` markdown-lint, ts-ci, rust-ci, package-ci, ALL
  GREEN (dependency-audit not triggered: no lockfile or manifest of its paths); this close's
  commit, as its post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 97 binaries; 566 passed, 0 failed, 11 ignored on `55a5329`'s
  tree; `cg-setup` gained 9 tests after it (162 in its 15 binaries).
- Elevated: unchanged from Session 70 (`--ignored` 11 passed; vitest 86 files, 504 passed, 2
  skipped); no elevated run in this session.
- `rust-ci`'s SCM step: 35 tests (32 and the wizard's 3), as probe #130 counted them.
- `package-ci`: `bundle` 11 tests (9 and the wizard's 2); `deploy/windows/test` 27; `msi` and
  `host` unchanged.

## Debts

- Closed: none.
- Opened: **#160**, the token's copies made by the generated configuration's checks are not
  zeroed: `cg_agent::service::check_staged_config` parses the configuration twice
  (`config::load_from_str` into `AgentConfig`, whose `enrollment.token` is a plain `String`, and
  `toml::Value`), and `toml::to_string` grows its buffer in `staged_config`; the strings holding
  the token are freed unzeroed, once per bootstrap file since C4 and once per wizard's submission
  now. SPEC-024 Amendment 2026-11-13, point 6 ("Preflight reads it at each submission into memory
  it zeroes after use") and the threat model's "zeroes its own copy after use" name only the
  reading's memory and the control's. Destination: Manuel's choice (*Waiting on Manuel*) between
  a text correction of point 6 and the threat model, or a zeroed secret type for
  `enrollment.token` in the agent's configuration; C10's runbook names the copies either way.
- Open at close: **58**.

## Residuals with their destination

- The isolated machine: the dialog's visibility above the setup's window, the foreground and the
  flash, the keyboard at once or after one click, its scaling (system DPI scaling, no manifest),
  closing the setup's window while it shows, and what a process of lower integrity in the session
  can read of the keys or send to the dialog; with Sessions 69's and 70's.
- The runbook (C10): Session 70's disclosures, `-quiet` for SYSTEM and deployment tools, and
  #160's copies.

## Waiting on Manuel

- **#160's text**: whether SPEC-024 Amendment 2026-11-13, point 6, and the threat model's
  Amendment 2026-11-13 are corrected to name the copies the generated configuration's checks make
  (an amendment on his list: the security posture), or the agent's configuration gains a zeroed
  secret type for `enrollment.token` (a change to `cg-agent`'s configuration types). Recommended:
  the text correction, with C10's runbook naming the copies; a secret type changes the agent's
  parsing for every route and gains nothing against SYSTEM or an elevated administrator, who
  alone read the process.
- The release-tag freeze stays.

## How Session 72 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 58 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answer on #160, if given: a text amendment drafted on an owner-review branch, or the
   code change on its own branch.
3. **C8, the release**, as Session 58's plan lists it: `release.yml`'s six assets, the source
   archive, the MS-RL notices (debt #130), the version checks, and the import and message-table check
   on the published binary; with #158 (the release job builds the MSI without `CgInjectFailure`
   and checks that the MSI it publishes carries no custom action), #149 (`ubuntu-latest`'s
   `kill EACCES` before `release.yml` changes, and SPEC-023's line) and #159 (`cargo audit`'s three
   allowed warnings moved within their ranges, or put to Manuel). Contract first: what the release
   publishes and how the setup is named, if a SPEC-024 text is needed, reviewed and, where it
   touches packaging or the deployment contract, put to Manuel before code. Then C9 (purge) and
   C10 (the runbooks with the residuals above, #87's migration items and #150's).
4. Keep the release freeze.

## Environment and memory

- `target\release\` holds `cg-agent.exe` and `cg-setup.exe` built at `CG_RELEASE_VERSION=1.2.3`
  from the second commit's tree before its review's fixes (the package gate's), and
  `deploy\windows\msi\bin` and `bundle\bin` their 1.2.3 builds; none is a release.
- Docker Desktop stayed up with Manuel's `aura_*` containers alone, not touched.
- Updated memory: the session's close; `reference-burn-wix-facts.md` gains the dialog's loop and
  driving facts (a sent message does not wake `GetMessageW`; `WM_SETTEXT` and posted
  `WM_COMMAND` drive an elevated dialog from the runner's elevated test).
