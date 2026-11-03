# Handoff — Session 53

Session 53, 2026-11-03, continues F2 under SPEC-024: it implements
`install_ac_009`, the agent's per-host instance lock. The code is complete
and green on a branch with a PR; it is **not on `main`**, because the
change touches the capture path and so waits for the elevated gate before
landing (*Compensating controls* §4). This session's only change to `main`
is this handoff and the pointers that cite it.

## Start and resulting state

- Start: `main` = `origin/main` = `f10f605`, clean, only `main` on origin,
  no PR open. Open debts 50; Known CI debt none; catalogs 22 ADRs / 23
  SPECs. `main`'s tip CI was green (`markdown-lint → success`). Repo and
  `docs/handoff-session-52.md` agreed; no discrepancy.
- Work done, on a branch, **not landed**:
  - `feat/s53-install-ac-009` @ `a168cf1` (one commit), PR **#79** open.
    CI on `a168cf1` is **ALL GREEN**: `rust-ci`, `ts-ci`, `markdown-lint`,
    `deploy-ci` all `success`. The push was `ffa8058`, amended to `d17ae29`
    (a Linux-clippy fix, below), to `bc293ff` (the review fixes, below), and
    to `a168cf1` (the `classify_token` test fix the elevated gate surfaced,
    *Elevated gate 1* below); the force-pushes are the branch's own,
    pre-landing (*Integration path* §3).
