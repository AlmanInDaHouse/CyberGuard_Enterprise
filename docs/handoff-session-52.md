# Handoff — Session 52

Session 52, 2026-11-02, continues F2 under SPEC-024. It is a short session
of contract work: it lands the CLAUDE.md change that SPEC-024's point 4
asked for, the two ADR amendments and the SPEC amendment that the instance
lock needs. No agent code changed. The instance lock (install_ac_009), the
service, the service logs and SYSTEM custody are not implemented, and the
probe pull request has not run.

Manuel's answers in the session, in his words, are under *Decisions*. The
opening prompt was Manuel's question "falta algo que tenga que confirmar
yo?", answered from `docs/handoff-session-51.md#waiting-on-manuel`.

## Start and resulting state

- Start: `main` = `origin/main` = `2c48aa02130b941ee01638f0c971de8646443451`,
  clean, two branches on origin besides `main` (`review/s51-claude-md-scm-tests`,
  `docs/s51-service-outcomes`; no PR open). Open debts 50; Known CI debt
  none; catalogs 22 ADRs / 23 SPECs. The start protocol's CI check was the
  S51 close's (markdown-lint only, docs-only commits).
- Landed, in order, each one push with CI read for its full SHA:
  - `3c0ffa7`: CLAUDE.md, the agent's service and SYSTEM tests in CI alone
    and their red run by probe (from `review/s51-claude-md-scm-tests`
    `187b2a1`, marker stripped; Manuel: "sí al punto 1, aterriza la rama").
  - `9d91b7c`: ADR-0008 and ADR-0010 Amendments 2026-11-02 and their catalog
    rows (from review branch `adr/s52-0008-0010-instance-lock`, three
    commits squashed into one; Manuel: "sí a las dos enmiendas, aterrízalas").
  - `19fdee2`: SPEC-024's second Amendment 2026-11-01, the service's
    outcomes, and its catalog row (from branch
    `spec/s52-spec-024-second-amendment`, squashed).
- All three are docs-only; only markdown-lint ran for each, and it passed.
  The squashed commits were each covered by one run on the pushed tip.
- Branches: all review and parked branches are deleted, local and origin
  (`review/s51-claude-md-scm-tests`, `adr/s52-0008-0010-instance-lock`,
  `spec/s52-spec-024-second-amendment`, and `docs/s51-service-outcomes` at
  Manuel's "sí, borra la rama aparcada"). Only `main` remains.
- Catalogs at close: 22 ADRs / 23 SPECs (three amendments, no new document).

## Decisions

### Manuel's decisions in chat (2026-11-02)

- "sí al punto 1, aterriza la rama": ratifies the CLAUDE.md change.
- "sí a las dos enmiendas, aterrízalas": ratifies ADR-0008 and ADR-0010
  Amendments 2026-11-02.
- "sí, sigue con la enmienda de SPEC-024": the SPEC amendment's go-ahead.
- "sí, borra la rama aparcada": deletion of `docs/s51-service-outcomes`.
- On #141's texts and the first-use confirmations: "que teng oque hacer en
  los puntos 3 y 4, si es solo dar mi si, tienes mi si". Read as: the first
  use of the .NET SDK (winget, system `PATH`) and of WiX (nuget.org) is
  confirmed, with the exact version and command reported before each runs.
  It is not a sí on #141's texts, which are not written yet (they need his
  read of the text, *Integration path* §4).
- "seguimos tu propuesta": S52 reaches the instance lock's red tests and the
  probe; the service, logs, custody and packaging go to later sessions; the
  order inside a phase is Claude Code's.

### Anticipated (Claude Code)

- **Landing by squash for the two review branches.** The ADR branch had
  three commits, the first with defects the reviews found; the SPEC branch
  two. Landing each as one commit keeps every commit true at its SHA.
  Rejected: three cherry-picks (the first two false). The tree of each
  landed commit equals its branch tip, except the ADR commit, which also
  records the ratification in the two texts and the catalog rows (four lines).
