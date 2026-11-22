# Handoff — Session 90

Session 90, 2026-11-22, finished roadmap step 3. One fresh-context round over the F3/F4 ADR
branch (round 14) found no List A from any of its three reviewers. Manuel then ratified the
branch and SPEC-026's, in five separate yeses, and both landed on `main` in five commits, each
green in CI:

- the ADR amendments (ADR-0004, ADR-0022 and ADR-0014);
- the threat model's amendment with SPEC-025;
- SPEC-025, `Accepted`;
- the threat model's amendment with SPEC-026;
- SPEC-026, `Accepted`.

Both owner-review branches were then deleted. No code changed and no tag was pushed; the release
freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `fb75c2e`, clean, no stash. Two other branches, as Session 89
  left them: `docs/f3-f4-adr-amendments` at `e62cb09` and `docs/f3-spec-026` at `442f096`
  (stacked), neither with a PR. No PR was open. The tip's CI was ALL GREEN (`markdown-lint`
  alone). Counts: 22 ADRs / 23 SPECs, 56 open debts, no Known CI debt. The repo agreed with
  Session 89's handoff. 25 GB were free on `C:` (35 GB at Session 89's start).
- Landed on `main`, one commit per push:
  - `51a1cda`: ADR-0004, ADR-0022 and ADR-0014 Amendments 2026-11-20, with the ADR catalog and
    the Blueprint's note;
  - `e084f26`: the threat model's Amendment 2026-11-20 (SPEC-025);
  - `ebe63b6`: SPEC-025 `Accepted`, its catalog row, and the debt register's destinations of #88,
    #89, #119 and #170;
  - `85d6016`: the threat model's Amendment 2026-11-20 (SPEC-026);
  - `ac9e604`: SPEC-026 `Accepted`, its catalog row, and the destinations of #26, #83, #98, #102
    and #136.
- Resulting `main`: those five commits and this close's.
- Branches: `main` alone. The two owner-review branches were deleted, local and on origin, after
  the landing guard below.
- Catalogs: 22 ADRs / 25 SPECs (SPEC-025 and SPEC-026 `Accepted`).

## Decisions

### Manuel's

All on 2026-11-22:

- **"2 si"**: the ten-year CA with no scheduled rotation, and its accepted threat (a holder of its
  key on the path answers a renewal with a certificate of its own, and the agent is stranded until
  enrolled anew).
- **"si"**, his next answer: read as ambiguous, since it named no point, and not taken as a
  ratification (`feedback-ambiguous-multi-point-answers`). The four open points were asked again,
  each as its own question.
- **Point 1, "Sí, ratifico"**, the recommended option: the ADR amendments and the threat model's
  amendment with SPEC-025. Its parts:
  - SPEC-025 §Operational §5's rate-limit counters in process memory, accepted under ADR-0003
    §Compliance without amending ADR-0003. The option "Sí, pero enmendar ADR-0003" was not taken.
  - The agent's host name, platform and version kept on its row without a period (data about
    people).
  - SPEC-025 §Risks' residuals: the copy flag no running service clears; a restore before a
    renewal; a server that refuses a certificate without a TLS alert; the per-process rate
    limits.
- **Point 3, "Sí, ratifico"**: the api's new rights (ADR-0022 Amendment 2026-11-20) and what a
  compromised api can then do.
- **Point 4, "Sí, ratifico"**: the threat model's Amendment 2026-11-20 (SPEC-026).
- **Point 5, "Sí, apruebo"**: the breaking change of `GET` and `POST /v1/users` (SPEC-026 §Data
  contracts §3).

### Anticipated (Claude Code)

- **Round 14 as one workflow on Sonnet 5.5.** It had three reviewers, each with the fixed prompt
  for a SPEC or ADR text in an isolated worktree, one extractor per report, and one adversarial
  verifier per List A item. This is Session 89's method, with `model: 'sonnet'` on every agent,
  as Manuel asked. Discarded: Opus, which cost about 2.4M subagent tokens a round in Session 89.
