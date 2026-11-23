# Handoff — Session 95

Session 95, 2026-11-23, built ingest's migration `0011_dashboard_rights` and the detection change
of SPEC-026 (§Data contracts §1, §Operational §7): the api's column grants on ingest's tables, the
triage history, ingest's narrowed `UPDATE` on `incidents`, the open incident's key, and grouping's
upsert on it with the techniques accumulated (incident_ac_006, incident_ac_007, deploy_ac_004's
remaining rows). Its first elevated gate was red in the marquee harness's ETW teardown, #174 again;
on Manuel's choice the harness was fixed first, in a PR of its own (`f715eab`), the change rebased
onto it, and a third elevated gate was green. #174 closed; #26's first half done; #176 opened.
Manuel gave no OK to R5's amendment this session; its branch stands. The triage routes and phase
2's pages did not start (*How Session 96 resumes*). No tag was pushed; the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `3480705` (Session 94's close), clean, no stash; branches `main`
  and `docs/tm-r5-revocation-built` (`57d7d75`, owner review), no open PR. The tip's CI was ALL
  GREEN. Counts: 22 ADRs / 25 SPECs, 54 open debts, no Known CI debt. The repo agreed with Session
  94's handoff.
- Claude Code's terminal was elevated again (`whoami /groups`: high mandatory level), so
  `cargo test --all` could not be run unelevated and #175 stays open.