- Resulting `main`: unchanged but for this handoff's docs commit. The
  feature branch stays open, named here with its reason: the elevated gate's
  capture-marquee half (Manuel's) has not passed yet, and the change does
  not land without it (*Elevated gate 1*, *Waiting on Manuel*).
- Catalogs unchanged: 22 ADRs / 23 SPECs (no new document; the contract
  was the SPEC-024/ADR-0008/ADR-0010 amendments already Accepted on `main`).

## What install_ac_009 is

The contract is SPEC-024's second Amendment 2026-11-01 §1 and the ADR-0008
and ADR-0010 Amendments 2026-11-02. One capture agent may run per host.

- **The lock** (`agent/cg-agent/src/instance_lock.rs`, new): a named mutex
  inside a private namespace whose boundary descriptor carries the built-in
  Administrators SID. Windows requires every boundary SID present and
  enabled in the caller's token to enter the namespace, so only an elevated
  administrator's token or SYSTEM can create or open the object; an ordinary
  user can neither take nor squat it. The namespace's own DACL
  (`D:P(A;;GA;;;BA)(A;;GA;;;SY)`) admits only Administrators and SYSTEM
  (defence in depth). The mutex's existence — one handle held for the
  process lifetime — is the lock; a crash frees it.
- **The token distinction** (read-only from the process token,
  `classify_token`): `BUILTIN\Administrators` enabled ⇒ can create the lock
  (elevated admin or SYSTEM); else a member of Performance Log Users /
  LocalService / NetworkService, or a holder of `SeSystemProfilePrivilege`,
  ⇒ controls trace sessions outside ADR-0010's model, refused at start with
  code 1; else ordinary, goes on without the lock.
- **`main.rs`** takes the lock before `identity::ensure_identity` on the
  secure path: Acquired → run on (guard held for the run); Held → exit 10
  before enrolling, stopping no session; controlling token → exit 1; other
  lock failure → exit 1; ordinary token → on without the lock.
- **`EtwSession::open`/`open_excluding`** gain a `StartMode`: only `Locked`
  reclaims a zombie of the agent's name; `Unlocked` never reclaims, its
  `ERROR_ALREADY_EXISTS` is a privilege failure (code 9), and its stop after
  a failed start narrows to privilege failures (never 183). The
  classification lives in `types.rs` (`with_lock_mode`,
  `must_stop_after_failed_start`).
- **Runbook**: `docs/operations/install.md`'s exit-code table gains code 10
  and the new causes of 9 and 1.

The two decisions SPEC-024's resume asked to be written are in PR #79's
description: the read-only token distinction, and the boundary/DACL that
keeps a non-administrator from creating or squatting the object.

## Decisions

### Anticipated (Claude Code)

- **`Capture::Platform { holds_lock: bool }`.** The lock state reaches the
  ETW open through the `Capture::Platform` variant, which `main` sets from
  `instance_lock::acquire`. Rejected: a new parameter on `run_secure` (it
  would churn ~15 in-process test call sites that do not touch the lock).
  The ADR allows the precondition "at the caller of `EtwSession::open` or in
  a parameter the open takes"; this is both.
- **The raw error decides the stop; the mode then reclassifies.** In
  `session.rs` the pump reports the raw `OpenError`; the caller decides the
  stop from the raw error (so an unlocked 183 is not stopped) and then
  applies `with_lock_mode` for the exit code (so an unlocked 183 still
  exits 9). This ordering is the load-bearing subtlety; the fresh-context
  reviewer confirmed it.
- **Single commit.** The lock, its plumbing, the tests and the runbook are
  one behaviour change; splitting creates artificial intermediate states.

### Reactive corrections

- **Linux clippy (`d17ae29`).** `InstanceLock` implements `Drop` only on
  Windows, so the explicit `drop(instance_lock)` in `main.rs` tripped
  `clippy::drop_non_drop` on the Linux job (the local clippy ran on Windows
  only). Fixed by binding the guard to `_instance_lock` and letting it drop
  at scope end (no explicit `drop`).
- **The dev host's token controls ETW.** This machine's unelevated terminal
  token *holds* `SeSystemProfilePrivilege`, so `classify_token` ranks it
  `ControlsEtw` and the real binary exits 1 before enrolling. Three
  pre-existing real-binary secure-path tests (`capture_ac_001`'s binary
  case, `deploy_ac_015_the_binary_exits_6`, `install_ac_014_binary_secure_path`)
  assumed an ordinary/elevated token; they now skip when the running token
  controls ETW (`common::token_controls_etw`), their contracts still covered
  by their in-process twins and by an ordinary-token or elevated run. This
  is correct behaviour, not a regression: a holder can enable the privilege
  on demand, so the lock refuses it (ADR-0010 Amendment item 1).
- **Review fixes (`bc293ff`).** From the fresh-context review: added an
  end-to-end test that the binary exits 1 with the ControlsEtw line when the
  token controls ETW (non-`#[ignore]`, gated on `token_controls_etw`,
  observed green on this host); strengthened the code-10 test to assert an
  unrelated session survives the refused instance ("stops no ETW session");
  aligned the well-known-SID buffer to DWORD.

### Manuel's decisions in chat

None this session: Manuel's input since the opening prompt has been the
session's own background notifications, not chat. The opening prompt
(Session 53, the work order) stands.

## Self-review and fresh-context review

Contract: SPEC-024 §1 (install_ac_009) + the two ADR amendments.

- **Claude Code's matrix** (criterion → test → verdict): the formal AC is
  covered — a concurrent instance refused before reclaiming
  (`a_held_lock_refuses_a_second_instance_with_code_10_before_enrolling`,
  `unlocked_start_does_not_reclaim_a_leftover_and_fails_privilege`), an
  orphan reclaimed sparing another name
  (`locked_start_reclaims_an_orphan_and_spares_another_name`), the outcome
  codes/lines (`lock_outcomes_carry_their_exit_codes_and_lines`,
  `spec_lines_match`), the 183/mode classification and stop narrowing (the
  pure units), and the three token kinds
  (`classify_token_tells_the_three_token_kinds_apart`). All in
  `tests/install_ac_009_instance_lock.rs`.
- **Fresh-context reviewer** (subagent, isolated worktree, fixed prompt, at
  `ffa8058`): List 2 (contradictions) empty; the AC covered; FFI
  memory-safe and leak-free (handle lifecycle, GetLastError ordering,
  `CreateMutexW` ALREADY_EXISTS ⇔ live holder, buffer alignment); the
  raw-error-before-reclassify ordering right. Weak points, all acceptable
  and **fixed** in `bc293ff` except one recorded: the ControlsEtw
  end-to-end binary exit (added a test), the Held-stops-nothing clause
  (added an assertion), the SID-buffer alignment nit (aligned). **Recorded,
  not fixed:** "any other failure to take the lock" with a real injected
  Win32 fault is covered only by the pure mapping unit — a real fault is
  hard to force and the reviewer judged this acceptable.
- The reviewer read `ffa8058`; `bc293ff` adds only the review fixes above
  (an equivalent `main.rs` guard refactor and the test/alignment additions),
  which do not change the reviewed substance.

## Gates and CI

- **Local gate** (unelevated dev host), on `a168cf1`: `cargo fmt --all --
  check`; `cargo clippy --all-targets --all-features -D warnings`;
  `cargo test --all` all green (the real-Win32 install_ac_009 cases
  `#[ignore]`d; the three real-binary secure-path tests skipped — this
  host's token controls ETW); `node .github/scripts/agent-version-matrix.mjs`
  passed; `markdownlint` 0 issues.
- The no-reclaim behaviour was observed red first (a temporary unconditional
  reclaim made `unlocked_start_does_not_reclaim_a_leftover` fail at the
  "must not open over an existing session" assertion), then green after
  reverting the probe (*Compensating controls* §1).
- **CI**, full SHA `a168cf1`: `rust-ci → success` (its Windows job ran
  `cargo test --all` and the `#[ignore]`d real-ETW/Win32 step as the
  built-in Administrator — the new install_ac_009 real cases among them,
  `classify_token` included, so the token distinction is validated on an
  elevated runner), `ts-ci → success`, `markdown-lint → success`,
  `deploy-ci → success`. Verdict **ALL GREEN**. GitHub Actions was
  capacity-constrained all day: runs queued, re-queued and sometimes froze
  mid-step for tens of minutes; each attempt, once on a runner, passed.

## Elevated gate 1 (run by Manuel) and the fix

Manuel ran `C:\tmp\s53-gate1.ps1` on `bc293ff`. It surfaced two problems,
neither a defect in the production lock:

1. **`classify_token_tells_the_three_token_kinds_apart` failed** (the other
   three install_ac_009 real cases passed). Its "Ordinary" token was
   synthesised with `DISABLE_MAX_PRIVILEGE`, which *disables* but does not
   *delete* privileges, so `SeSystemProfilePrivilege` — held by every
   elevated administrator — stayed present, and `classify_token` correctly
   ranked the token `ControlsEtw` (a holder can re-enable it). A **test**
   assumption bug, not a `classify_token` bug. Fixed in `a168cf1`: the
   Ordinary token now *deletes* `SeSystemProfilePrivilege` and makes the
   present ETW-relevant groups deny-only; verified unelevated on this host
   (whose token controls ETW) by the new `restricted_ordinary_token_is_ordinary`
   test, and re-validated by `rust-ci`'s elevated `--ignored` step on
   `a168cf1`.
2. **The vitest marquees did not run:** Docker Desktop was not running, so
   testcontainers failed at setup (`Could not find a working container
   runtime strategy`). The `cargo --ignored` half (no Docker) ran fully and
   passed but for point 1. `C:\tmp\s53-gate2.ps1` now starts Docker Desktop
   and waits before the vitest step.

- **Elevated gate 2: PENDING.** The capture marquees (SPEC-005/006) are the
  one part CI cannot run; the production capture path is unchanged since
  `bc293ff` (the `a168cf1` delta is test-only), and `rust-ci` validated the
  agent `--ignored` tests on an elevated runner, so gate 2's load-bearing
  job is the marquees. The change does not land until gate 2 is read green.

## Test baselines at close

- Non-elevated (`cargo test --all`) on `a168cf1`: all green.
  `tests/install_ac_009_instance_lock.rs` runs 6 pure units + 2 Windows
  conditional tests that run where the host's token controls ETW (the
  ControlsEtw-binary end-to-end and `restricted_ordinary_token_is_ordinary`);
  its 4 real-Win32 cases are `#[ignore]`d.
- Elevated: the agent `--ignored` tests are GREEN via `rust-ci` on
  `a168cf1`. The ingest vitest marquees are PENDING gate 2
  (`C:\tmp\s53-gate2.ps1`); the lock does not break them (the vitest config
  runs one file at a time, so one agent holds the lock at a time, and an
  elevated marquee agent is the sole instance).

## Debts

- Closed: none. Opened: none. Open at close: **50**.
- **#141** (texts behind F2's decisions) stays open and unchanged: its
  ADR-0010 sentence "LocalSystem … is not run in CI" turns false only when
  the SCM/SYSTEM tests land (install_ac_007, a later session), which this
  session did not touch. Its texts and the probe pull request were not
  started this session (no margin after install_ac_009 and the long CI).
- The register's sweep pointer moves to this handoff's *How Session 54
  resumes*.

## Waiting on Manuel

1. **Elevated gate 2 for install_ac_009** (gate 1's re-run; *Elevated gate
   1* above). From an elevated Windows PowerShell, with the working tree on
   `feat/s53-install-ac-009` @ `a168cf1` (it is left there):

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File C:\tmp\s53-gate2.ps1
   ```

   It starts Docker Desktop if the engine is down (waits up to 180 s),
   builds the release binary, runs `cargo test -p cg-agent --ignored
   --test-threads=1` and the ingest `pnpm test` (the capture marquees);
   logs land in `C:\tmp\s53-gate2-*.log` (UTF-16). Claude Code reads them; a
   green run — above all the marquees — lets the branch land.
2. **#141's texts** and **the probe pull request** (SPEC-024 Amendment
   2026-11-01 point 4) remain, as `docs/handoff-session-52.md#waiting-on-manuel`
   had them.

## How Session 54 resumes

1. Start protocol (CLAUDE.md). `main` should be this handoff's close commit;
   the feature is on `feat/s53-install-ac-009` @ `a168cf1` with PR #79 and
   CI ALL GREEN. Recheck C:, memory and Docker before builds.
2. **If elevated gate 2 has not passed, it is the first thing** (*Waiting on
   Manuel* 1; `C:\tmp\s53-gate2.ps1`). Read its logs — above all the vitest
   marquees; diagnose any failure from them (*Compensating controls* §2).
3. **Land install_ac_009** once the gate is green (*Integration path* §3):
   if `main` moved, rebase the branch and force-with-lease; then land by
   `git merge --squash` (one-commit branch) with `git diff --cached --quiet
   feat/s53-install-ac-009` as the guard, CI green on the push; close PR #79
   and delete the branch, local and origin. Update this handoff's state and
   the roadmap's install_ac_009 status to landed in that same session.
4. **Then**, with margin: the probe pull request for the SCM/SYSTEM tests
   and #141's texts on an owner-review branch (both as
   `docs/handoff-session-52.md#how-session-53-resumes` set out).
5. Later: install_ac_007 (the service), install_ac_010 with #137 (logs),
   #87 (custody), then packaging. Keep #50/#59 explicit and the release
   freeze active.

## Environment and memory

C: had ~37 GB free at start; the non-elevated builds ran with `-j 2`,
`CARGO_INCREMENTAL=0` and debug info off. Docker was not needed (no local
marquee run). The session-close auto-memory entry is
`project-session-53-close.md`, indexed in `MEMORY.md`.
