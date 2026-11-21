# Handoff — Session 89

Session 89, 2026-11-21, went on with roadmap step 3's review rounds on the two owner-review
branches. It ran four complete fresh-context rounds over the F3/F4 ADR branch (rounds 10 to 13);
round 14 was stopped unfinished. It also ran three rounds over SPEC-026 (rounds 5 to 7). Round 7
had no List A from any of its three reviewers, so SPEC-026's text is ready for Manuel's yes once
the ADR branch under it is. The ADR branch has not yet had a clean round: its last complete round
found one item, which is fixed. Manuel stopped the session on the hours limit ("para, 95 de
limite de horas no da tiempo a mas"), so no compare was given for ratification and nothing landed
from the branches. No code changed and no tag was pushed; the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `45f3e12`, clean, no stash. Two other branches, as Session 88
  left them: `docs/f3-f4-adr-amendments` at `4cb678a` and `docs/f3-spec-026` at `b61f5b6`
  (stacked), neither with a PR. No PR was open. The tip's CI was ALL GREEN (`markdown-lint` alone,
  docs-only). Counts: 22 ADRs / 23 SPECs, 56 open debts, no Known CI debt. The repo agreed with
  Session 88's handoff; 35 GB were free on `C:`.
- Resulting `main`: this close's commit alone.
- Branches, both pushed with no PR (owner-review branches: a push with no PR runs no workflow).
  Every commit on them is marked NOT YET RATIFIED.
  - `docs/f3-f4-adr-amendments` at `e62cb09`: Session 88's twelve commits on `2491a23`, then the
    fixes of rounds 10 to 13 (`27be7da`, `b3f1dee`, `8843507`, `e62cb09`). Compare:
    `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...docs/f3-f4-adr-amendments`.
  - `docs/f3-spec-026` at `442f096`, stacked on `e62cb09` and restacked after each ADR commit with
    `git rebase --onto`. It holds SPEC-026's seven commits, `b4703c5` to `442f096`; the fixes of
    rounds 5 and 6 are `1448400` and `442f096`. Compare:
    `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/docs/f3-f4-adr-amendments...docs/f3-spec-026`.
- Catalogs on `main`: 22 ADRs / 23 SPECs, unchanged. On the branches, SPEC-025 and SPEC-026 are
  `Proposed`.

## Decisions

### Manuel's

- **2026-11-21, the close** (quoted above): stop without further rounds.

### Anticipated (Claude Code)

- **One workflow per round for both branches**: three reviewers per branch, each with the fixed
  prompt for a SPEC or ADR text in an isolated worktree, and one adversarial verifier per List A
  item. Only verified items counted. This is Session 88's method.
  Discarded: running the two branches in separate workflows, which only doubles the waiting.
- **SPEC-026's clean round kept across the later restacks.** Round 7 ran at `aa5b121`. The two
  later ADR commits changed only ADR-0004 and SPEC-025 lines, and `git range-diff` showed every
  SPEC-026 patch unchanged (`=`) after each restack, so SPEC-026's diff against its base is the
  text that was reviewed. Discarded: another SPEC-026 round after each ADR fix; the security lens
  never runs dry (`feedback-contract-review-convergence`), and the diff it would read is the same.
- **Two List B items fixed with round 10's List A**, both with a security bearing and both cheap:
  - The swap under the new pair resumes no TLS session of the old one. SPEC-025 §Operational §10
    and renew_ac_103 carry it; two reviewers raised it.
  - The ten-year CA's accepted threat names a holder on the path who answers a renewal with a
    certificate of its own. The agent keeps that certificate and the real server then refuses
    it, so the agent is stranded until it is enrolled again.

### Reactive corrections

Each verified List A item was fixed in the commit after its round. Two of them changed behaviour,
not only wording:

- **Round 11 (ADR branch).** A renewal request whose connection closed with no HTTP response
  exited `7`. The code reads such a close as a refusal of the client certificate
  (`agent/cg-agent/src/tls.rs:216-221`, `:259-273`), and the SCM restarts no exit with a code
  (`agent/cg-setup/src/scm.rs:483`). That contradicted ADR-0004 item 4 ("a lost answer to a
  renewal does not strand the agent"). Now such a close is a failed attempt, tried again after an
  hour; a real refusal shows again at the next heartbeat, which exits `7` as today (SPEC-025
  §Operational §9, the by-scope list, renew_ac_105).
- **Round 13 (ADR branch).** A renewal under the pending certificate needs two writes: promoting
  the pending certificate and storing the new one. The text gave them no order, and either order
  broke: a `503`, or the new certificate erased and the agent stranded. Now one statement does
  both, under the same compare-and-set (SPEC-025 §Operational §3, renew_ac_008).

One slip of Claude Code's, caught before any commit: `git rebase --onto` leaves the rebased branch
checked out, so round 13's edit was first made in `docs/f3-spec-026`'s tree. It was moved to the
ADR branch as a patch (`git diff`, `git checkout -- .`, `git apply`) before the commit, and the
branch was checked with `git branch --show-current` before each later commit.

## The reviews (CLAUDE.md compensating control 5)

### Claude Code's pass

- Each fix was applied as an exact string replacement that must match once, then grepped for its
  siblings before the commit: "does not refuse it", "under any other certificate", "nothing
  refuses it", "the copied agent" and "only in the body" are left nowhere.
- Each new citation was opened before it was written: `docs/specs/README.md:15` (the required
  *Test scenarios*); SPEC-002 `:72`, `:88`, `:105`, `:290`; SPEC-003 AC-008 (`:265`); ADR-0003
  Amendment 2026-10-19 item 3 (`:172`).
- SPEC-025's new *Test scenarios* map each scenario to criteria that assert it. SC-F4-001 says the
  host's command issues single-use tokens only, since `issue-token` cannot issue the three-use
  token the scenario needs.
- Curly apostrophes, which one edit had introduced, were replaced with straight ones before the
  commit.

### The ADR branch, rounds 10 to 14

- **Round 10, at `4cb678a`, one item** (two reviewers found none): SPEC-025 had no *Test
  scenarios* section. Fixed in `27be7da`, together with the two List B items above.
- **Round 11, at `27be7da`, three items** (one reviewer found none):
  - the renewal's lost answer (above);
  - ADR-0004 item 8's "does not refuse it" against item 4's refusal of a superseded certificate;
  - the copy's remedy: revoking the agent refuses the original too, so every endpoint that holds
    the identity is enrolled anew (item 8, SPEC-025's Security considerations and its runbooks
    list, R18).

  Fixed in `b3f1dee`.
- **Round 12, at `b3f1dee`, three items** (one reviewer found none):
  - item 4's "any other certificate", which also refused a later pending certificate;
  - SPEC-025's copy risk, "nothing refuses it";
  - SPEC-002's `SAN = org_id` and its signed JWT, which SPEC-025 now amends by scope.

  Fixed in `8843507`.
- **Round 13, at `8843507`.** One item was verified: the order of the writes (above), fixed in
  `e62cb09`. A second was refuted: SPEC-023's `issue-token --org` row follows SPEC-004 FR-015,
  which SPEC-025 amends, and deploy_ac_009's criterion still holds (only its test changes). One
  reviewer found none.
- **Round 14, at `e62cb09`:** stopped unfinished at the close; no result.

### SPEC-026, rounds 5 to 7

- **Round 5, at `b61f5b6`, three items verified, one refuted.**
  - Fixed in `1448400`:
    - Open question 1 stated ADR-0003's retention table instead of its Amendment 2026-10-19
      item 3;
    - the threat-model amendment's "until it lands" left out R3, R7, R13 and §8's assets;
    - the compromised api's list left out the host name each agent sends.
  - Refuted: debt #26's re-route not yet in the register. `docs/debts.md` records a destination at
    the landing, and Session 88's handoff schedules it.
- **Round 6, at `775e00a`, two items; two reviewers found the first.**
  - The token reaches the dashboard's server also in the answer of the route that issues it.
  - An expired invitation's TOTP secret is kept until its withdrawal (by the route or the user's
    disable) or until the user's next invitation or reset.

  Fixed in `442f096`.
- **Round 7, at `aa5b121` (now `442f096`): no List A from any of the three reviewers.**

### List B, not fixed, with the reason

- These become false only when the server half lands, before the runbooks' PR, which SPEC-025
  *Out of scope* places before the release: the runbooks' and data-protection lines
  (`backup-and-restore.md:169`, `data-protection.md:31`, `:77`). One reviewer advised landing that
  PR no later than the agent half; this is for the plan of commits.