- Landed on `main`, by fast-forward to the SHA each commit's CI ran, one commit per push:
  - PR #162: `f715eab` (the harness's ETW stop, #174);
  - PR #161, rebased onto `f715eab`: `13c69e6` (migration `0011` and the detection change;
    `e49805b` before the rebase), `ec581c0` (the threat model's lines; `5caf1c3`). The rebase
    changed neither commit (`git range-diff`: both `=`).
- Resulting `main`: those commits and this close's.
- Branches: `main`, and `docs/tm-r5-revocation-built` (`57d7d75`, owner review, pushed, no PR),
  which still waits for Manuel. PRs #161 and #162 closed, their branches deleted; the gate's
  worktree `C:\Users\manul\cg-s95-gate` removed (no junction in it); the review workflow's two
  worktrees and its four branches (`worktree-wf_048e4c74-fc0-*`, at `3480705`, no commit of their
  own) removed.
- Catalogs: 22 ADRs / 25 SPECs, unchanged.

## What landed

- **Migration `0011_dashboard_rights`** (`services/ingest/src/db/migrations/0011_dashboard_rights.ts`),
  run as `cg_ingest_owner` after `0010`:
  - `triage_history` (`history_id` identity, `org_id`, `object_kind` incident or alert,
    `object_id`, `field` status or assigned_to, old and new value, `changed_at`,
    `changed_by_role`) and its index `triage_history_object`;
  - `triage_history_record()`, `SECURITY DEFINER`, `SET search_path = ingest, pg_temp`, fired
    `AFTER UPDATE OF status, assigned_to` on `incidents` and `AFTER UPDATE OF status` on `alerts`:
    one row per field that changed, `now()` and `session_user`;
  - `triage_history_write_once()` behind `triage_history_write_once_row` (update, delete) and
    `triage_history_no_truncate`: `P0001`;
  - `incidents_grouping_key_unique` dropped; `incidents_open_grouping_key_unique` on
    `grouping_key` where the status is not `resolved` or `false_positive`;
  - the grants, each where its role exists: `cg_api`'s column grants on `enrollment_tokens`
    (`INSERT` of a new token's six columns, `SELECT` of all but its hash, `UPDATE (withdrawn_at)`),
    `agents` (`SELECT` of the inventory's thirteen columns, `UPDATE (revoked_at,
    revocation_reason)`), `incidents` (`UPDATE (status, assigned_to, updated_at)`) and `alerts`
    (`UPDATE (status, updated_at)`); `SELECT` on `triage_history` to `cg_incident_reader`;
    `cg_ingest`'s `UPDATE` on `incidents` narrowed to `(alert_ids, severity_id, cg_mitre,
    updated_at)`;
  - `down()` throws, as `0010`'s does.
- **Grouping's upsert** (`services/ingest/src/detect/incidents.ts`): `ON CONFLICT (grouping_key)
  WHERE status NOT IN ('resolved', 'false_positive')`, so an alert whose key has only closed
  incidents opens a new incident, notified and running its playbooks; the update appends the
  alert's techniques the incident lacks, in the alert's order (`jsonb_array_elements WITH
  ORDINALITY`, grouped by value, ordered by first position).
- **The tests**: `migration-0011-dashboard-rights.test.ts`, `incident-ac-006-closed-incident.test.ts`
  (both closed statuses, `contained`, and a close that commits while the upsert waits on its row
  lock), `incident-ac-007-techniques.test.ts`, a `runtime-role.test.ts` case as `cg_ingest`, and
  deploy_ac_004's rows in `services/api/test/deploy-ac-004-005-postgres.test.ts`; the
  `10-roles.sh` comment names the triage history.
- **The threat model** (`ec581c0`), the Amendment 2026-11-20 (SPEC-026) executed in place for what
  `0011` makes true: R3's column grants, R7's "neither an alert's status nor an incident's is among
  the evidence an export signs", §8's "no signed mirror of `audit_log` or of the triage history".
- **The harness's ETW stop** (`f715eab`, `services/ingest/test/helpers/marquee-agent.ts`): a
  stopped session is awaited for 15 s (60 queries, 250 ms apart), the stop issued again every
  fourth query unless it was refused for access, which is final; a session left is logged
  (`marquee_agent_etw_session_left`) with each stop's exit status.

## Decisions

### Manuel's

- **The first elevated gate's red fixed, not accepted**, 2026-11-23: "Arreglar y re-gatear"
  (the diagnosis script, the harness fixed in a PR of its own, the full gate run again).

### Anticipated (Claude Code)

- **`down()` irreversible**, as `0010`: a down would drop the history, and closed incidents may
  share a key. Discarded: a down that drops the table and restores the constraint, which fails on
  such rows.
- **`EXECUTE` on the trigger functions left to `PUBLIC`**: a function returning `trigger` cannot be
  called directly. Discarded: a `REVOKE` that changes nothing reachable.
- **The index's name `incidents_open_grouping_key_unique`**; SPEC-026 names none.
- **An incident whose `cg_mitre` is `NULL` keeps none** (`jsonb_set` on `NULL`); grouping's insert
  never writes one.
- **The threat model's R3 and R7 restated with `0011`**, the credential's statements;
  the Amendment 2026-10-14 sentence at `docs/security/threat-model.md:260` left as written: it
  records a reading (CLAUDE.md *Class H-inline*).
- **Extra tests**: grouping as `cg_ingest`, and the close-during-upsert race, which SPEC-026
  §Operational §7 states and no criterion names.
- **One code commit**, so that the gated tree is the landed code tree.
- **The harness fix in a PR of its own, before `0011`**, so `0011`'s gate ran with it. Discarded:
  a commit on `0011`'s branch, which mixes a test-harness change into the migration's review.
- **#174's fix covers both mechanisms**, a stop that does not take and a slow teardown, since gate
  2 could not tell them apart; the log names which a recurrence is.

### Reactive corrections

- **incident_ac_006 counted two messages**: the shipped playbook's dry-run email is sent too. The
  test checks the "[CyberGuard] New incident" notice alone, and that no message names the first
  incident.
- **Biome's organizeImports** on three test files, and **the harness's UUID-literal rule** on
  fixed-prefix ids in two: random ids.
- **auth_ac_013 failed in Claude Code's Git Bash run**: `whoami` there is MSYS's; it passed from
  PowerShell (`C:\WINDOWS\system32\whoami.exe`). An environment fact, no change (#175's
  destination now names it).
- **Gate 1's red** (*Gates and CI*), diagnosed with gate 2, fixed by `f715eab`.
- **The first review of `f715eab`**: the 250 ms wait unasserted, fifteen useless stops on an
  access-denied session, a comment citing this handoff before it existed. Fixed before the push,
  in the same unpushed commit.

## The review (CLAUDE.md compensating control 5)

Reviewers and verifiers ran on Sonnet, in isolated worktrees, with the fixed code prompt.

`0011` and the detection change (`3480705..e49805b`, SPEC-026), two reviewers, one verifier per
item:

| Criterion | Test | Verdict |
|---|---|---|
| incident_ac_006, resolved and false_positive | `services/ingest/test/incident-ac-006-closed-incident.test.ts:78-118`; `runtime-role.test.ts` | covered (both) |
| incident_ac_006, contained joins | `incident-ac-006-closed-incident.test.ts:120-142` | covered (both) |
| §Operational §7, a close during grouping | `incident-ac-006-closed-incident.test.ts:144-211` | covered (both) |
| incident_ac_007 | `services/ingest/test/incident-ac-007-techniques.test.ts:19-97` | covered (both) |
| §Data contracts §1, the history and the index | `services/ingest/test/migration-0011-dashboard-rights.test.ts` | covered (both) |
| deploy_ac_004, `0011`'s rows | `services/api/test/deploy-ac-004-005-postgres.test.ts` | covered (both) |

- Red first, against the tree without the code: 12 ingest tests (the history missing, the old
  constraint, a closed incident absorbing the alert, the techniques not accumulated) and 6 of
  deploy_ac_004's (permission denied, the relation missing). Three passed before the code and stand
  as guards: `contained`, the repeated techniques, and `runtime-role`'s case.
- Neither reviewer found a contradiction. Fifteen items, each with a verifier, none holding: the
  irreversible `down()`, `EXECUTE` to `PUBLIC`, the index's and triggers' names, the extra tests,
  the `10-roles.sh` comment, the deploy_ac_004 refusals beyond the SPEC's rows (each follows from
  its allowlist), the `NULL` `cg_mitre`, the write-once function's `search_path`, and the
  "missing" invitations and `audit_log` rows, which `0008`'s tests already assert. No fix, so no
  fresh pass.

The harness fix (`3480705..f715eab`; #174's entry and the defect statement), one reviewer, then a
fresh pass after its fixes:

| Criterion | Test | Verdict |
|---|---|---|
| a stop that does not take is issued again | `services/ingest/test/marquee-agent-etw.test.ts`, "a stop that does not take" | covered |
| a stopped session awaited 15 s | the same file, "a stopped session still running is awaited" (61 queries, 59 waits of 250 ms, 15 stops) | covered (weak in the first pass, fixed) |
| a session left logged with each stop's status | "a session left is logged" | covered |
| a stop refused for access is final | "a session this process may not stop" (one stop, no wait) | covered (contradicted in the first pass, fixed) |
| the rest unchanged | the file's other tests | covered |

- Red first: the three new tests failed against the old helper (21 queries for 61, `["X"]` for
  `[]`, no log line), and the access-denied one against the first fix (15 stops for 1).
- Recorded, not changed: a refusal on a later re-issued stop is not final (an elevated process
  does not lose its token mid-wait); the access-denied path's log line is asserted only through the
  stuck path's; a stuck session now blocks for 15 s, up to about 30 s with the `0x80071068`
  retries, inside the hooks' 180 s.

## Gates and CI

- Local, Claude Code's elevated terminal (PowerShell), before the rebase: ingest typecheck, lint,
  build `0`, entrypoints ok, "Test Files 102 passed (102)" / "Tests 572 passed | 2 skipped (574)";
  api "Test Files 64 passed (64)" / "Tests 517 passed (517)", its typecheck, lint, build and
  entrypoints `0`; deploy tests 64, 60 pass, 0 fail, 4 skipped, `licences admitted: 432 packages`;
  markdownlint on the threat model `0`.
- Local, the harness fix on `main`: "Test Files 99 passed (99)" / "Tests 560 passed | 2 skipped
  (562)" (twice, before and after the review's fixes); with `0011` rebased onto it: typecheck,
  lint, build `0`, entrypoints ok, "Test Files 102 passed (102)" / "Tests 574 passed | 2 skipped
  (576)".
- **Elevated gate 1**, `C:\tmp\s95-gate1.ps1`, run by Manuel on `e49805b` (tree `5703e4a`) in
  `C:\Users\manul\cg-s95-gate`: `net session` exit `0`; Docker up; the release `cg-agent.exe` at
  03:11:59 after the commit's 02:15:45; the `--ignored` tests all passed, the seven of CLAUDE.md
  among them; vitest "Test Files 1 failed | 101 passed (102)" / "Tests 1 failed | 571 passed | 2
  skipped (574)", the failure `auth-ac-013-marquee.test.ts`: "the agent's ETW session
  CGAgent-KernelProcess is still running after the run and could not be stopped", raised at the
  run's await, before the test's row assertions; "detect_ac_001 drained 22 rows in 1 cycles",
  `detect_marquee_elapsed_seconds` 39.169, the images in Win32 form; no `CGAgent-KernelProcess`
  left. Step 6: the `logman` probe failed 1 of 3 runs alone, `stopEtwSessions([PROBE])` returning
  `["CgS38HarnessProbe"]` after 5584 ms; a raw 40-round start, query, stop loop: `start=0x0
  query=0x0: 38`, `start=0x803000b7 query=0x0: 2` (ERROR_ALREADY_EXISTS, the previous round's
  session still there).
- **Elevated gate 2**, `C:\tmp\s95-gate2.ps1`, the diagnosis alone, no build or suite: 40 of 40
  rounds `start=0x0 query=0x0 stop=0x0`, the stop 23 to 33 ms, the session gone in 16 to 28 ms;
  no probe left. So the session outlives its stop only right after the full suite.
- **Elevated gate 3**, `C:\tmp\s95-gate3.ps1`, run by Manuel on `13c69e6` (tree `20ab746`), the
  same worktree: `net session` exit `0`; Docker up; the release `cg-agent.exe` at 03:11:59, built
  from the same agent sources (the last change under `agent/` is `9bd8f83`, 2026-11-20; cargo found
  it fresh); the `--ignored` tests all passed; vitest "Test Files 102 passed (102)" / "Tests 574
  passed | 2 skipped (576)", the marquees among them (`auth_ac_013` 25.5 s, `net_ac_010` 25.3 s,
  SPEC-005 35.3 s, `ac-001`), "detect_ac_001 drained 152 rows in 1 cycles", `detect_marquee_elapsed_seconds`
  39.123, the images in Win32 form; no `marquee_agent_etw_session_left` for a real session; no
  `CGAgent-KernelProcess` left; the `logman` probe 3 of 3.
- CI, each push, the REST fallback:

```text
SHA e49805b (PR #161, before the rebase)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 5caf1c3 (PR #161, before the rebase)
  markdown-lint -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA f715eab (PR #162)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA f715eab (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 13c69e6 (PR #161)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA ec581c0 (PR #161)
  markdown-lint -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 13c69e6 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA ec581c0 (push, main)
  markdown-lint -> success
Verdict: ALL GREEN
```

- `deploy-ci` runs on a pull request, never on a push. This close's push: CI on its SHA
  (`event=push`), reported in the turn.

## Test baselines

- `services/ingest`: elevated, 576 tests, 102 files (574 passed, 2 skipped); at Session 94's close
  560 in 99 files.
- `services/api`: 517 tests, 64 files (512 at Session 94's close).
- `dashboard`: unchanged, 370 tests, 17 files.

## Debts

- Closed: **#174**, by `f715eab`: the harness awaits a stopped session for 15 s and issues the
  stop again; gate 3 green and its probe 3 of 3. Whether the stop failed or the teardown was slow
  was not told apart; a recurrence logs each stop's exit status.
- Changed: **#26**, its first half done by `13c69e6` (a closed incident takes no new alert; an
  incident accumulates its alerts' techniques); its second half, an email on a severity raise,
  stays with ADR-0017's later notification increment. **#175** kept: the terminal was elevated
  again; its destination now also names Git Bash's MSYS `whoami`, which fails auth_ac_013 there.
- Opened: **#176**, `docs/operations/data-protection.md:103-104` states the Postgres roles' rights
  before `0011`: `cg_api` "No agent, enrollment token or CA" and "never reads" the audit log (the
  api reads it since `0008` grants its `SELECT`), `cg_ingest` updating incidents without its narrowing, no
  triage history. A runbook a CI test reads: a branch and a PR. Destination: Session 96, first.
- Open at close: **54**.

## Waiting on Manuel

- Unchanged since Session 87's close: the Authenticode decision and its purchase; #130's legal
  readings; the commercial licence and terms; `SECURITY.md`'s contact; the isolated machines
  (M3); a penetration test; #99, #124 and #125.
- **R5's amendment**, on `docs/tm-r5-revocation-built` (`57d7d75`), as Session 93's handoff states
  it.

## How Session 96 resumes

1. Run the start protocol:
   - `main` is this close's commit; the branches are `main` and `docs/tm-r5-revocation-built`
     (owner review, unless Manuel has answered); no PR;
   - 54 open debts; 22 ADRs / 25 SPECs;
   - whether the terminal is elevated (`whoami /groups`): unelevated, run `cargo test --all` and
     close or keep #175 by its result.
2. If Manuel has given his OK to R5's amendment, land it as Session 93's handoff, step 2, says.
3. **#176**: `docs/operations/data-protection.md`'s Postgres rows restated from `0008` and
   `0011`'s grants, through a branch and a PR (deploy-ci's runbook tests), with the *bundle and
   runbooks* gate bullet.
4. **The triage routes** (SPEC-026 §Operational §6 and §Data contracts §3, the api, on `cg_api`'s grants of `0011`):
   an incident's status and assignee, an alert's status, and the history read, each change audited;
   their tests seen red first; then **phase 2's pages** with §9's forwarded-address clause. The api
   alone is off the elevated gate's paths unless the change touches ingest's detection or the
   `alerts` schema.
5. Then the tokens, agents and revocation routes with phase 3's pages, R13's restatement and the
   revocation's runbook (SPEC-026 Amendment 2026-11-22), then the audit view (phase 4), then
   **F4's agent half** after its SPEC-024 amendment.