- **The parked SPEC text re-applied, not rebased.** `docs/s51-service-outcomes`
  was cut from `c223091`, before `46c6cb5` landed the first amendment, so a
  rebase conflicted in three files. Only the second amendment's section was
  carried onto a fresh branch from `main`, with its ADR references pointed at
  the landed amendments. Rejected: resolving the conflicts by hand (the
  branch's first amendment is an older version of the landed one).
- **ADR-0008 narrows SPEC-017's stop after a failed start.** The code
  (`stop_after_failed_start`, `session.rs:344`) stops the session of the
  agent's name after any failed start, a refusal because the name exists
  included, so an unlocked second agent would stop a live instance's session
  even with the reclaim gated. The ADR states the narrowing; the code change
  is install_ac_009's. Found by the first review.
- **Restart policy.** The SPEC's second amendment adds that the SCM must not
  restart the failures §Operational §3 calls operator-action (invalid config,
  rejected tokens, corrupted identity, failed custody or trust, missing
  capture privilege), beside the deliberate stop and code 12. A consistency
  with that section, not a new setting.

### Reactive corrections

- ADR review 1: the amendments cited a SPEC section that was not on the same
  tree (now "lands after this one"), and the stop after a failed start
  (above). ADR review 2: a wrong line citation, the error 5 versus 1314
  description (a refusal at `StartTraceW` leaves no session), an incomplete
  test list. SPEC review: the service has no file log when it takes the lock
  (code 10 reaches it through `STOPPED` and entry 1002), and `RUNNING`'s
  definition omitted the lock.

## Self-review and fresh-context reviews

Contract reviews (*Compensating controls* §5, SPEC or ADR text), each a
subagent with the fixed prompt, in an isolated worktree, removed after:

- ADR-0008 / ADR-0010 amendments, at `0d288ed`: List A two items (above),
  fixed in `2d281b2`; List B five items, of which the helper names
  (`stop_zombie` against `reclaim_zombie`) and the availability surface were
  written into the texts, the rest left (classification mechanism, code 9
  needing a new classification in `types.rs`).
- Same, at `2d281b2`: List A three items (citation lines, the error 5 case,
  the test list), fixed in `b8203f7` and each checked against the code by
  Claude Code; no third pass, since the fixes touch citations and
  descriptions only.
- SPEC-024's second amendment, at `c2a5436`: no contradiction with the code
  and no clash in the exit codes (10 to 13 are used nowhere in
  `agent/cg-agent/src`); List A two items, fixed in the landed commit; List B:
  the lock mechanism and the test that tells "controls ETW" from "does not"
  are not stated (install_ac_009 settles them, below), an unlocked unelevated
  start enrolls before it exits 9 (one line for the runbook), and
  `SPEC-005 AC-002` still names only errors 5 and 1314 for code 9 (covered by
  the amendment's *by scope* list).

Claude Code's own pass: each statement in more than one place agrees (the
exit codes and lines across the two ADRs and the SPEC, read by the reviewer
too); each `path:line` cited was opened (`session.rs:130`, `:279`, `:284`,
`:344`, `types.rs:213-215`).

## Gates and CI

- Local: markdownlint-cli2 0.23.2 on each touched file, `0 issues`. No code
  gate: no code changed.
- CI, full SHA each, `markdown-lint → success`: `3c0ffa70f6dac0b354be5b2b4ab65e6e07d055f7`,
  `9d91b7ce5de10d7c017a716bb1b7bc73ff0e4290`,
  `19fdee2c9eba083a583f7cd1bc371cb78e474c46`. No other workflow ran (path
  filters). The close commit's SHA and CI go in auto-memory and the close
  response.
- Elevated gate: not run; no change to its paths.

## Test baselines at close

Unchanged from `docs/handoff-session-51.md#test-baselines-at-close`: no code
changed.

## Debts

- Closed: none. Opened: none. Open at close: **50**.
- **#141** (texts behind F2's decisions) is still open: its ADR-0010 sentence
  "LocalSystem … is not run in CI" turns false when the SCM/SYSTEM tests
  land, so its texts are due with or right after the probe pull request.
- Destinations unchanged otherwise; the register's sweep pointer moves to
  *How Session 53 resumes*.

## Waiting on Manuel

1. **#141's texts**, when drafted: the threat model's entry for the
   installer's elevated third-party code (and the lock's availability
   surface, ADR-0010 Amendment 2026-11-02), its dump deferral, ADR-0010's CI
   sentence, and the reopen conditions of ADR-0019 Open question 1 and
   ADR-0020 Open question 4. On an owner-review branch, his sí on the text.
