# Handoff — Session 42

Session 42 opened on 2026-10-18. Manuel set its order: what his answers to
handoff-41's *What waits on Manuel* unblock (#96, #13); then the sweep in
handoff-41's order (#27, #30, #8, #11, #110); then roadmap §F's SPEC
(criterion 7). He delegated again every decision that waits on him, an ADR
text and the elevated gate excepted.

This record was started early because the documents this session amends
point to it. Later commits of the session complete it.

## Start

- `main` = `origin/main` = `e11b9fa`, tree clean, no stash. Three branches
  open, as handoff-41 names them: `adr/s41-0003-dev-stack` (no PR),
  `fix/s41-debt-96-dev-stack` (PR #24) and `fix/s41-debt-13-anchors`
  (PR #28). CI of `main`'s tip: `markdown-lint → success`, the only
  workflow triggered. No weekly `dependency-audit` run since Session 41's
  close; the next is due on Monday 2026-10-19.
- Catalogs ADR 21 / SPEC 21; Known CI debt zero; `docs/debts.md` open 68,
  which matches its rows.
- `logman query -ets` listed no `CGAgent` session. Docker Desktop was
  running; `C:` had 30.9 GB free.

Manuel's decisions this session, in chat:

- 2026-10-18, opening the session: the order above, and the delegation, as
  in Sessions 38 to 41: "lo que dependa de mí, decídelo tú en beneficio del
  proyecto, para un comprador empresarial, registrando cada decisión como
  delegada. Un ADR sigue necesitando mi "sí" explícito, y el gate elevado lo
  sigo corriendo yo."
- The same message kept a slot for each of handoff-41's six questions, and
  each reached the session holding the template's placeholder, not an
  answer. Claude Code put four of them to him as one question with four
  parts, each with a recommendation, and he chose, on 2026-10-18:
  - ADR-0003's Amendment 2026-10-16: "Sí (Recommended)";
  - an anchor-only edit to an ADR: "Tuyas, Claude Code (Recommended)";
  - the `cyberguard-dev` containers on `0.0.0.0`: "Recréalos tú
    (Recommended)";
  - cargo-audit on the *Approved local toolchain*: "Sí (Recommended)".
- The other two, #80's `401` half and #31's bound against arrival, Claude
  Code first decided under the delegation, as handoff-41 recommended. The
  fresh-context review of that text found that the same delegation had
  left #80 to Manuel in Sessions 40 and 41, and #31 in Session 41 (its
  List A, sixth point; *Decisions*), so they were put to him as one
  question with two parts, each with three options and a recommendation.
  He chose, on 2026-10-19:
  - #80: "Ninguna fila; cierra #80 (Recommended)";
  - #31: "Sin cota atrás; adelante tras medir (Recommended)".

## Progress

### ADR-0003 and #96

- ADR-0003's Amendment 2026-10-16 landed on `main` as `b6bce33`, after
  Manuel's "sí": the commit of the owner-review branch, its subject without
  the marker, and a ratification line in the amendment. CI:
  `markdown-lint → success`. The branch `adr/s41-0003-dev-stack` was
  deleted, local and on origin.
- PR #24 (#96) was rebased onto it without a conflict (`cd9b3c8`). Local
  gate: `node --test infra/dev/check-compose.test.mjs` 7 of 7; the render
  and the check: "compose ok: 5 services, 7 published ports on 127.0.0.1,
  clickhouse/clickhouse-server:24.8.14.39 in the compose and in 2 test
  backends"; `markdownlint` clean. CI on the PR: `markdown-lint`,
  `infra-ci → success`. Landed with `git cherry-pick --ff` behind
  `git diff --quiet cd9b3c8 HEAD`, under the SHA its CI ran on; the push
  run: `markdown-lint`, `infra-ci → success`. GitHub closed PR #24 as
  merged; the branch was deleted.
- Self-review: Session 41's fixed-prompt review of PR #24 found only the
  clash with ADR-0003 §Consequences, which the amendment resolves; the
  rebase changed none of the PR's files. Matrix against debt #96
  (`docs/handoff-session-39.md:729`): NATS and MinIO out of the dev compose
  → `infra/dev/check-compose.test.mjs:84` (either refused, by service name
  or by image) and the render's check, covered; the README, `.env.example`
  and the Taskfile no longer name them, read in the diff.

### #80 and #31, Manuel's decisions

- SPEC-008 and SPEC-017, each an Amendment 2026-10-19 with Manuel's
  decision, the threat model's §3 sentence and R9, roadmap §F and the
  register: `ec38fba`. CI: `markdown-lint → success`. The two fixed-SPEC
  reviews and their dispositions are under *Decisions*.
- apiauth_ac_023, the test the SPEC-008 amendment names: `cc6fa34` (PR #33,
  one commit, landed by fast-forward behind `git diff --quiet cc6fa34 HEAD`).
  Five kinds of `401` leave `audit_log`'s row count as it was, and the
  file's last test counts again once the api and its pool are closed.
  - The criterion holds as built, so the test cannot be seen to fail
    first. Two local mutations stand in for that: a row written on the
    `401` path, awaited, fails the five cases; one not awaited fails the
    last test (6 rows). Both were reverted.
  - Matrix (SPEC-008 Amendment 2026-10-19, apiauth_ac_023):
    - a token that names no entry → `apiauth-ac-023-no-session-no-row.test.ts`
      "a token that names no entry", covered;
    - an entry issued 24 h and one second ago, then gone → "an entry
      issued 24 h…", covered (the entry's absence asserted too);
    - an `issued_at` that is not a time, no `issued_at`, an entry that is
      not a JSON object (text, `null`) → their three tests, covered;
    - "writes no row" → `refusedWithNoRow` and "no row lands after the
      responses either", covered.
  - The fixed code prompt's reviewer: every case covered; "writes no row"
    weak, since a row the api wrote without waiting would land after the
    count (its mutation M2 passed). Fixed in the same commit: the last
    test. No list-2 or list-3 finding.
  - Local gate (`services/api`): typecheck, lint, build, entrypoints, 49
    files, 301 tests. CI on the PR and on `main`: `ts-ci → success`.

### #27 and #8

- #27: `d6790b1`. `ac-001-marquee` asserts SPEC-004 AC-001 (b) as it
  reads, some heartbeat with `sequence_number` 1 and `online`, not the
  first row by `arrived_at`; and AC-001 (a)'s key, the SHA-256 of the
  stored pubkey against the `agent_pubkey_fingerprint` the agent wrote.
  Diagnosis, inferred and not reproduced: ClickHouse stamps `arrived_at`,
  the agent heartbeats every second, and Docker Desktop's VM clock steps
  back by more than a second (#41's 2351 ms); S31 did not record which
  assertion failed. The fixed code prompt's reviewer: (b) covered with
  one residual, the first row with sequence 1 checked for `online` (taken:
  `some(...)`); (a)'s key weak, its length only (taken: the fingerprint);
  the test's comment paired the VM clock with `going_offline`, which a
  Windows run never writes (taken); the token is inserted, not issued by
  the CLI (pre-existing, not taken). Gate 1 ran the marquee elevated:
  passed, 4391 ms.
- #8: `a9e4ef0`. `services/ingest/test/helpers/throwaway.ts` holds the
  throwaway-database pattern of five tests, and its migration provider is
  `migrate.ts`'s own `folderMigrationProvider`, now exported. The
  reviewer: extraction covered; the sweep weak, since the api's
  `apiauth-ac-011` and `read-schema.ts` keep their copies (not taken: a
  helper shared by two packages is the shared package debt #7 leaves to
  Manuel, as the commit says); each test asserts what it did (covered);
  the helper's provider a copy of the source's (taken: the export);
  database names unquoted in SQL (taken: the helper refuses a name that is
  not a lowercase identifier). Local gate (`services/ingest`): 309 passed,
  2 skipped, 4 failed, the four capture marquees with the agent's exit
  code 9.

### #13, the anchor sweep

- `5ddec66`, PR #28's one squashed commit, landed by fast-forward after
  gate 1, behind `git diff --quiet 5ddec66 HEAD`; PR #28 closed and the
  branch deleted. CI on `main`: `markdown-lint`, `rust-ci`, `ts-ci →
  success`.
- The sweep: the inventory of handoff-41 and a blame-based recheck at this
  session's `main`, run by four agents over the ADRs, the SPECs, the threat
  model and the code comments, each citation verified against its target.
  An anchor is corrected when its target still exists and shows what is
  cited; it is left when the target is gone, the sentence is no longer
  true, the passage is a record (Class H-inline), an amendment redirects
  it by name, the source is a third party's, or the citation is pinned to
  a commit.
- The fixed code prompt's reviewer, on the branch before its review
  fixes:
  - taken: ADR-0017:105's block (`index.ts:67-108`, as SPEC-014 maps
    it); SPEC-013's two Sources ranges; SPEC-022's nodemailer release;
    SPEC-008's "first code consumer of Redis" sentence, rewritten as
    handoff-41 said; the threat model's Amendment 2026-10-15 note, now
    false after the sweep; Manuel's answer on ADR anchors, recorded under
    *Decisions*; the elevated gate, gate 1 below;
  - left, each with its reason: ADR-0015:13 and ADR-0016:22, whose
    sentences no longer hold (debt #117); SPEC-014:117, a ratification
    record; the anchors written wrong rather than drifted
    (`session_stub.rs`, SPEC-013:39, roadmap:23), corrected all the same,
    since their targets show what is cited;
  - recorded: the commit message names the path changes, not only the
    numbers.

### #30, the real-ETW tests in CI

- A probe first: PR #31, closed without landing, ran the seven `#[ignore]`d
  tests on `windows-latest` in a step that could not fail the job (job
  117804210704): all seven passed.
- SPEC-017's second Amendment 2026-10-19: `0bfc238`, with debt #115. The
  fixed SPEC prompt's review found three List A points, each fixed before
  the commit landed:
  - four of the seven tests fail through an assertion on the heartbeat,
    not an `expect`;
  - the Amendment 2026-10-13 still read the runner's privilege as
    untested; the new *Effect* says capture_ac_001's privilege assertions
    run developer-local only;
  - CLAUDE.md's SPEC-005 validation introduction said the privilege model
    "has not been validated"; the *Effect* names it.

  From its List B, taken: the job's date in UTC; "PR #31"; the step's 25 s
  and an image update as a cause of red; what a failing test prints, about
  the runner and not a person; the §Risks row; the catalog line as a
  self-amendment, Claude Code's; ADR-0010's *Consequences* line, debt
  #115. Not taken: the header's *Depends on* (the amendment cites ADR-0010
  §Decision part 3 itself).
- The workflow: `acb0654` (PR #34, one commit, rebased onto `cc6fa34`,
  landed by fast-forward behind the guard). `rust-ci`'s Windows job runs
  `cargo test -p cg-agent --no-fail-fast -- --ignored --test-threads=1`
  after `cargo test --all`; on the PR its step took 28 s and printed seven
  `... ok` (job 117835316720). CLAUDE.md, `.github/workflows/README.md`
  and the workflow's comment say so.
  - The fixed code prompt's reviewer: no list A. Taken: CLAUDE.md's gate
    preamble named the install as the only step the local gate does not
    mirror (now two, the Windows job's ignored tests too); "a change to
    the agent" in *Environment facts*. Recorded: `--ignored` selects by
    the attribute, so a test `#[ignore]`d later joins the step; the job's
    log is public, and on a failure it carries the runner's connections
    and redacted logons; SPEC-005:146 and ADR-0008:100 carry the same old
    framing as ADR-0010:97 (debt #115).
  - CI on the PR and on `main`: `markdown-lint`, `rust-ci → success`.

### #11, the ADR catalog

- `194c628`. Each row names every amendment of its ADR, its date, the
  record that made it or came with it, and what it changes; the seven
  "self-amendment" lines leave §Dependencies, which keeps the edges and
  their notes; a sentence under *Catalog* states it.
- Two passes of the fixed SPEC prompt:
  - the first: List A, two inline amendments in no row (ADR-0002's of
    2026-05-22, made by ADR-0007, and ADR-0011 §6's of 2026-05-28), and the
    ADR-0011 → ADR-0009 edge still citing the requirement §6's amendment
    struck. Fixed. From List B, taken: ADR-0004's part (c), ADR-0010's two
    constraints, ADR-0012's every rule class, ADR-0002's "(with ADR-0021)"
    and its firehose clause, the Dependencies sentence;
  - the second: List A, the edge quoted ADR-0009's "no on-disk event
    state" as §Decision part 3's; it is §Consequences' and §Compliance's.
    Fixed. From List B, taken: ADR-0011's two amendments of 2026-05-28
    named apart, with the cache-miss case; "came with it" in the
    convention; "three bullets" for ADR-0015; the commit message's list.
    Not taken: ADR-0011:290 and SPEC-005:72 keep the struck phrase, which
    ADR-0011's own amendment supersedes where they differ (CLAUDE.md, *SPEC
    amendment workflow*); the SPEC catalog keeps its self-amendment lines,
    debt #11 being the ADR catalog's.
- CI: `markdown-lint → success`.

### #116, apiauth_ac_014 against the VM's clock

- `57845d4` (PR #35, one commit, landed by fast-forward behind the guard),
  with debt #116's register row and entry.
- Diagnosis, measured: the case races Postgres's 5 s `statement_timeout`
  (the Docker VM's clock) against the client's 6 s `query_timeout` (the
  host's). A temporary probe of twelve statements: ten cancelled at 4.8 s
  of host time, two at 5.96 s while the VM counted 5.01 s. A lag past the
  1 s margin lets the client's timeout fire first, and its error, "Query
  read timeout", has no code: the failure seen twice this session.
- The test reads the VM's clock right after the host's, before and after
  the statement, and tries again, up to three times, only an attempt that
  the client's read timeout ended while the VM fell more than 0.5 s behind.
  A mutation without `statement_timeout` fails it on the first attempt.
- The fixed code prompt's reviewer: the criterion covered; the defect
  weak at a lag of about 1 s, since the first version compared the VM's
  span with 5000 ms (taken: the lag between the two clocks); the retry
  wider than the defect (taken: the read timeout only); the comment cited
  #116 before it existed (taken: its row in the same commit); the 503
  case's comment (taken). Recorded: a persistent rate lag of 17 % or more
  over 20 s would still fail it; SPEC-008 item 15's "a slow statement ends
  at the server first" does not hold on a lagging VM, where either bound
  answers `503`.

### #110 and #111's chacha20 half

- PR #36, seven commits, each pushed alone with CI green on each, then
  landed one push per commit by fast-forward, each behind
  `git diff --quiet <sha> HEAD` and with its push runs green; PR #36 closed
  and the branch deleted.
  1. `a0a885d`: vitest 4.1.11 in the three workspaces, with vite 7.3.6 in
     the dashboard, one vite in the lock. `poolOptions` becomes
     `fileParallelism: false`, each file in a process of its own, and
     `globalSetup` receives the `TestProject`.
  2. `137c632`: testcontainers 12.1.0 (undici 8.12, dockerode 5). Its
     breaking change, the default wait strategy, changes nothing here:
     each backend's container sets its own.
  3. `27cc224`: the flagged transitive packages, within their ranges.
  4. `fe06426`: chacha20 0.10.2 (#111's half); `cargo audit` keeps one
     allowed warning, rustls-pemfile. The agent's normal build does not
     compile chacha20.
  5. `2a617f9`, after the review: `@vitejs/plugin-react` back to main's
     4.7.0, vite a range (`^7.3.6`), and the dashboard's postcss 8.5.30, so
     that no production package moves; the config's comment.
  6. `f30ad7c`, after the review: the ingest suite's teardown stops any
     ETW session of the agent's that a failed marquee left. A session
     started under the agent's name, unelevated, was stopped by the
     teardown of a one-file run.
  7. `2bb3f0b`, after the review: the dependency runbook and CLAUDE.md's
     Node row (the test tooling needs Node 22.22 or later; CI runs 22.23.3).
- `pnpm audit` over all the dependencies: from 53 advisories (3 critical,
  19 high) to 12 (no critical, 6 high). All are in markdownlint-cli2 but
  one, tailwindcss 3's postcss-selector-parser (moderate, fixed in its next
  major). `pnpm audit --prod --audit-level=high`: no known vulnerabilities.
- `isolate: false` was tried first, as vitest's migration from
  `singleFork`, and failed four api tests (apiauth_ac_013, ac_015), whose
  modules must be fresh per file.
- In Claude Code's terminal vitest 4 detects an agent (`std-env`'s
  `isAgent`) and uses its minimal reporter, which hides a passing test's
  output. In Manuel's terminal and in CI it uses the default reporter, and
  gate 2's evidence lines printed as before.
- The fixed code prompt's reviewer:
  - no list-A block;
  - criteria: the advisories' removal weak, since only a manual audit
    shows it; markdownlint-cli2's half missing by design.
  - Contradicted, and taken (commits 5 to 7):
    - the runbook still named chacha20 among `cargo audit`'s warnings;
    - a production package moved (`next>postcss` 8.5.30 → 8.5.29);
    - vite was pinned to one release;
    - plugin-react's major was not needed;
    - the process per file let a failed marquee's agent end before it
      stopped its session (debt #63's cleanup);
    - the Node floor rose unrecorded;
    - a stale config comment.
  - Recorded, not taken:
    - no CI step audits the development dependencies, so a critical
      advisory could come back unseen (#110's narrowed destination);
    - fast-copy 4.2.0 and @grpc/grpc-js 1.14.6 are recent releases
      (*Claude Code's decisions*);
    - vite, a direct development dependency of the dashboard since
      `a0a885d`, is MIT, and is there so that every vitest resolves one
      vite.

  No second pass ran on commits 5 to 7, which put back what `main` had or
  are covered by gate 2 and the check above.
- Local gates at `2bb3f0b`:
  - `services/api` 49 files, 301 tests;
  - `dashboard` 10 files, 117 tests;
  - `services/ingest` 309 passed, 2 skipped, 4 failed (the capture
    marquees, exit code 9);
  - Rust: fmt, clippy, `cargo test --all` 66 binaries, 126 passed, 7
    ignored;
  - `cargo audit`: one allowed warning.
- Gate 2, on `2bb3f0b`: green (*Gate outputs*).

## Decisions

Manuel's, in chat, each an explicit choice among options put to him:

- **ADR-0003's Amendment 2026-10-16**, ratified on 2026-10-18 ("Sí
  (Recommended)"); it landed as `b6bce33`.
- **An anchor-only edit to an ADR is Claude Code's**, 2026-10-18 ("Tuyas,
  Claude Code (Recommended)"): debt #13's sweep corrects the ADRs' line
  anchors with the rest of the pattern.
- **The dev stack's containers**, to be recreated by Claude Code,
  2026-10-18.
- **cargo-audit on the *Approved local toolchain***, 2026-10-18.
- **#80's `401` half**, 2026-10-19: a request with no session is not an
  authentication event in ADR-0014 §Compliance's sense, and writes no audit
  row (SPEC-008 Amendment 2026-10-19). Options put to him: a row only when
  the api knows the principal; a row for each `401`.
- **#31's bound against arrival**, 2026-10-19: none behind arrival in the
  MVP; one ahead of arrival once a measurement shows what an honest agent's
  times do across a correction of its host's clock (SPEC-017 Amendment
  2026-10-19). Options put to him: a 5-minute bound ahead now; leaving #31
  as it was.

The review that sent #80 and #31 back to him (the fixed SPEC prompt, on the
first draft of the two amendments, a local commit since rebuilt) also
found, in its List A:

- the draft said every authentication event writes its row "a failure
  included", but an attempt the limiter refuses (`429`) or whose body fails
  its schema (`400`) writes none, and a logout writes `auth.logout` only
  for a session that passes the checks;
- it said a request with no session "has no principal to record", but the
  api reads the `user_id` of an entry it ends at its cap before it
  deletes it;
- the SPECs said this record lists the threat model's changes for Manuel's
  audit, and it did not;
- the threat model said #80 closed while the register still held it open;
- SPEC-017 said an element far behind its arrival is accepted, but a POST
  whose times spread over more than 100 days still fails (debt #32).

Each is fixed in the amendments as they landed. From its List B, taken: the
other kinds of `401` get a test (apiauth_ac_023); "lateness horizon" said
of detection, not ingest; a returning agent's backlog is what its buffer
kept; an entry that is not a JSON object is listed; the catalog's two
entries of 2026-10-16 point to the decisions; the option of ordering an
element by its arrival would change what ADR-0006 and ADR-0012 decide.
Recorded: the request log's retention is the deployment's (roadmap §F).

A second pass of the fixed SPEC prompt, on the rebuilt text, found three
List A points, each fixed before the commit landed:

- a password change or a logout that the CSRF check refuses writes an
  `access.denied` row, which the text's "refused before its password or
  code is checked writes no row" left out;
- the first Amendment 2026-10-16, item 1, still gave "no principal to
  record" as its reason; the new amendment's *Effect* now says item 1 does
  not rest on it;
- SPEC-017 named ADR-0006 and ADR-0012 for what partitions the events and
  places them in the incident windows; those are ADR-0003 Rule 2 and
  ADR-0013.

From its List B, taken: #31 was left to Manuel in Session 41 only; a
rebuilt local commit is not cited by its SHA; roadmap §F lists the
request log's retention; a `429` can come with no earlier failure; "when
it fails" for the login and the confirmation; apiauth_ac_023 covers an
entry with no `issued_at` and one that is not a JSON object, and says it
cannot be seen to fail first; "presents no password or code"; "a table
the api only appends to", since no grant enforces it; the measurement's
steps stay inside the envelope's ±5 min window. Recorded: the register's
anchors into this record are checked again at the close.

### Claude Code's decisions

Anticipated, from the handoff, the SPECs and the debt entries:

- **#27: the AC as SPEC-004 states it, not a retry.** A sleep or a retry
  would hide the order the test should not depend on. The diagnosis is
  inferred, not reproduced (`d6790b1`).
- **#8: the helper inside `services/ingest`.** The api's copies stay: a
  helper shared by two packages is the shared package debt #7 leaves to
  Manuel (`a9e4ef0`).
- **#13: what the sweep corrects.** An anchor whose target still shows
  what is cited, wherever it stands; not a record, nor a sentence that no
  longer holds (#117). An anchor-only edit to the threat model is read as
  Manuel's answer on ADR anchors reads it, and listed for his audit
  (`5ddec66`).
- **#30: in CI, failing the job.** The two alternatives: the elevated gate
  alone, or a step that cannot fail. ADR-0010 is not amended; its stale
  line is #115 (`0bfc238`, `acb0654`).
- **#11: every amendment in its row.** The alternative, every amendment in
  §Dependencies, was discarded: the row is what a reader looks at first,
  and §Dependencies records edges (`194c628`).
- **#110: the versions.**
  - vitest 4.1.11, not 5: the smallest major that fixes vitest's
    advisories, released two months ago; 5.0 is six weeks old and a
    further major.
  - testcontainers 12.1.0, not 11.14: 11 keeps dockerode 4 and its uuid
    advisory, and 12's Node 22.22 floor is met in CI and here.
  - A release at least two weeks old where a fixed one was: vite 7.3.6,
    not 7.3.7, and postcss 8.5.30, not 8.5.31.
  - fast-copy 4.2.0 (five days) and @grpc/grpc-js 1.14.6 (eleven days)
    were taken as pnpm resolved them in range. The alternative, an
    override pinning an older fixed release, was discarded: the runbook
    keeps overrides for a parent that pins a vulnerable child.
  - markdownlint-cli2 stays at CI's action engine.

Reactive, because a test, a check or a review surfaced it:

- **`isolate: false` failed four api tests** (apiauth_ac_013, ac_015),
  whose modules must be fresh per file. Each file now runs in its own
  process; the review then found that a failed marquee's agent ends with
  its file's process before it stops its session, and the suite's
  teardown stops it (`f30ad7c`).
- **apiauth_ac_014 failed twice:** debt #116, measured and fixed
  (`57845d4`).
- **A stale `node_modules` link** after a sequence of `pnpm add`: the api
  resolved an intermediate vitest with vite 5. A frozen reinstall fixed it;
  the lock was right, and CI installs from it.
- **vitest 4 hides a passing test's output in Claude Code's terminal**
  (its agent reporter); the probes ran with `--reporter=default`.
- **#110's review:** the production `next>postcss` moved, vite was pinned
  to one release, and plugin-react's major was not needed. Each was put
  back as `main` had it (`2a617f9`).
- **#11's inventory missed the inline amendments;** two review passes
  found them.
- **A reviewer's prompt carried a mistyped base SHA** (#116's). The
  prompt also said the base is the tip's parent, and a message corrected
  the SHA before the reviewer reported.

## The threat model, for Manuel's audit

This session changed it as follows:

- §3's sentence on a request with no session (#80, his decision);
- R9's destination (#31, his decision);
- the record of the system as built, its line on NATS and MinIO, and the dev
  stack's sentence removed (#96, `cd9b3c8`);
- line anchors only (#13): the compose citations of its Amendments
  2026-10-13 and 2026-10-14, and its Amendment 2026-10-15's note on them,
  which now says the sweep corrected them. Claude Code reads an anchor-only
  edit to the threat model as Manuel's answer on the ADRs' anchors reads it:
  the posture it records does not change.

The threat model's changes that handoff-41 listed for his audit (#93, #82
and #31) still wait for him too.

## Local environment operations

- **cargo-audit 0.22.2 installed**, with Manuel's approval of 2026-10-18:
  `CARGO_INCREMENTAL=0 cargo install cargo-audit --locked --version 0.22.2 -j 2`.
  Verified: `cargo audit --version` printed `cargo-audit-audit 0.22.2`, and
  `cargo audit` at the repository root printed the two warnings CI prints
  (rustls-pemfile unmaintained, chacha20 yanked; debt #111) and "2 allowed
  warnings found". CLAUDE.md's *Approved local toolchain* gains its row, and
  the *Local pre-commit gate*'s *Dependencies* step runs it.
- **The dev stack's stores recreated on `127.0.0.1`**, as Manuel chose,
  after #96 landed (`cd9b3c8`):
  `docker compose -f infra/dev/docker-compose.dev.yml --env-file infra/dev/.env up -d --force-recreate --remove-orphans postgres clickhouse redis`.
  Verified: `docker ps` lists `cyberguard-dev-postgres-1`
  (`127.0.0.1:15432`), `-redis-1` (`127.0.0.1:16379`) and `-clickhouse-1`
  (`127.0.0.1:8123`, `127.0.0.1:9000`), each `healthy`; the NATS and MinIO
  containers were removed as orphans. Their volumes
  (`cyberguard-dev_cg_nats_data`, `cyberguard-dev_cg_minio_data`) and the
  other volumes stay. Manuel's `infra/dev/.env`, which is not tracked, still
  holds the NATS and MinIO variables, now unused. Docker Desktop was not
  restarted.

## Gate outputs

Both gates ran by Manuel in an elevated PowerShell, from the one-line
scripts `C:\tmp\s42-gate1.ps1` and `C:\tmp\s42-gate2.ps1`; the lines below
are from their logs (`C:\tmp\s42-gate<N>-*.log`, UTF-16). The dash in the
two capture lines is the log's "—".

Gate 1, on `5ddec66` (#13, with #27 and #8 under it), 2026-10-19 00:52
to 00:57:

```text
02-head     5ddec66
            fix/s41-debt-13-anchors
03-status   status lines: 0
01-elev     No hay entradas en la lista.
05-build    Finished `release` profile [optimized] target(s) in 4.54s
06-cargo    test auth_ac_012_logons_on_the_real_log ... ok
            test capture_ac_006_elevated_shutdown_leaves_no_session ... ok
            test net_ac_008_a_real_connection_is_reported_with_its_probe ... ok
            test net_ac_009_an_accepted_connection_and_the_filter ... ok
            test ac_004_cache_hit_terminate_matches_launch_byte_for_byte ... ok
            test ac_007_parent_pid_only_when_parent_dead_at_child_launch ... ok
            test ac_009_events_lost_under_deliberate_etw_buffer_pressure ... ok
08-vitest   RUN  v2.1.9
            detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-TgHXDc\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
            spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
            Test Files  75 passed (75)
                 Tests  314 passed | 1 skipped (315)
10-session  No se encontró el Conjunto de recopiladores de datos.
```

Gate 2, on `2bb3f0b` (PR #36: #110 and #111's half, with `cc6fa34`,
`acb0654`, `194c628` and `57845d4` under it), 2026-10-19 05:12 to 05:19:

```text
02-head     2bb3f0b
            chore/s42-debt-110-vitest
03-status   status lines: 0
01-elev     No hay entradas en la lista.
05-build    Finished `release` profile [optimized] target(s) in 10.20s
06-cargo    the same seven lines, each "... ok"
08-vitest   RUN  v4.1.11
            detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-gJVFWo\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
            spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
            Test Files  75 passed (75)
                 Tests  314 passed | 1 skipped (315)
10-session  No se encontró el Conjunto de recopiladores de datos.
```

The skipped test is `servercert_ac_008`, which runs only off Windows. In
both, `logman query -ets` after the run lists no `CGAgent` session, and
`detect_ac_001` logged no alert from another rule. The marquees' budgets:
`spec_005_marquee_complete` 40.4 s and 40.3 s, `net_ac_010` 25.4 s and
25.3 s, `auth_ac_013` 25.6 s and 25.3 s, against 45 s.

## Baselines

At the close's `main`:

- `services/api`: 49 files, 301 tests.
- `services/ingest`, unelevated: 75 files, 70 passed, 4 failed, 1 skipped;
  309 tests passed, 4 failed, 2 skipped. The four failures are the capture
  marquees, with the agent's exit code 9, as expected.
- `services/ingest`, elevated (gate 2): 75 files, 314 passed, 1 skipped.
- `dashboard`: 10 files, 117 tests.
- Rust: `cargo test --all` 66 binaries, 126 passed, 7 ignored; the seven
  `--ignored` pass in both gates and, since `acb0654`, in `rust-ci`'s
  Windows job.
- `cargo audit`: one allowed warning (rustls-pemfile, #111).
  `pnpm audit --prod --audit-level=high`: no known vulnerabilities.
  `pnpm audit` over all dependencies: 12 (6 high, 5 moderate, 1 low), in
  markdownlint-cli2 and tailwindcss 3 (#110).

Catalogs: ADR 21, SPEC 21 (amendments only). Known CI debt: none.

## Debts

New:

- **#115 — ADR-0010 still reads the runner's ETW-open privilege as
  unvalidated.** Its *Consequences*
  (`docs/adr/0010-agent-privilege-model-mvp.md:97`) say the CI assumption
  is unvalidated until the SPEC-005 marquee's first run in CI, which does
  not come (its Amendment 2026-05-29). The agent's own real-ETW and
  Security-log tests passed on `windows-latest` (SPEC-017, the second
  Amendment 2026-10-19). Surfaced by the fixed SPEC prompt's review of
  that amendment. Destination: an amendment to ADR-0010, for Manuel's
  "sí".
- **#116 — apiauth_ac_014's Postgres case fails when the Docker VM's clock
  falls behind.** The case races Postgres's 5 s `statement_timeout`, timed
  on the VM's clock, against the client's 6 s `query_timeout`, timed on the
  host's. A probe of twelve 5 s statements measured the VM counting 5.01 s
  while the host counted 5.96 s, twice; with a lag past the 1 s margin the
  client's timeout fires first, and its error, "Query read timeout", has no
  code. It failed so once in S41 (`docs/handoff-session-41.md:880`) and
  twice in S42, once under vitest 2 and once under vitest 4. Destination:
  the test reads the VM's clock around the statement and tries again an
  attempt the client's timeout ended while the VM fell behind (PR #35).
- **#117 — Sentences the code has outgrown, which #13's sweep left.** The
  sweep corrects an anchor whose sentence still holds; the reviewer of PR
  #28 listed sentences that no longer do, their anchors left or corrected
  beside them: ADR-0014:14 ("No human authentication exists anywhere"),
  ADR-0015:13 and :16, ADR-0016:22 and :24, SPEC-007:160, SPEC-008:54,
  SPEC-010:119 and :121, SPEC-011:12, SPEC-014:45. Several sit in an
  ADR's *Context*, which may read as the situation the ADR was decided in.
  Destination: a pass that classifies each one, a record (Class H-inline)
  or a live claim (Class B); the ADRs' live claims go to Manuel as
  amendments, with #115's.

Closed this session, with the commit that closed each:

- #96, `cd9b3c8` after `b6bce33`; #80, its `403` half in S40 and its `401`
  half by `ec38fba` (Manuel's decision) with its test `cc6fa34`;
- #27, `d6790b1`; #8, `a9e4ef0`; #13, `5ddec66`;
- #30, `acb0654` (the contract `0bfc238`); #11, `194c628`;
- #116, opened and closed: `57845d4`.

Narrowed:

- **#31** to the bound ahead of arrival, after the measurement (Manuel's
  decision, SPEC-017 Amendment 2026-10-19, the first).
- **#110** to what `pnpm audit` lists after PR #36:
  - markdownlint-cli2's dependencies, which follow CI's markdownlint
    action;
  - tailwindcss 3's postcss-selector-parser, whose fix is in tailwindcss's
    next major;
  - an audit of the development dependencies in CI, which would keep a
    critical advisory from coming back unseen; at `critical` it passes
    today.
- **#111** to rustls-pemfile: chacha20 is 0.10.2 (`fe06426`).

New this session: #115, #116 and #117, their entries above.

**Open at the close: 63.** Session 41 closed with 68; this session closed
eight of them or of its own (#96, #80, #27, #8, #13, #30, #11 and #116)
and opened three (#115, #116 and #117).

## What waits on Manuel

- **#115 and #117's ADR items, for his "sí".** ADR-0010's *Consequences*
  (`:97`) still read the runner's ETW-open privilege as unvalidated, and
  ADR-0008:100 says the same; and #117 lists the ADR sentences the code
  has outgrown. Claude Code drafts them on an owner-review branch when he
  asks, or with the next ADR change.
- **#31's measurement:** stepping a test host's system clock during a
  capture changes that host, and needs his OK when it is run.
- **The threat model, for his audit.** This session's changes are listed
  in *The threat model, for Manuel's audit*; handoff-41's (#93, #82 and
  #31) wait too.
- **His `infra/dev/.env`** still holds the NATS and MinIO variables, and
  the volumes `cyberguard-dev_cg_nats_data` and
  `cyberguard-dev_cg_minio_data` remain: they are his to remove.
- **Debt #7, a shared package** (the JCS canonicalization the two services
  duplicate); #8 left the api's throwaway-database copies for the same
  package.
- **The elevated gate**, for each change to the capture or the detection
  path.

## State at the close

`main` = `origin/main` = the close commit, the tree clean, no stash, no
branch but `main`; every pull request of the session is closed. CI on the
close SHA: ALL GREEN (*How Session 43 resumes* asks for the check again).

## How Session 43 resumes

1. *At the start* (CLAUDE.md *Session protocol*): this record, `main`,
   CI, catalogs ADR 21 / SPEC 21, Known CI debt none, `docs/debts.md` open
   63.
2. Ask Manuel for what waits on him above that the work needs; the rest
   goes on without it.
3. Roadmap §F's SPEC (criterion 7), the next phase by dependency:
   `SPEC-XXX-infra-docker` (the production compose, the operator's
   configuration and secrets surface, TLS and CA provisioning, published
   images, the dashboard in compose). It decides the deployment-contract
   points Manuel delegated on 2026-10-14 (#12's compose location, forensic
   trust anchoring) for an enterprise buyer; the ADR amendments they need
   wait for his "sí", and the SPEC lands before any code. Its carried
   debts are the list under roadmap §F.
4. The sweep, if §F's SPEC waits on him: #117's classification, then #110's
   remaining half, #111's rustls-pemfile with the next agent change under
   the elevated gate.
