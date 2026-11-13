# Handoff — Session 70

Session 70, 2026-11-13, wrote the wizard's contract: SPEC-024 Amendment 2026-11-13 and the
threat model's, approved by Manuel, from an audit of WiX's source and the probe pull request #127,
which read that Burn skips the rollback of a preflight it abandons on a cancellation. A RustSec
advisory published after Session 69's close, RUSTSEC-2026-0538 on `quinn-udp`, was fixed and
landed after the elevated gate. The wizard's code did not start: the session closed at a green
`main` with the plan below rather than land a partial wizard.

## Start and resulting state

- Start: `main` = `origin/main` = `bab081a`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 56 open debts (56 rows), no Known CI
  debt. The repo agreed with Session 69's handoff. 4.7 GB of 15.8 GB of memory free, 27.1 GB on
  `C:`; Docker Desktop down at the start (the elevated gate started it); builds with `-j 2`,
  `CARGO_INCREMENTAL=0` and debug info off.
- Resulting `main`, in order:
  - `4935a4f`, `quinn-udp` 0.5.16 for RUSTSEC-2026-0538 (PR #128, fast-forwarded, the SHA CI and
    the elevated gate ran on);
  - `efccc4c`, SPEC-024 Amendment 2026-11-13, the threat model's Amendment 2026-11-13 (SPEC-024)
    and the SPEC catalog's line (docs, squashed from the owner-review branch `review/s70-wizard`
    after Manuel's approval, marker removed);
  - this close's commit.
- The probe pull request #127 is closed without landing (its head stays at
  `refs/pull/127/head`, `f0cf1a8`); PR #128 shows merged. No other branch and no worktree is
  left.
- Catalogs: 22 ADRs / 23 SPECs; the SPEC catalog gains the Amendment 2026-11-13's line.

## What landed

### RUSTSEC-2026-0538 (`4935a4f`)

The probe's `dependency-audit` run failed on an advisory published after Session 69 closed:
`quinn-udp` 0.5.14, "Receive loop can spin after a truncated datagram", fixed in 0.5.16.
`cargo update -p quinn-udp` adds no crate; `windows-sys` 0.60.2 and `windows-targets` 0.53 leave
the lock, and `socket2` (which the agent links through tokio and hyper-util), `anstyle-query` and
`anstyle-wincon` (through clap) keep their versions and resolve to `windows-sys` 0.61.2, which the
agent already linked. No crate compiles `quinn-udp` on a target the
workspace builds. Since the agent links a crate the change moved,
`docs/security/dependency-review.md` asked the elevated gate, which Manuel ran.

### SPEC-024 Amendment 2026-11-13 and the threat model's (`efccc4c`)

The wizard of the Amendment 2026-11-09, item 1:

- preflight takes `--ui=[WixBundleUILevel]` (read: `2` quiet, `3` passive, `4` full window); the
  dialog shows only at `4`, with no `CG_BOOTSTRAP` or migration variable, on a fresh host or a
  pending one, which it may keep (keeping needs the host still pending: enrolled since is 30,
  fresh since is 22);
- it shows after the state, 26 and 29; its fields are the bootstrap file's keys under the same
  checks, the anchor's path typed with no file picker, the fingerprints never computed from the
  anchor, the token masked, copy and cut refused, paste allowed; a refused field (24) or anchor
  (25) is asked again in the dialog;
- on submission, the starting process (held by a handle opened before the dialog; 40 if it
  cannot be opened) must still run, and the state, 26 and 29 are read again;
- a cancellation, 15 minutes from its showing, or the starting process's exit closes it with 22
  and nothing changed;
- the dialog is topmost, asks for the foreground and flashes when refused; the isolated machine
  reads it from a setup started from an administrator's desktop with a real elevation prompt:
  visible above the setup's window, taking the keyboard at once or after one click; otherwise
  item 1 reopens for the wizard alone;
- point 8 corrects the Amendment 2026-11-12 (second), point 6, as read: Burn abandons a running
  preflight on a cancellation and skips its rollback; the dialog can still accept a submission
  while the setup's window shows its last page after a cancellation, which then stages and stops a
  service that preflight did not read as enrolled;
- point 9 reads §Operational §1's four states as the setup's result and the runbook (C10);
- the runbook gains: the clipboard (and its history and synchronisation), an imitating window,
  the token in the control's memory, `LocalDumps` for `cg-setup.exe`, `-quiet` for SYSTEM and
  deployment tools, and a token expiring while the dialog waits.

