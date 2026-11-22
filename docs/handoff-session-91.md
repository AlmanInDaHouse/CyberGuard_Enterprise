# Handoff — Session 91

Session 91, 2026-11-22, built roadmap step 4's first part: **F4's server half** (SPEC-025). The
acceptance tests renew_ac_001 to 013 came first, and each was seen failing against the tree
without the implementation. The implementation then landed on `main` through PR #154, four
commits, each green in CI. With it landed SPEC-025's Amendment 2026-11-22, which the code review
surfaced. Manuel's second elevated gate was green on the tree that landed. The runbooks' change
that SPEC-025's *Out of scope* lists did not fit with quality; it is Session 92's first task. No
tag was pushed; the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `f20be88` (Session 90's close), clean, no stash, no other
  branch, no open PR. The tip's CI was ALL GREEN (`markdown-lint`). Counts: 22 ADRs / 25 SPECs, 56
  open debts, no Known CI debt. The repo agreed with Session 90's handoff. 25 GB free on `C:`.
  Docker Desktop was down and was started (an allowed local operation).
- Landed on `main`, one commit per push, in this order:
  - `f6c363b`: SPEC-025 Amendment 2026-11-22 and its catalog row (docs, straight to `main`);
  - `9c67d8a`: the server half (migration `0010_agent_certificates`, enrollment, `issue-token`,
    the request checks, the renewal route, the rate limits, the copy signals, renew_ac_001 to
    013);
  - `1cfdad0`: the first review round's fixes;
  - `62b561e`: the enrollment limiter's sweep at most once a minute;
  - `11c3832`: the marquee harness awaiting its agent's ETW session's end (#161).
- The four code commits reached `main` by fast-forward to the SHAs PR #154's CI ran: the branch
  was rebased onto `f6c363b` before them, so each landed tree is the tree CI and, for `11c3832`,
  the elevated gate ran.
- Resulting `main`: those five commits and this close's. Branches: `main` alone (PR #154 closed,
  `feat/f4-server-half` deleted, local and on origin; the gate's worktree removed).
- Catalogs: 22 ADRs / 25 SPECs (SPEC-025's row gains its amendment).

## What landed

- **Migration `0010_agent_certificates`** (SPEC-025 §Data contracts §1). In `enrollment_tokens`:
  - `token_id` and `token_sha256` are filled for each existing row, the plain `token` dropped;
  - `max_uses`, `uses` and `withdrawn_at` are added;
  - `consumed_by_agent_id` is copied to `agents.enrolled_by_token_id`, then dropped;
  - each existing row's expiry is capped before the 15-minute and 7-day `CHECK`s.

  The `agents` columns of §1 are added, with the three triggers (functions with `search_path`
  pinned), and `cg_ingest`'s `INSERT` and `UPDATE` on `agents` are narrowed to §1's columns.
  `down` throws: the plain tokens cannot be recovered.
- **Enrollment** (`services/ingest/src/routes/enroll.ts`). The token's row is found by its
  SHA-256 and locked `FOR UPDATE`, then checked in §1's order before the CA signs. The agent takes
  the token's org, its enrolling token and its host name, platform and version, cut to 255 code
  points. The token's use is counted. Ten failures from one address in 15 minutes refuse it for 15
  minutes, before the request is read. `issue-token` stores the hash under an identifier, lets the
  database compute the expiry, prints the identifier and expiry to standard error, and refuses any
  `--org` but `default` with exit 2.
- **Every request of an agent** (`services/ingest/src/agent-request.ts`, shared by the heartbeat
  and the renewal), in §2's order and all before the nonce is spent:
  - the `CN`;
  - the row, read once;
  - the revocation (`403 agent_revoked`);
  - the presented certificate's DER against `cert_pem` and `pending_cert_pem` (`403
    agent_superseded` and the copy signal).

  Then the timestamp, the nonce and the signature under the key the certificate gave. The
  heartbeat (`services/ingest/src/routes/heartbeat.ts`) writes the row's org on every
  `cges_events` row. It updates `last_seen`, the agent block's fields and the process columns in
  one statement. Under the pending certificate it promotes it by a compare-and-set, and answers
  `503` when that is not recorded. `interleaved_process` is raised from the process starts.
- **The renewal** (`services/ingest/src/routes/renew.ts`, `POST /v1/agents/renew`):
  - its checks: both signatures over the same canonical bytes, the 45-day window, the CA's end,
    one certificate per agent per 10 minutes;
  - the new certificate is stored as `pending_cert_pem`, by a compare-and-set, or promotes the
    presented pending one in the same statement;
  - `503` and no certificate when it cannot store it.
- **Certificates** (`services/ingest/src/cert.ts`): `notBefore` a minute back in whole seconds,
  `notAfter` the earlier of 90 days after it and the CA's end; a CA ended by then signs nothing.
- **The rate limits** (`services/ingest/src/rate-limit.ts`), in the process's memory: 100 000
  tracked addresses at most, the expired ones swept at most once a minute.

## Decisions

### Manuel's

None this session beyond running the two elevated gates.

### Anticipated (Claude Code)

- **One implementation commit, not two split at enrollment and at the request path.** The
  enrollment route uses the shared module's text cut, so a first commit would carry dead code or a
  duplicate helper. Discarded: the split, which also cost a CI round for no bisectability the
  reviews needed.
- **The renewal route and the heartbeat share `authenticateAgentRequest`**
  (`services/ingest/src/agent-request.ts`), so that §2's order is one code path. Discarded: each
  route with its own copy of the checks.
- **Rate limiters per app instance** (one enrollment app and one mTLS app per process), held in
  the routes' closures. Discarded: on `Services`, which test-built partial services objects lack.
- **The renewal's slot taken at signing** (SPEC-025 Amendment 2026-11-22, point 3). Discarded:
  after a successful store, which lets an attacker who makes the store fail ask the CA to sign
  without bound.
- **The copy signal in a second, guarded statement** (`duplicate_signal IS NULL`), so that a
  concurrent signal never makes the write-once trigger fail an accepted request. Discarded: in the
  heartbeat's one statement, where a race would turn a `200` into a failure.
- **Tests through a harness of their own** (`services/ingest/test/helpers/renew.ts`): both
  listeners as `startIngest` builds them, with their log lines captured. Discarded: a product seam
  for the apps' log stream.
- **`issueClientCert` spied by `vi.mock`** (renew_ac_002). Discarded: a signing hook in the
  product's `Services`.
- **The landing by fast-forward to the PR's SHAs** after a rebase onto the amendment, one commit
  per push. Discarded: cherry-picks, which give new SHAs over the same trees. The guard is that
  `origin/main` was the branch's base before the first push.
- **The runbooks' change moved to Session 92.** The server half took the session, with two
  elevated gates. The runbooks need choices that should not be rushed: a revocation by the owner
  role before SPEC-026's route exists, a restore's effect, a copy's remedy, the token's wording and
  the personal data of `data-protection.md`. Discarded: drafting them at the end of the session.

### Reactive corrections

- **`deploy-ac-009-commands.test.ts`'s `withPending` fixture**: it applied every migration and
  removed the last three from the ledger, which Kysely refused once 0010 was applied ("corrupted
  migrations"). It now applies up to `0009_runtime_grants`, so the run has 0007 to 0010 pending.
- **`deploy-ac-009-commands.test.ts`'s `issue-token` test**: `--org deploy-ac-009` and the
  `token` lookup follow SPEC-025 (no `--org`, the hash). `runtime-role.test.ts`'s statements test
  became a real heartbeat, renewal, promotion and superseded heartbeat as `cg_ingest`.
- **The first review round** (below) found three defects, fixed in `1cfdad0`:
  - a malformed enrollment from a limited address answered `400`;
  - the limiter's maps had no bound;
  - an ended CA gave a certificate that ends before it starts.
- **The amendment's review** found the limiter's full sweep on each failure above 10 000
  addresses, fixed in `62b561e`.
- **deploy-ci on `9c67d8a`** failed once in `upgrade.test.mjs` (#127's test): `Bind for
  127.0.0.1:40817 failed: port is already allocated`, run 44981409285. Diagnosed as
  `freePort()`'s race (`deploy/docker/test/lib.mjs:102`: a port freed, then bound by Compose
  seconds later). The tree differed from `c881373`, green in deploy-ci, by one Markdown file no
  test reads. The job was re-run and was green; recorded as #172.
- **Elevated gate 1 failed `ac-001-marquee` in its teardown** (#161 recurring): `logman query`
  answered `0x80071068`, the session being torn down. Fixed in `11c3832` as #161's destination
  says; gate 2 was green.

## The reviews (CLAUDE.md compensating control 5)

### Claude Code's matrix (criterion → test → verdict)

| Criterion | Test | Verdict |
|---|---|---|
| renew_ac_001 | `services/ingest/test/renew-ac-001-token-at-rest.test.ts` (migration over plain tokens; `issue-token`'s hash, its stderr identifier, no plain form in any column; `--org other` exit 2, nothing written) | covered |
| renew_ac_002 | `renew-ac-002-checked-before-signing.test.ts` (each refusal with a spy on `issueClientCert` at 0 and no agent row; two concurrent enrollments, one `200`, one `409`, one signature) | covered |
| renew_ac_003 | `renew-ac-003-several-uses.test.ts` (three uses, each naming the token; fourth `409`; withdrawn `401`; the `CHECK`s at their bounds) | covered |
| renew_ac_004 | `renew-ac-004-org.test.ts` (another org set on the row is on the next `cges_events` rows) | covered |
| renew_ac_005 | `renew-ac-005-write-once.test.ts` (revocation at the transaction's time, then frozen; reason required; signal frozen; withdrawal frozen and dated; terms fixed; uses only grow) | covered |
| renew_ac_006 | `renew-ac-006-revoked.test.ts` (heartbeat and renewal `403`, no `nonce:` key, no heartbeat row, no event, no pending certificate) | covered |
| renew_ac_007 | `renew-ac-007-pending-certificate.test.ts` (promotion and its log with both serials; old certificate superseded and flagged; CA-signed unrecorded certificate; pending key not verifying `401`; restored row) | covered |
| renew_ac_008 | `renew-ac-008-renewal.test.ts` (45 days `200` with `CN`, key, chain and 90 days; renewal under a due pending certificate; heartbeat write failure `503`; renewal store failure `503`; 46 days `409`; `401`s; replay `409`; stale `422`; second issuance `429`) | covered |
| renew_ac_009 | `renew-ac-009-ca-end.test.ts` (CAs 60 and 30 days from their end and one ended; the `issued_at` a year ahead) | covered |
| renew_ac_010 | `renew-ac-010-enrollment-limit.test.ts`, `rate-limit.test.ts` (ten failures then `429`, malformed included; the token's use kept; another address enrolls; the bound and the sweep) | covered |
| renew_ac_011 | `renew-ac-011-what-the-agent-sends.test.ts` (enrollment's fields; heartbeat's; 300 to 255 code points, outside the BMP too) | covered |
| renew_ac_012 | `renew-ac-012-interleaved-process.test.ts` (A, B, A once, then no change and no second line; A, B, B; A, A + 60 s) | covered |
| renew_ac_013 | `services/api/test/deploy-ac-004-005-postgres.test.ts` (as `cg_ingest`: `last_seen` allowed, revocation refused, an insert naming `revoked_at`, `revocation_reason` or `duplicate_signal` refused, the signal's trigger binding the role); `services/ingest/test/runtime-role.test.ts` (the routes as `cg_ingest`) | covered |

### Red before green

- Against the tree without the implementation, all 43 tests failed:
  - renew_ac_002's spy counted the CA's signature for an unknown token;
  - `issue-token` accepted `--org other`;
  - the rest failed on the columns migration 0010 adds.
- With the migration alone, the migration's criteria passed (renew_ac_001's migration, 003's
  `CHECK`s, 005, 009's trigger), and the routes failed (`500`).
- With the new enrollment and the old heartbeat, these failed for the behaviour:
  - renew_ac_004 (`'default'` where the row's org was set);
  - renew_ac_006 (a revoked agent's heartbeat `200`);
  - renew_ac_007 (a forged certificate `401` where `403` is due);
  - renew_ac_011 and renew_ac_012;
  - the renewal's tests, with `404`.
- renew_ac_013 failed with the migration's grants removed: "UPDATE agents SET revoked_at … was
  allowed".
- The review fixes' tests failed against the code before them: `400` for `429`, an ended CA
  enrolling, the limiter with no size, and #161's two new tests. The real probe session's test
  failed with the old helper on this host too.

### The fresh-context reviews (Sonnet 5.5, isolated worktrees)

- **Round 1** (`f20be88..c881373`):
  - three reviewers with the fixed code prompt, and a fourth that checked Session 89's and
    Session 90's List B items for the code;
  - one extractor, and one adversarial verifier per item: 26 agents, 2.33M subagent tokens;
  - all 13 criteria covered by all three reviewers, none missing or contradicted; 21 items
    verified.
  - Holding and fixed in `1cfdad0`: #2 (`400` for a limited address's malformed request), #4
    (no bound), #6 (an ended CA), #11 (the renewal's own `503` untested).
  - Added as cheap coverage: #13 (the promotion's log on the renewal path), #10 (uses only grow),
    #9 (the trigger binding `cg_ingest`).
  - Not blocking:
    - #1, the slot at signing, stated by the amendment;
    - #3 and #7, already in SPEC-025 §Risks;
    - #5, a `revoked_at IS NULL` guard on the heartbeat's writes, which the contract does not ask
      for;
    - #8, `token_id` not in the fixed terms, which would narrow the contract;
    - #12 and #14 to #20, no defect;
    - #21, `data-protection.md:77`, which goes to the runbooks' PR.
- **Session 89's and Session 90's List B for the code**, as the checker read them:
  - handled: the `409` pair told apart by `error`; the triggers' `search_path` (none is `SECURITY
    DEFINER`); the database computing `expires_at`; the tests the migration and `--org` change;
    the guarded signal; the `notBefore`; the migration's order;
  - recorded above: `token_id` (#8) and the real-role tests (#9).
- **Round 2** (`f20be88..5a6f412`, after the fixes): two reviewers with the fixed prompt, 16
  agents, 1.35M subagent tokens. All 13 covered, no contradiction. Two items holding, both minor
  and both disclosure: the bound and the `500`. They became SPEC-025 Amendment 2026-11-22.
- **The amendment's text review** (`8ead854`, the fixed prompt for a text, one Sonnet reviewer):
  no List A. Fixed from List B:
  - point 1's sentence, made exact;
  - point 2's heading, made its condition;
  - the handoff's path, which did not exist yet;
  - the sweep's cost, in code (`62b561e`).

  Not changed: the original §1 and §Risks, which are read "with" the amendment, and #6, which does
  not apply (each test has its own agent). The prompt carried one sentence beyond the fixed text,
  naming the branch SHA where the code the amendment describes lives.
- Not re-reviewed: `62b561e` and `11c3832`, a limiter sweep and a test helper, each with its own
  unit tests seen red, after the second round.

## Gates and CI

- Local, on the tip, the *Local pre-commit gate* for each path touched:
  - `services/ingest`: typecheck `0`, lint `0`, build `0`, `entrypoints ok`; `pnpm test` "Test
    Files 4 failed | 94 passed | 1 skipped (99)" / "Tests 4 failed | 553 passed | 3 skipped
    (560)", the four capture marquees, the expected unelevated result;
  - `services/api`: typecheck `0`, lint `0`, build `0`, entrypoints ok, "Test Files 56 passed
    (56)" / "Tests 391 passed (391)" at `c881373`; deploy_ac_004 "Tests 22 passed (22)" after the
    fixes;
  - deploy-ci's bullet: `licences admitted: 431 packages`; 64 tests, 60 pass, 0 fail, 4 skipped.
- `markdownlint-cli2` on each changed Markdown file, its exit read alone: `0`.
- **Elevated gate 1** (`C:\tmp\s91-gate1.ps1`, `62b561e`, TREE `70d9203…`): `--ignored` 8 ok;
  vitest "Test Files 1 failed | 98 passed (99)" / "Tests 1 failed | 554 passed | 2 skipped
  (557)", `ac-001-marquee`: "logman query CGAgent-KernelProcess -ets failed: exit status
  0x80071068". Red.
- **Elevated gate 2** (`C:\tmp\s91-gate2.ps1`, `11c3832bab2fc9470255ff0be41c68e8c8d10b92`, TREE
  `8ae8128fec7482f5a772537ffa2a361fc41bc355`, worktree `C:\Users\manul\cg-s91-gate`): `net
  session exit: 0`.
  - cargo `--ignored`: 8 tests ok, 0 failed.
  - vitest: "Test Files 99 passed (99)" / "Tests 558 passed | 2 skipped (560)", `ac-001-marquee`
    passing.
  - `detect_ac_001 drained 25 rows in 1 cycles; capture checked at 35.269 s, drained at 39.082
    s`; `detect_marquee_elapsed_seconds` 39.093.
  - `detect_ac_001 captured image_file_name — probe:
    C:\Users\manul\AppData\Local\Temp\cg-detect-probe-tPeAmh\winword.exe; child:
    C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`; `spec_005 captured
    image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe`.
  - No `CGAgent-KernelProcess` session left (`logman exit: -2144337918`).
  - The release binary (14:15) was built by gate 1 from the same agent source: the branch
    changes nothing under `agent/`.
- CI, each push, the REST fallback:

```text
SHA c881373
  ts-ci → success
  deploy-ci → success
Verdict: ALL GREEN
SHA 5a6f412
  ts-ci → success
  deploy-ci → success
Verdict: ALL GREEN
SHA f6c363b
  markdown-lint → success
Verdict: ALL GREEN
SHA 9c67d8a
  ts-ci → success
  deploy-ci → success (attempt 2; attempt 1 failure, run 44981409285, #172)
Verdict: ALL GREEN
SHA 1cfdad0
  ts-ci → success
  deploy-ci → success
Verdict: ALL GREEN
SHA 62b561e
  ts-ci → success
  deploy-ci → success
Verdict: ALL GREEN
SHA 11c3832
  ts-ci → success
  deploy-ci → success
Verdict: ALL GREEN
```

- The pushes to `main` of `9c67d8a`, `1cfdad0`, `62b561e` and `11c3832`, and this close: CI on
  each (`event=push`), reported in the turn.

## Test baselines

- `services/ingest`, unelevated on this host: 560 tests, 553 passed, 4 failed (the four capture
  marquees), 3 skipped. Elevated (gate 2): 558 passed, 2 skipped.
- `services/api`: 391 passed (at `c881373`).

## Debts

- Closed:
  - **#89** (enrollment hardening): the token stored as its hash, its scope and org checked
    before the certificate is signed, and the enrollment's rate limit, by `9c67d8a` and
    `1cfdad0`. The TLS half landed earlier in `63ea972`.
  - **#161** (`ac-001-marquee`'s harness): by `11c3832`, as its destination asked, when it
    recurred in gate 1.
- Opened: **#172**, deploy-ci's `freePort()` race (`deploy/docker/test/lib.mjs:102`). A port is
  freed, then bound by Compose seconds later, and another process can take it between. It failed
  `upgrade.test.mjs` once (run 44981409285). Destination: a port the stack binds itself (`0`, read
  back from Compose) or a retry of `compose up` on that error, when the bundle's tests next change
  or it recurs.
- Destinations changed:
  - #88: its server half done (`9c67d8a` to `11c3832`), the agent half after F3;
  - #119: the server's drift closed with the server half, the agent's with the agent half;
  - #170: the server's detection done (§Operational §6's two signals), the reading on the isolated
    machines remains.
- Open at close: **55**.

## Waiting on Manuel

- Unchanged since Session 87's close:
  - the Authenticode decision and its purchase;
  - #130's legal readings;
  - the commercial licence and terms;
  - `SECURITY.md`'s contact;
  - the isolated machines (M3);
  - a penetration test;
  - #99, #124 and #125.
- Nothing new from this session.

## How Session 92 resumes

1. Run the start protocol:
   - `main` is this close's commit, with no other branch and no PR;
   - 55 open debts; 22 ADRs / 25 SPECs.
2. **The runbooks' change** that SPEC-025's *Out of scope* lists, through a branch and a PR
   (`deploy-ci` reads `docs/operations/`):
   - revoking an agent and what undoing one means (nothing undoes it: the endpoint is enrolled
     anew), with the owner role until SPEC-026's route;
   - the CA's compromise;
   - a restore's effect on renewed agents (SPEC-025 §Operational §7);
   - a copy's remedy;
   - the token's wording in `docs/operations/install.md`;
   - `docs/operations/data-protection.md`'s personal data: the host name, platform and version on
     the agent's row, the token as its hash, `:77`'s "the agent that used it".

   A step that asks the operator to run SQL as the owner role is a deployment-contract question
   for Manuel. The rule of `feedback-runbook-no-pasted-names` applies: never a pasted name the
   attacker chose. Run the *bundle and runbooks* bullet.
3. Then **F3** (SPEC-026: the api's migration and routes, ingest's `0011_dashboard_rights`, the
   detection change, the pages in four phases), then **F4's agent half** after its SPEC-024
   amendment, under the elevated gate.