- **No further SPEC-026 round.** Session 90 changed no line of the ADR branch before its landing,
  so SPEC-026's seventh round (no List A) covered the text that landed.
- **Landing as five logical commits, not the branches' 23 review commits.** Each commit takes its
  documents from the branch tip and sets their status lines at landing, in the handoff's order:
  ADRs, threat model, SPEC-025, then SPEC-026's threat-model amendment and SPEC-026. This follows
  the landings of SPEC-024's amendments after their reviews (`b2fbb1a`, `0a3c64a`). Discarded:
  cherry-picking each review commit, which would put on `main` the unratified states between the
  rounds and no CI-run tree. Owner-review branches have no PR, so no workflow ran on them.
- **The landing guard by content.** The status lines change at landing, so the guard is not
  `git diff --quiet` but the diff of every file the branches touched between `docs/f3-spec-026`
  and `main` after the last landing. Its only lines are:
  - each amendment's status line, with Manuel's words and date, and each ADR's `Last updated`;
  - SPEC-025's and SPEC-026's `Status`, and their catalog rows, `Proposed` to `Accepted`.

  The branches were deleted only after it.
- **Debt destinations recorded at the landings**, as the register's *How the register is kept*
  allows:
  - #88: SPEC-025's two halves;
  - #89: its server half;
  - #119: SPEC-025's amendments by scope;
  - #170: the copy flag, as the server's detection of one identity on several hosts;
  - #83, #98, #102 and #136: SPEC-026's code;
  - #26: its first half to SPEC-026's code, its second half (an email on a severity raise) to
    ADR-0017's later notification increment.

  None is closed: each closes with its code.
- **The runbooks' change moved after F4's server half, not before it.** The runbooks SPEC-025 and
  SPEC-026 list in *Out of scope* describe a revocation route, a restore's effect on renewed
  agents and the users' creation from the dashboard. None of that is built, and a runbook that
  `deploy-ci` reads must describe what is. Session 89's List B also recorded that their current
  lines turn false when the server half lands. Discarded: Session 89's order (runbooks first),
  which would document behaviour before its code.
- **Close after the prompt's two tasks.** F4's server half is code, under SPEC-025's contract and
  the elevated gate; it starts in a session of its own.

### Reactive corrections

- **SPEC-026's catalog patch did not apply** over `main`: its context held SPEC-025's row as
  `Proposed`, which the landing had set `Accepted`. The two lines it adds were written by a script
  that checks its anchors, and the guard above shows them equal to the branch's but for `Accepted`.
  The failed command chain stopped before anything was staged, and nothing was committed in that
  state.

## The reviews (CLAUDE.md compensating control 5)

### Round 14, the ADR branch at `e62cb09`

- Model: Sonnet 5.5 for the three reviewers and the three extractors (no List A, so no verifier
  ran); 694k subagent tokens, 120 tool uses.
- **No List A from any reviewer.** Each report's List A was read in the raw report, not only in
  the extracted list.
- List B, read whole. These were weighed for a blocking item that was misfiled:
  - ADR-0022's "§3's '`cg_api` has `INSERT` alone'" (`:302`): the quote stands in the *§3 cg-api*
    bullet of the threat model's Amendment 2026-10-15, so the citation holds.
  - The host name stored (data about people): on Manuel's list, so it was named in point 1's
    question, and he ratified it.
  - The ADR-0003 rate-limit departure: named in point 1's question, ratified under ADR-0003
    §Compliance.
- List B for the code's contract review (SPEC-025's server half):
  - `enrollment_tokens_write_once` does not fix `token_id`;
  - `409 renewal_not_due` shares its status with the nonce's `409` (SPEC-004 FR-010), told apart
    by `error`;
  - the history trigger's `SECURITY DEFINER` function needs its `search_path` and owner pinned
    (SPEC-026);
  - the api's token insert lets the database compute `expires_at`, as `issue-token` does
    (SPEC-026);
  - the per-column grants and write-once triggers need real-role tests (deploy_ac_004).