The threat model adds to §1 (Information Disclosure, Spoofing, Tampering, Denial of Service), corrects
the Amendment 2026-11-12 (SPEC-024)'s "can go on alongside its own reversal", and accepts the
dialog's residuals (an imitating window, the clipboard, the control's memory, an over-the-shoulder
elevation's desktop; UIPI is not a security boundary).

## The probe (closed without landing)

| Probe | Run | Readings |
|---|---|---|
| #127, `e2ec39b` then `f0cf1a8` (on `bab081a`) | 43444878349 (failed: the job lacked `DOTNET_CLI_TELEMETRY_OPTOUT=1`), 43445276014 | an instrumented `cg-setup` (`--ui=` logged its session, window station, desktop, parent and foreground, showed a window and held) under a setup run quiet, passive, full to its end, and full with the setup's window closed during preflight: `--ui="2"`, `"3"`, `"4"`; session 2, `WinSta0\Default` visible, foreground lock timeout `2147483647`; the window foreground at once (no one at the console, no prompt: not evidence); on the close, `i319 … Preflight, result: 0x80070642`, `i365: Skipping rollback of package: Preflight due to abandoning its process`, the setup still running after 180 s on its last page and the elevated parent alive through the 150 s hold; a 22 followed by the reversal |

## Decisions

### Anticipated (Claude Code)

- **A research workflow (four readers and a critic) over WiX v7.0.1's source, cg-setup's code,
  Win32 and the runner, and the existing contract, then the probe, before drafting.** Discarded:
  drafting the wizard from item 1's sentence and memory.
