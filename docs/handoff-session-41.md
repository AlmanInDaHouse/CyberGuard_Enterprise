# Handoff — Session 41

Session 41 opened on 2026-10-16. Manuel set its order: PR #23 (debt #93)
first, with its review and elevated gate 3; then the sweep in handoff-40's
order; then roadmap §F's SPEC when the sweep allowed. He delegated again
every decision that waits on him, an ADR text and the elevated gate
excepted.

This record was started early because the register's rows for the debts
this session opens point to it. Later commits of the session complete it.

## Start

- `main` = `origin/main` = `328416b`, tree clean, no stash. One branch and
  one PR open, as handoff-40 names them: `fix/s40-debt-93-dependencies`,
  PR #23, at `4fbce0c`. CI of `main`'s tip: `markdown-lint → success`, the
  only workflow triggered. CI of the PR's tip: `rust-ci`, `ts-ci`,
  `markdown-lint` and `dependency-audit → success`.
- Catalogs ADR 21 / SPEC 21; Known CI debt zero; `docs/debts.md` open 69.
- `logman query -ets` listed no `CGAgent` session. Docker Desktop was
  stopped; Claude Code started it, an operation CLAUDE.md allows. `C:` had
  46 GB free.

Manuel's decisions this session, in chat:

- 2026-10-16, opening the session, the order: PR #23 first, its review with
  the fixed prompt and its fixes, then elevated gate 3 (`C:\tmp\s40-gate3.ps1`),
  which he runs, then the landing commit by commit, #93 closed and
  `dependency-audit` recorded in CLAUDE.md's *Environment facts*; then the
  sweep (#96, #82, #31 with the elevated gate, #13 with #84, then #30, #39,
  #40, #27, #8, #9, #11 and #110); then roadmap §F's SPEC (criterion 7).
- The same message, the delegation, as in Sessions 38 to 40: "lo que
  dependa de mí, decídelo tú en beneficio del proyecto, para un comprador
  empresarial, registrando cada decisión como delegada. Un ADR sigue
  necesitando mi "sí" explícito". The rest of that line reached the session
  cut off. The slot the prompt kept for his reading of ADR-0014:98 (debt
  #80's `401` half) held the template's placeholder, not an answer, so that
  half stays open, waiting for him.
- 2026-10-16, after gate 3: "termino lee logs".

## Progress

### #93: PR #23, its review

One pass of the fixed code prompt on `93127fc..4fbce0c`. Contract: debt #93
(`docs/handoff-session-39.md:714-716`) and the audit statement
(`docs/handoff-session-40.md:957-1018`).

Claude Code's own pass came first:

| Claim of the contract | Evidence | Verdict |
|---|---|---|
| No `pnpm audit` step in CI | `.github/workflows/dependency-audit.yml:45-64`, job `pnpm-audit`; green at the tip | covered |
| No `cargo audit` step in CI | `dependency-audit.yml:66-82`, job `cargo-audit`; red at `c91de2d`, green at `4fbce0c` | covered |
| The dependency runbook does not exist | nothing in the diff | missing |
| `pnpm audit --prod` reports none | a local run at the tip, every severity: "No known vulnerabilities found" | covered |
| nodemailer 10.1.1 keeps the three fields the send reads | `services/ingest/test/soar-ac-006-bounds.test.ts:267` (the version pin), `:276`, `:297`, `:327`, `:343`, `:359` (TLS), `:401` (HELO), `:413` (mechanisms) | covered |
| The crates move within their ranges | `Cargo.toml` unchanged; `Cargo.lock` also gains `rand_pcg` and reselects `windows-sys` | covered; more moved than stated |
| "Development dependencies are not audited" | true of pnpm only: `cargo audit` reads the whole lock | weak (the text) |

Then the reviewer, with the fixed prompt. Its list 1: C1 and C2 (the two audit steps) covered; C3 (the runbook) missing; C4 (`pnpm audit --prod` reports none) covered for today, weak as a standing gate for moderate advisories; C5 (the exact pin) covered; C6 (the three fields) weak; C7 (the crates within their ranges) covered, with more moved than stated; C8 (the policy) covered for pnpm, contradicted for cargo. Its list 2: the cargo job's policy, pnpm's moderate advisories, the landing order, the threat model's two sentences, the gate script's header. Its list 3: what moved beyond the contract, and the security-relevant changes (rustls, rustls-webpki).

What was done with each point, Claude Code's and the reviewer's:

- **The runbook** (missing). Written: `docs/security/dependency-review.md`,
  in the docs commit that closes #93.
- **nodemailer** (weak). 10.1.1 reads the AUTH line's mechanisms as whole
  tokens, where 8.0.10 matched a prefix, and two comments still described
  the prefix match. Fixed in the nodemailer commit. The `PLAIN-CLIENTTOKEN`
  case stays, as a guard against a release that reads prefixes again; the
  `XOAUTH2` case still reaches the send's own reading of the AUTH line.
- **The cargo job** (contradicted). It fails on any vulnerability advisory,
  whatever its severity, across the whole `Cargo.lock`, while the workflow's
  comment and its README row said "high or critical" and "development
  dependencies are not audited". Fixed in the text; the job stays as strict
  as it is.
- **pnpm's moderate advisories** are counted in the summary line, not
  listed. Fixed in the text.
- **The landing order.** `c91de2d` was red at its own SHA, and cherry-picked
  in order it would have turned `main` red. Fixed: the agent's crates now
  come before the workflow.
- **The threat model's `:311` and `:327`.** Fixed in the docs commit that
  closes #93 (below).
- **The gate script's header** named h2 and quinn-proto, which the agent
  does not compile. Fixed: rustls and rustls-webpki, and the `windows-sys`
  reselection.
- **Recorded, from list 3:**
  - rustls-webpki 0.103.15, certificate path validation in the agent, and
    rustls 0.23.46's stricter handshake;
  - `rand_pcg` 0.10.2, a new crate in the lock, never compiled;
  - `windows-sys` reselected among the versions the lock held; socket2, the
    anstyle crates and nu-ansi-term are in the agent's Windows build;
  - the overrides beyond postcss and browserslist (nanoid, source-map-js,
    baseline-browser-mapping), each needed by the base audit, and the
    `browserslist` override being global;
  - fastify's fast-json-stringify 7 and set-cookie-parser 3, and next's
    sharp 0.35.5, whose install script `onlyBuiltDependencies` blocks, as
    before.
- **Recorded, minor:** `@types/nodemailer` 8 against nodemailer 10.1.1, which
  ships its own types the send does not use; SPEC-022:398 names nodemailer
  8.0.10, a citation for #13's sweep.
- **cargo audit's three warnings**, which pass:
  - anyhow 1.0.102, unsound (RUSTSEC-2026-0190, `Error::downcast_mut`,
    patched in 1.0.103; the agent does not call it): moved to 1.0.104 in the
    agent commit, before the gate;
  - rustls-pemfile 2.2.0, unmaintained (RUSTSEC-2025-0134), a direct
    dependency the agent reads its certificates with
    (`agent/cg-agent/src/tls.rs`), and chacha20 0.10.0, yanked, in the
    development dependencies: debt #111.

The reviewer ran no `git checkout`; the branch and the status were checked
after it.

### #93: the rebuild and the landing

The branch was rebuilt from `aab07dd`: the nodemailer commit with its
comments fixed, the overrides, the agent's crates with anyhow, then the
workflow with its text fixed. It was rebased onto `328416b` (docs only) and
pushed one commit per push, the first with `--force-with-lease`:

```text
f876753  next 15.5.28                        ts-ci -> success
fcbf6ce  fastify 5.12.7, fast-uri 3.1.8      ts-ci -> success
833444e  kysely 0.28.17                      ts-ci -> success
fb74bca  nodemailer 10.1.1                   ts-ci -> success
acbdae2  pnpm.overrides                      ts-ci -> success
028ae86  rustls 0.23.46, h2 0.4.20,
         quinn-proto 0.11.19, anyhow 1.0.104 ts-ci, rust-ci -> success
6d1f393  dependency-audit.yml                rust-ci, markdown-lint,
                                             dependency-audit, ts-ci -> success
```

Local gates at the tip:

- Rust: `cargo fmt --check` OK, `cargo clippy -D warnings` OK, `cargo test
  --all` 66 binaries, 126 passed, 0 failed, 7 ignored (`-j 1`, the host's
  memory being short).
- `services/ingest`: typecheck, lint, build and the entrypoint check OK;
  vitest 74 files, of which 69 passed, 4 failed (the capture marquees, the
  agent's exit code 9, unelevated) and 1 skipped; 290 tests passed, 2
  skipped.
- `services/api` and `dashboard`: their files are the ones S40's gates ran
  on at `1105c07`, `aab07dd` and `5a4209c`; the rebase onto `328416b` changed
  none of them.

Elevated gate 3, run by Manuel on `6d1f393` (`C:\tmp\s40-gate3.ps1`,
2026-10-16, 13:45 to 13:51). Its logs, as pnpm and cargo printed them
(see B14 under *#93: the close*):

```text
01-elev    net session: No hay entradas en la lista.
02-head    6d1f393 / fix/s40-debt-93-dependencies
03-status  status lines: 0
05-build   Finished `release` profile [optimized] target(s) in 1m 38s
06-cargo   test auth_ac_012_logons_on_the_real_log ... ok
           test capture_ac_006_elevated_shutdown_leaves_no_session ... ok
           test net_ac_008_a_real_connection_is_reported_with_its_probe ... ok
           test net_ac_009_an_accepted_connection_and_the_filter ... ok
           test ac_004_cache_hit_terminate_matches_launch_byte_for_byte ... ok
           test ac_007_parent_pid_only_when_parent_dead_at_child_launch ... ok
           test ac_009_events_lost_under_deliberate_etw_buffer_pressure ... ok
08-vitest  ✓ test/servercert-ac-renewal.test.ts (19 tests | 1 skipped) 7798ms
           ✓ test/soar-ac-006-bounds.test.ts (23 tests) 22759ms
           ✓ test/detect-ac-001-marquee.test.ts (1 test) 46811ms
           ✓ test/spec-005-marquee.test.ts (1 test) 40467ms
           ✓ test/net-ac-010-marquee.test.ts (1 test) 25554ms
           ✓ test/auth-ac-013-marquee.test.ts (1 test) 25494ms
           ✓ test/rules-ac-007-loaded-at-start.test.ts (3 tests) 10513ms
           ✓ test/ac-001-marquee.test.ts (1 test) 4449ms
           detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-tVvu4b\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
           spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
           Test Files  74 passed (74)
                Tests  295 passed | 1 skipped (296)
10-agent   logman query CGAgent-KernelProcess -ets: No se encontró el Conjunto de recopiladores de datos.
```

The skipped test is `servercert_ac_008`, which runs only off Windows.

Landed on `main` commit by commit with `git cherry-pick --ff`: the branch
sat on `main`'s tip, so each commit landed under the SHA its CI had run on.
Before each push, `git diff --quiet <branch-sha> HEAD` stopped the loop on a
mismatch; none occurred. One push per commit, each waiting for its push
runs:

```text
SHA f876753: ts-ci -> success
SHA fcbf6ce: ts-ci -> success
SHA 833444e: ts-ci -> success
SHA fb74bca: ts-ci -> success
SHA acbdae2: ts-ci -> success
SHA 028ae86: ts-ci, rust-ci -> success
SHA 6d1f393: markdown-lint, dependency-audit -> success
```

GitHub closed PR #23 as merged, since `main` holds its head; the branch was
deleted, local and on origin.

Decisions (Claude Code's, inside the accepted ADRs):

- Reactive:
  - **The commits reordered**, the agent's crates before the workflow, so
    that no landed SHA is red. Alternative: squash the two.
  - **The cargo job kept strict**, its text corrected. Alternative: a
    severity threshold, which `cargo audit` does not offer without parsing
    its JSON output, and which would let a moderate advisory in the agent's
    TLS stack pass.
  - **anyhow 1.0.104 in the agent commit**, before the gate. Alternative:
    leave the unsound warning to a later change.
  - **`cherry-pick --ff`.** Alternative: new SHAs for the same trees,
    whose push runs would cover commits that never ran on a PR.

### #93: the close

The docs commit that closes #93:

- `docs/security/dependency-review.md`: the runbook the threat model names.
- The threat model: `:204` names the runbook; the record of the system as
  built (*Cross-cutting*) and R12 say what is audited and what is not
  (#110 to #113).
- CLAUDE.md:
  - *Environment facts*: the weekly run on `main`'s tip;
  - *Local pre-commit gate*: a *Dependencies* step, `pnpm audit --prod
    --audit-level=high` when a lockfile or a `package.json` changes, and the
    statement that `cargo audit` has no local mirror;
  - the elevated gate's expected outcome: `net_ac_008`'s and
    `auth_ac_012`'s lines print only under `--nocapture`, as gates 1 to 3
    showed.
- `docs/security/README.md` and `.github/workflows/README.md` link the
  runbook.

Decisions (delegated: the security posture, recorded for Manuel's audit):

- **What the runbook binds.** No advisory is ignored without Manuel
  accepting it as debt; a change to a crate the agent links, or to a package
  ingest's capture or detection path runs on, lands after the elevated
  gate; a crate that `cargo update` adds and the agent compiles is a new
  dependency of `cg-agent`, his. Alternative: leave each to the session's
  judgement.
- **Licences compatible with Apache-2.0**, the workspace's licence, checked
  when a dependency is added. Alternative: a list of allowed licences, which
  #112's tooling would enforce.
- **#112 to roadmap §F's packaging.** An SBOM describes a release, and
  §F makes the first one. Alternative: a sweep now, with nothing released
  to describe.

Self-review: one pass of the fixed SPEC prompt on the working tree, before
the commit. List A, seven points, all fixed:

- A1: the runbook said pnpm's moderate advisories are "printed"; they are
  counted in the summary line.
- A2: the runbook said development dependencies are not audited; `cargo
  audit` reads the agent's.
- A3: #111 said three warnings; the job at `6d1f393` prints two.
- A4: this record stated no open count for the register to match.
- A5: the threat model's cadence sentence (`:204`) named npm, Go and
  Python, beside the runbook that names neither.
- A6: the threat model's *New risks* line cited no handoff for R12's new
  debts.
- A7: CLAUDE.md said a green local gate predicts a green CI run, and
  `dependency-audit` had no local step.

From list B, taken: the as-built record's date (B1); a malicious release
with no advisory, under *What is not covered* (B3); a crate's `build.rs`
(B4); where `cargo audit` runs for a new dependency (B5); a crate `cargo
update` adds (B6); the detection path (B7); pnpm's ignore options (B9);
SPEC-022's other citations of nodemailer 8.0.10 (B10, in #13 below); the
two decisions above (B11); the matrix's test lines and the reviewer's
lists apart (B12); the expected outcome of the elevated gate (B15).
Recorded: `pnpm-workspace.yaml` and `rust-toolchain.toml` do not trigger
the audit (B8; neither holds an override today); the security README's
"Populated by" list (B16). On B14: gate 3's vitest log holds `Ô£ô` and
`ÔÇö` where pnpm printed `✓` and `—` (its UTF-8 read in the console's code
page), and its agent-session log an `Error:` line above the message; the
lines above show what pnpm printed, as handoff-40 did.

### #96: PR #24, waiting on ADR-0003

Branch `fix/s41-debt-96-dev-stack` (PR #24), one commit: NATS and MinIO out
of the dev compose, `.env.example`, the README and the Taskfile, and
`check-compose.mjs` refusing them until a SPEC uses them. Its review (the
fixed code prompt) found that ADR-0003 §Consequences says "The local
development environment requires all five services to be up", which the
change makes false. An ADR amendment is Manuel's: it is on the
owner-review branch `adr/s41-0003-dev-stack` (`b0b1bbf`, NOT YET
RATIFIED), and #96 lands after it. This section is completed when it lands.

### #82: the contract

SPEC-008's second Amendment 2026-10-16, *the session's idle lifetime and
its cap*, lands first as a document. Every request that passes the session
check rewrites the entry: `last_seen` now, and a time-to-live of 8 h or the
time left until 24 h after `issued_at`, whichever is shorter. A session
past its cap is deleted and answers `401`; the rewrite is `SET ... XX`, so
it never brings back a revoked session. The values are NFR-008-002's
defaults; its per-org setting stays an open question, with debt #20.

Decisions (delegated: the deployment contract and the security posture,
recorded in the amendment's *Decision* with their alternatives): what counts
as a use (every request that passes the session check, refused ones
included); the per-org setting deferred to roadmap §F's settings; `SET ...
XX`; a session past its cap deleted; an unreadable `issued_at` ending the
session.

For Manuel's audit, when the code lands: the threat model's §3 sentence
"Sessions are opaque random tokens in Redis, not signed, and last a fixed
8 h (debt #82)" becomes the sliding 8 h within a 24 h cap; and a token in
use works for up to 24 h after its login, where it worked for 8 h.

Two passes of the fixed SPEC prompt on the working tree:

- First pass, List A, four points, all fixed: what counts as a use (item 1
  and the *Decision* disagreed on a refused request); the first Amendment
  2026-10-15 item 9's "one Redis read", not listed in *Effect*; item 1
  citing §Operational §5 for the session check; an api `cgcsrf` cookie that
  does not exist. From List B, taken: the earlier references to "Amendment
  2026-10-16" meaning the first; SPEC-009's fixed 8 h clause; the security
  cost; a store failure in the rewrite; milliseconds; the handoff named; the
  password change among the deletions; the race test's seam; the README's
  "whichever is shorter". Recorded: the test harness's fixed `issued_at`
  (`services/api/test/helpers/db.ts`), which the code commit changes.
- Second pass, List A, two points, both fixed: this record did not yet list
  the threat model's change; the new text for SPEC-009 left out the cap
  and the deletions. From List B, taken: criteria for an unchanged
  `issued_at`, a refused request as a use, and a failed rewrite or delete;
  `503` over `401` for a failed delete; the per-org setting kept as an open
  question; the cookies' wording; the cost of a Redis that refuses writes;
  "writes once"; "not a time"; the catalog's delegation and edge; the race
  case outside "the real api"; the alternatives of three more decisions; no
  audit row for an expiry. Recorded: ADR-0014's "one O(1) Redis read"
  (`:35`), the cost of the option it chose, which leaves the lifetime to
  SPEC-008 (`:104`): not amended. No third pass ran on these fixes.

## Debts

New this session:

- **#111 — `cargo audit` lets two warnings pass, one in the agent's
  runtime.** rustls-pemfile 2.2.0 is unmaintained (RUSTSEC-2025-0134); it
  is a direct dependency of `cg-agent`, which reads its certificates with it
  (`agent/cg-agent/src/tls.rs`). chacha20 0.10.0 is yanked, in the
  development dependencies only, through testcontainers. The cargo-audit job
  at `6d1f393` prints "2 allowed warnings found"; a third, anyhow 1.0.102,
  went with `028ae86`. Surfaced by the review of PR #23. Destination: rustls-pemfile replaced by rustls-pki-types' PEM reader
  with the next agent change under the elevated gate, as #87 is; chacha20
  with the test tooling's sweep (#110).
- **#112 — No tool checks the dependencies' licences, and no SBOM is
  produced.** A licence is checked by hand when a dependency is added
  (`docs/security/dependency-review.md`); the transitive ones are not.
  Destination: roadmap §F's packaging, with the release artifacts.
- **#113 — The workflows pin their actions by major tag, not by commit.**
  `actions/checkout@v5`, `actions/setup-node@v5`,
  `actions-rust-lang/setup-rust-toolchain@v1` and the rest run whatever
  their tag names when they run. Destination: a later sweep, each action
  pinned by commit SHA.

Changed this session:

- **#20**: SPEC-008's per-org session lifetime joins it, with Open
  question 2 (SPEC-008's second Amendment 2026-10-16, item 4).
- **#13**: SPEC-022's third Amendment (`:398`) names nodemailer 8.0.10 for
  the `ESOCKET` code of a certificate it cannot verify; since `fb74bca` the
  release is 10.1.1, which reports it the same way. A citation for the
  pattern sweep. The same SPEC's `:347`, `:348` and `:376` cite nodemailer
  8.0.10's file paths too, but they sit in an amendment's *What surfaced
  it* and *Decisions*: records of that moment (Class H-inline), not edited.

Closed this session:

- **#93**, by `f876753` to `6d1f393`: the production dependencies past their
  advisories, `dependency-audit.yml`, and the runbook.

**Open at the commit that closes #93: 71** (`docs/debts.md`, which
matches): 69 at the start, #93 closed, #111, #112 and #113 new.