- List B that does not block, as before:
  - SPEC-004 `:38` and SPEC-003 `:44` stay worded as before, read by scope through SPEC-025's
    header;
  - the Blueprint's §7 bullets above its note;
  - the catalog edge "R5 left to the renewal SPEC" in ADR-0022's row, which is history;
  - what the repo cannot check before the code exists.

### Claude Code's pass over the landing

- Each status line was replaced by a script that requires exactly one match, then the file was
  linted. No "awaiting his ratification" is left in the threat model, and each landed status quotes
  the words Manuel gave for it.
- The guard of *Anticipated* above, run once after the last landing, printed only status lines
  and the two catalog rows.

## Gates and CI

- `markdownlint-cli2` on each changed Markdown file before each commit, its exit read alone: `0`
  each time.
- No test suite ran: every change is Markdown outside `services/`, `agent/`, `dashboard/` and
  `rules/`, and no document a CI test reads changed (`docs/operations/`, `deploy/docker/README.md`).
  The elevated gate does not apply.
- CI after each push, the REST fallback filtered to `event=push`:

```text
SHA 51a1cda
  markdown-lint → success
Verdict: ALL GREEN
SHA e084f26
  markdown-lint → success
Verdict: ALL GREEN
SHA ebe63b6
  markdown-lint → success
Verdict: ALL GREEN
SHA 85d6016
  markdown-lint → success
Verdict: ALL GREEN
SHA ac9e604
  markdown-lint → success
Verdict: ALL GREEN
```

- This close: CI on its SHA (reported in the turn).

## Debts

- Opened: none. Closed: none.
- Re-routed: #26's second half to ADR-0017's later notification increment. The destinations of
  #26's first half, #83, #88, #89, #98, #102, #119, #136 and #170 now name SPEC-025 or SPEC-026
  (*Anticipated* above).
- Open at close: **56**.

## Waiting on Manuel

- Unchanged since Session 87's close:
  - the Authenticode decision and its purchase;
  - #130's legal readings;
  - the commercial licence and terms;
  - `SECURITY.md`'s contact;
  - the isolated machines (M3);
  - a penetration test;
  - #99, #124 and #125.
- The elevated gate, when F4's server half reaches it (it changes
  `services/ingest/src/routes/heartbeat.ts`).

## How Session 91 resumes

1. Run the start protocol:
   - `main` is this close's commit, with no other branch and no PR;
   - 56 open debts; 22 ADRs / 25 SPECs.
2. Roadmap step 4, **F4's server half** (SPEC-025), contract before code:
   - Write renew_ac_001 to 013 from SPEC-025's criteria and *Test scenarios*. Run them against the
     tree without the implementation and see each fail for the missing behaviour, not for its
     setup.
   - Write ingest's migration `0010_agent_certificates`; enrollment checking the token before it
     signs (with `issue-token`'s hash); every request's revocation and certificate checks; the
     renewal route; the rate limits; the copy signals.
   - Work on a branch, one commit per push, a PR open, CI green on each.
   - The gates: the *Local pre-commit gate* bullet of each path it touches: the TypeScript one
     for `services/ingest/`, and the bundle's, since `deploy-ci` runs on `services/`.
   - Then the self-review against SPEC-025 (the matrix and the fixed code prompt, on Sonnet) and
     Manuel's elevated gate, as a script `C:\tmp\s91-gate1.ps1`.
   - Take Session 89's and Session 90's List B items for the code into its contract review.
3. With the server half, or right after it, **the runbooks' change** that SPEC-025's *Out of
   scope* lists, through a branch and a PR (`deploy-ci` reads `docs/operations/`). SPEC-026's
   runbook lines follow with SPEC-026's code.
4. Then **F3** (SPEC-026: the api's migration and routes, ingest's `0011_dashboard_rights`, the
   detection change, the pages in four phases), then **F4's agent half** after its SPEC-024
   amendment, under the elevated gate.