- **`cargo update -p quinn-udp` alone, with the elevated gate the review document asks.**
  Discarded: an ignore entry (Manuel's to accept); a manual lock edit keeping `windows-sys` 0.60.2.
- **The amendment drafted in a separate worktree** (`C:\tmp\s70-wt`) so that the elevated gate's
  checkout of the repository stayed clean. Discarded: waiting for the gate.
- **Close at a green `main` with the code plan below.** Discarded: starting the wizard's code in
  this session and checkpointing it half-built (the code is three commits, a probe and reviews).

### Reactive corrections

- **The amendment's text after six fixed-prompt reviews**: List A items, each applied — the failure
  page does not show the code; the staging/record order; the starting-process check's item; the
  dialog outliving the run (now it closes when the starting process exits, Claude Code's); the
  timeout as 43 or 44; a fresh host may hold a service; Manuel's choices attributed exactly;
  pull request #127 open (closed); "no interactive user" (no one at the console); "never an
  enrolled one" narrowed; "interactive route" kept for the launch, the wizard's condition named
  "the setup's full window"; §Data contracts §3's list acknowledged; the control memory's
  disclosure in the SPEC; the catalog's quotation; keeping a host that changed (30/22); point 8's
  wording. The sixth pass (`d3bcd81`) found no List A item; two disclosures added after it (UIPI is
  not a boundary; over-the-shoulder elevation) were not reviewed again, which Manuel was told.
- **"The starting process alive" covers less than Manuel's first choice assumed**: the probe read
  the parent alive on the setup's last page after a cancellation. Manuel chose again (below).
- **The probe's first run** failed at the pins' telemetry check: the job's environment gained
  `DOTNET_CLI_TELEMETRY_OPTOUT=1`.
- **The fourth review was first started with a wrong SHA** (launched before the commit existed); it
  was stopped and started again on `0fdd749`.
- **PR #128 landed without the fresh-context code reviewer** of CLAUDE.md compensating control
  5; the reviewer ran after the landing (*Self-review* below). A deviation from the control,
  disclosed here.

### Manuel's decisions

- 2026-11-13, in chat (question tool), each in the recommended option: the wizard on a pending
  host with a choice to keep its configuration; 22 for a cancelled wizard; the isolated
  machine's reading "visible above the setup and one click"; the starting process's check and
  15 minutes.
- 2026-11-13, in chat, after probe #127: "Padre vivo + timeout + divulgar (Recomendado)".
- 2026-11-13, in chat: the elevated gate of PR #128 run ("Lo lanzo ahora"; then "gate terminado,
  lee os logs").
- 2026-11-13, in chat: SPEC-024 Amendment 2026-11-13 and the threat model's approved ("si").

## Self-review (CLAUDE.md compensating control 5)

- **The amendment's text.** Claude Code's pass: each claim made in more than one place (the
  conditions in point 2, the catalog and the threat model's context; 22 in points 2 and 5; point 3's
  order against `agent/cg-setup/src/verbs.rs:84-98`) agrees; the cited passages exist (SPEC-024
  :1095-1096, :1531-1536, :1571, :2398-2399; threat model :355, :429). Six fresh-context passes
  with the fixed text prompt, on `5f14ae5`, `0a56a77`, `2f0f4f7`, `0fdd749`, `351f3f5` and
  `d3bcd81`: List A items 4, 3, 4, 4, 2, 0, each applied (above). List B items not applied, recorded
  for the implementation: the unit tests "on every platform" need the decision in a portable
  module (`HostState` is Windows-only); the dialog test needs an injectable starting process and
  timeout; comctl32 v6 against `/DEPENDENTLOADFLAG:0x800`; a parent-PID reuse check by creation
  time; the 24 line of the generated configuration must hold no token; a timing difference
  between 40 (parent gone before the handle) and 22 (gone after); whether a cancelled upgrade of
  a pending host runs the older setup's restore beside the abandoned preflight (not read).
- **PR #128** (contract: the advisory and `docs/security/dependency-review.md`). Claude Code's
  matrix: the advisory gone → `cargo audit` exit 0 locally and `dependency-audit` success on
  `4935a4f` (PR and push) — covered; no crate added → the lock's diff (removals and one version)
  — covered; the agent's links moved → the elevated gate — covered. The fresh-context reviewer ran
  after the landing; its lists and dispositions are below.

The fresh-context reviewer of PR #128 (fixed code-change prompt, after the landing):

- List 1: the advisory's range, `cargo audit`, the declared range, no crate added, no ignore, the
  branch and CI, the elevated gate before the landing — covered; "only the intended packages
  moved" — weak, below. It reproduced the lock with `cargo update -p quinn-udp --offline`
  (identical) and the crate's checksum.
- List 2: none.
- List 3: `socket2`, `anstyle-query` and `anstyle-wincon` move from `windows-sys` 0.60.2 to
  0.61.2 with their versions unchanged, the agent compiling all three — recorded above (the
  commit's message named only the two `anstyle` crates); the elevated gate covers them.
  `quinn-udp` 0.5.16 resolves to `windows-sys` 0.52.0, already locked and not compiled by the
  agent. No change to authentication, authorization, cryptography, privileges or collected data.
- Outside the change: `cargo audit`'s three allowed warnings (RUSTSEC-2026-0440 `astral-tokio-tar`
  unmaintained, RUSTSEC-2026-0486 `rustix` unsound, RUSTSEC-2026-0386 `tokio` unsound) have no row
  in `docs/debts.md`, though `docs/security/dependency-review.md` makes a new one a debt; Sessions
  60 and 64 recorded them only in their handoffs. Disposition: **#159** opened.

## Gates and CI

- Local Rust gate on `4935a4f`: `cargo fmt --all -- --check` clean; clippy `--all-targets
  --all-features -D warnings` clean; `cargo test --all` "binaries 95 passed 542 failed 0 ignored
  11"; "agent version matrix: all steps passed"; `cargo audit` exit 0, "3 allowed warnings found"
  (the same three as Session 69).
- Elevated gate on `4935a4f` (tree `0298171`), Manuel, 2026-11-13 17:22–17:33: `net session exit:
  0`; `HEAD = 4935a4f85fed7c6102011266402292356abb0273`; the release `cg-agent.exe` written 17:23:43,
  after the commit (15:43:18); `cargo test -p cg-agent --no-fail-fast -- --ignored
  --test-threads=1`, 11 passed, 0 failed (`process_ac_004`, `process_ac_007`, `process_ac_009`,
  `capture_ac_006`, `net_ac_008`, `net_ac_009`, `auth_ac_012` and the four `classify_token`
  cases); vitest `Test Files 86 passed (86)`, `Tests 504 passed | 2 skipped (506)` (the two skips
  of the baseline, in `servercert-ac-renewal` and `deploy-ac-014-server-names`); `detect_ac_001
  captured image_file_name — probe: …\winword.exe; child:
  C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`, `detect_ac_001 drained 57 rows in 1
  cycles`, `detect_marquee_elapsed_seconds 38.324`; no `CGAgent-KernelProcess` session after.
- CI: PR #128 `4935a4f` ALL GREEN; `main` `4935a4f` (push) dependency-audit, ts-ci, package-ci,
  rust-ci success, ALL GREEN; `efccc4c` markdown-lint success, ALL GREEN; this close's commit, as
  its post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 542 passed, 0 failed, 11 ignored, in 95 binaries.
- Elevated: `--ignored` 11 passed; vitest 86 files, 504 passed, 2 skipped.
- `package-ci`: unchanged from Session 69 (`msi` 5 and its matrix of 14; `bundle` 9 and its
  matrix of 10; `host` 1; `deploy/windows/test` 27).

## Debts

- Closed: none.
- Opened: **#159**, `cargo audit`'s three allowed warnings, RUSTSEC-2026-0440 (`astral-tokio-tar`,
  unmaintained), RUSTSEC-2026-0486 (`rustix` 1.1.4, unsound) and RUSTSEC-2026-0386 (`tokio`,
  unsound), recorded since Sessions 60 and 64 in handoffs alone, though
  `docs/security/dependency-review.md` makes a new one a debt (PR #128's reviewer). Destination:
  the next session that touches `Cargo.lock`, or C8 before the release: move each within its
  declared range as that document's *When the run is red* orders, read which ones the agent
  compiles, and put any that cannot move to Manuel.
- Open at close: **57**.

## Residuals with their destination

- The isolated machine: the dialog's visibility, foreground and focus from a setup started from an
  administrator's desktop with a real elevation prompt, its scaling, closing the setup's window
  while it shows, what a process of lower integrity reads of its keys, and how long Burn's
  elevated process runs after a cancellation there; with Session 69's (the window, its failure
  page, a real elevation prompt, Alt+F4, the files-in-use dialogs, the floor's builds).
- The runbook (C10): the Amendment 2026-11-13's disclosures and `-quiet`, with Session 69's.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays.

## How Session 71 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 57 open debts and 22 ADRs / 23 SPECs.
2. **The wizard's code**, against SPEC-024 Amendment 2026-11-13, one branch and PR, each commit
   true and green at its own SHA, pushed one per push, the acceptance tests seen failing first:
   1. *The input and the decision, with no dialog.* `agent/cg-setup/src/inputs.rs`: the wizard's
      fields (`Fields`, the token `Zeroizing`) through the checks `parse_document` applies, one
      shared validation; a portable module for point 1's signal and point 2's states (a kind of
      host mapped from `HostState` on Windows), so that its unit tests run on every platform;
      `cli.rs`: preflight's `--ui=`, at most once, configure and reverse refusing it (40);
      `preflight.rs`: `take_in` split into the bootstrap file's reading and the inputs' checks,
      `decide` gaining the wizard's decision; `verbs.rs`: the wizard's route behind a trait the
      tests fake (the starting process's handle and 40, 26 and 29 before it, the second reading
      of point 3 with 30/27/28/26/29 and keep's 30/22, the record's second reading, cancel,
      timeout and the parent's exit as 22). Tests: a new portable test target for the fields'
      equivalence with a bootstrap file and for the decision table; `package_preflight.rs` and
      `package_verbs.rs` for the Windows paths.
   2. *The dialog.* `agent/cg-setup/src/dialog.rs` (Windows): an in-memory dialog template, a
      single-line `ES_PASSWORD` token field with copy and cut refused, `WS_EX_TOPMOST`,
      `SetForegroundWindow` then `FlashWindowEx`, the 15 minutes and the starting process's exit
      in its message loop, the refusal text shown in the dialog, the token zeroed and the field
      cleared; `windows-sys` features only (no crate); decide and record whether comctl32 v6 is
      used, after a local check of its loading under `/DEPENDENTLOADFLAG:0x800`. Test: the dialog
      created hidden in the test's process and driven by its controls' messages, with an injected
      starting process and timeout.
   3. *The setup passes its window.* `deploy/windows/bundle/Bundle.wxs`: preflight's
      `--ui=&quot;[WixBundleUILevel]&quot;`; `deploy/windows/test/projects.test.mjs`;
      `agent/cg-setup/tests/package_bundle_host.rs`: a fresh installation through the wizard
      driven by its controls (operational, the token in none of the places), a cancelled wizard
      (22 with the reversal), a quiet run (22, no dialog), a pending host kept and replaced;
      driving the setup's window as probe #127 did (`refs/pull/127/head`,
      `deploy/windows/test/s70-probe.ps1`). Seen red first on a probe pull request closed without
      landing.
   The self-review in two halves per commit, and another pass after its fixes.
3. Then C8 (the release, with #158, #149 and #159), C9 and C10 (the runbook with the residuals above and
   #87's migration items).
4. Keep the release freeze.

## Environment and memory

- `target\release\` holds `cg-agent.exe` built by the elevated gate at `4935a4f` (no release
  version input) and `cg-setup.exe` from Session 69.
- Docker Desktop was started by the elevated gate; `aura_*` are Manuel's.
- Updated memory: `reference-burn-wix-facts.md` (probe #127's readings).