- These are implementation: the tests that the token's migration and `--org` change
  (`deploy-ac-009-commands.test.ts:101-111`, `test/helpers/db.ts:83`, `runtime-role.test.ts:88`);
  `tls.rs`'s need to tell a TLS alert from a bare close; the heartbeat update guarded by
  `duplicate_signal IS NULL` under the write-once trigger; the 60-second `notBefore` of
  `cert.ts:79-80`; the migration's order of checks and triggers. They are for the code's own
  contract review.
- A copy flag that a holder of the CA's key sets on any agent and that cannot be cleared, and
  duplicate `heartbeats` rows after a `503`: as Session 88 recorded (disclosed in SPEC-025's
  Risks; events deduplicate by `event_id`).
- `threat-model.md:298` (the per-agent rate limits "not built"): that line is about the limits on
  ingest floods (`:68`), which are still not built.
- The catalogs' dependency lines name the load-bearing edges only, as for SPEC-025.
- SPEC-026's list B (QR encoder, `revision`'s SQL, order of refusals, cross-service tests): as
  Session 88 recorded.

## Gates and CI

- `markdownlint-cli2` on each changed Markdown file before each commit, its exit read alone: `0`
  each time.
- The branches: a push with no PR open runs no workflow.
- This close: CI on its SHA (reported in the turn).

## Debts

- Opened: none. Closed: none. Re-routed: none.
- Open at close: **56**.

## Waiting on Manuel

- The ratification of the ADR branch, after a round with no List A, read on the compare, with
  three separate yeses:
  - the ADR amendments (ADR-0004, ADR-0022 and ADR-0014) and the threat model's Amendment
    2026-11-20 (SPEC-025);
  - the ten-year CA, whose accepted threat now names the stranding by a holder on the path;
  - the api's new rights and what a compromised api can then do.

  With them come SPEC-025's departure from ADR-0003's rate-limit row (the counters in process
  memory, `ADR-0003:31`), which appears in SPEC-025 §Operational §5 alone, and the residuals: the
  copy flag that cannot be cleared; a restore before a renewal; a server refusing a certificate
  without an alert.
- On the SPEC-026 branch, now clean: his yes on the threat model's Amendment 2026-11-20 (SPEC-026)
  and on the breaking change of `GET` and `POST /v1/users`, declared in SPEC-026 §Data contracts
  §3.
- Unchanged since Session 87's close:
  - the Authenticode decision and its purchase;
  - #130's legal readings;
  - the commercial licence and terms;
  - `SECURITY.md`'s contact;
  - the isolated machines (M3);
  - a penetration test;
  - #99, #124 and #125.

## How Session 90 resumes

1. Run the start protocol:
   - `main` is this close's commit;
   - `docs/f3-f4-adr-amendments` is at `e62cb09` and `docs/f3-spec-026` at `442f096`, stacked on
     it, neither with a PR;
   - 56 open debts; 22 ADRs / 23 SPECs.
2. Run a fresh-context round over the ADR branch at `e62cb09`: the fixed prompt for a SPEC or ADR
   text, over its eight documents. Commit its fixes with NOT YET RATIFIED and restack SPEC-026
   after each, until a round finds no List A. Rounds 10 to 13 found one, three, three and one.
   After each restack, check with `git range-diff` that SPEC-026's patches are unchanged. Run
   `git branch --show-current` before each commit, since `git rebase --onto` leaves the rebased
   branch checked out. Then give Manuel the compare and ask the three yeses above, apart, naming
   the ADR-0003 departure and the residuals.
3. With them, or right after, ask his yes on SPEC-026's threat-model amendment and on the breaking
   `/v1/users` change. Its last round was clean; if an ADR fix touches a line SPEC-026 cites, run
   one more SPEC-026 round first.
4. After his yeses:
   - land the ADR branch on `main` in order (ADRs, threat model, SPEC-025), with the markers
     stripped and CI green on each push;
   - then SPEC-026 `Accepted`, with its catalog row and its debts' destinations (#26's second half
     to ADR-0017's later increment; #98, #83, #102 and #136 closing with its code);
   - then the runbooks' change, through a branch and a PR;
   - then F4's server half (roadmap step 4), contract before code.