2. **The elevated gate for install_ac_009**, when it is implemented: he runs
   `C:\tmp\s53-gate1.ps1` (Claude Code writes it).
3. Nothing else: the first use of the .NET SDK and WiX is confirmed (above),
   to be reported with version and command when packaging starts.

## How Session 53 resumes

1. Start protocol (CLAUDE.md). `main` should be this handoff's close commit.
   Recheck C:, memory and Docker before builds; `-j 2`, `CARGO_INCREMENTAL=0`,
   debug info off.
2. **install_ac_009, the instance lock**, contract before code:
   - the contract is SPEC-024's second Amendment 2026-11-01 §1 and ADR-0008 /
     ADR-0010 Amendments 2026-11-02; read them first;
   - the mechanism (S51 probe: unelevated `CreatePrivateNamespaceW` is
     refused with error 5): a private namespace with a boundary descriptor
     for the Administrators SID, and a mutex's existence as the lock, in a
     new module of `cg-agent`, taken in `main.rs` before
     `identity::ensure_identity`, Windows capture path only;
   - decide, and state in the PR's description: how the agent tells a token
     that controls trace sessions (Performance Log Users group,
     `SeSystemProfilePrivilege`, LocalService, NetworkService) from an
     ordinary one, read-only from the process token; the DACL or boundary
     that keeps non-administrators from creating or squatting the object;
   - the code changes the ADRs need: `EtwSession::start` calls
     `reclaim_zombie` only with the lock (a parameter or a caller-side
     check); `stop_after_failed_start` runs only for a privilege failure
     without the lock (`session.rs:279`, `:284`); `OpenError` (`types.rs:213-215`)
     classifies error 183 without the lock as privilege (code 9), with the
     lock as code 1; amend `etw_reclaim.rs`,
     `capture_ac_001_startup_failure.rs`, `net_ac_009_accepted_connection.rs`
     and any other test of `agent/cg-agent/tests/` that calls
     `EtwSession::open`;
   - tests first, seen failing for the missing behaviour: an unelevated
     leftover gives 9 and survives; elevated, a held lock refuses with 10
     before the second binary enrolls; an orphan after a kill is reclaimed
     while a session of another name runs on; a token that controls ETW
     outside the model gives code 1 and the line. They run in the elevated
     gate and in rust-ci's Windows job (the `#[ignore]`d step); none creates
     a service or runs as SYSTEM, so none needs the probe's test-only
     feature.
   - code on a branch and a PR, one commit per push, CI green on each;
     self-review matrix and a fresh-context reviewer (*Compensating
     controls* §5); then the elevated gate, run by Manuel.
3. **The probe pull request** for the SCM/SYSTEM tests (SPEC-024 Amendment
   2026-11-01, point 4; CLAUDE.md control §1): a step of rust-ci's Windows
   job that cannot fail, run on a PR closed without landing, its log the red
   evidence. Only before the first such test (install_ac_007).
4. **#141's texts** on an owner-review branch, with the probe or right
   after, as above.
5. Later sessions, in this order: install_ac_007 (the service), install_ac_010
   with #137 (logs), #87 (custody), then packaging (install_ac_015, #131,
   #140; the .NET SDK and WiX first use, reported before it runs).
6. Keep #50/#59 explicit and the release freeze active. Close with
   handoff-53, the register, README, roadmap, memory, a clean pushed `main`
   and ALL GREEN.

## Environment and memory

C: and RAM were not measured (no build ran). The session-close auto-memory
entry is `project-session-52-close.md`, indexed in `MEMORY.md`; the S51
entry's line on the parked branches is retired by it.
