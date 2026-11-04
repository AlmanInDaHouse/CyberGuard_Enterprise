# Handoff — Session 55

Session 55, 2026-11-04, continues F2 under SPEC-024 with the custody of the
service's paths and identity: install_ac_002 (the service's half),
install_ac_004 and install_ac_005, debt **#142**, and the fresh-installation
part of **#87**. The contract went first, as SPEC-024's Amendment
2026-11-04, approved by Manuel (`85ef9f5`); the implementation landed after
two probe pull requests, two rounds of fresh-context review, CI ALL GREEN
and a green elevated gate (`9053513`). #142 is closed; #87 stays open for the
legacy migration and the package.

## Start and resulting state

- Start: `main` = `origin/main` = `c833e9a`, clean, no stash, only `main` on
  origin, no PR open; `main`'s tip CI green (`markdown-lint → success`).
  Catalogs 22 ADRs / 23 SPECs; open debts 52; Known CI debt none.
- **The opening prompt and the repo disagreed**, and the repo won: the prompt
  expected `ab2938b` or later docs only, a branch `review/s54-141-texts`
  (`8b5a338`) and 53 open debts. `main` was `c833e9a` (five docs commits
  after `ab2938b`, #141's ratified texts), the review branch had been
  deleted when #141 landed (`docs/handoff-session-54.md`, *#141's texts*),
  and the register said 52 (`docs/debts.md:26`). Reported in the first turn.
- Resulting `main`: `85ef9f5` (the amendment), `9053513` (the
  implementation), and this close commit. No branch but `main`, no PR open.
- Catalogs: 22 ADRs / 23 SPECs (SPEC-024 amended in place).
- C: 31 GB free at start; about 1.5 GB of RAM free; Docker Desktop down at
  start (the gate script started it).

## The contract: SPEC-024 Amendment 2026-11-04 (`85ef9f5`)

Seven items: installed mode is `--service`; its configuration, trust anchor
and identity at the fixed paths under `%ProgramData%` (equal but for ASCII
case) and `server.heartbeat_url` required (code 2); a handle-based custody
check (owner, protected DACL, allow entries) of the data directories (the
file log's rule, code 13, before the configuration is read), `agent.toml`
(owner SYSTEM, SYSTEM and Administrators, code 2), `server-ca.pem` (same,
code 6, after `RUNNING`) and the identity (owner SYSTEM, SYSTEM alone, code
5); the identity written file by file with its final security,
`identity.json` last, an `identity\` without all three files refused as
incomplete; the certificate's Ed25519 key and common name checked against
the key and `agent_id` (code 3 at enrollment, 5 at load); token hygiene
keeping the original's owner and protected DACL; DPAPI's decrypted buffer
zeroed.

- **Who approved it.** It extends the second Amendment 2026-11-01, which
  Manuel approved as packaging; the third fresh-context review flagged that
  the classification was therefore his, and CLAUDE.md puts an unclear one on
  his list. Put to him with the compare URL on branch
  `review/s55-spec024-custody` (`59f0f2e`, `NOT YET RATIFIED`); his answer,
  2026-11-04: **"Sí, apruebo"**. Landed with the marker stripped and his
  approval recorded in the text; markdown-lint green locally and in CI.
- **Its reviews** (CLAUDE.md's fixed SPEC/ADR-text prompt, fresh context,
  isolated worktree), four rounds before the approval:
  - Round 1, List A: the order turned R17's code 13 into 2 (the ratified
    threat model says 13) → the data directories are checked first, with
    code 13; owner Administrators accepted against §Data contracts §2's
    "SYSTEM owner" → owner SYSTEM and a protected DACL for the files, the
    directories keep the file log's rule; the anchor checked before
    `RUNNING` against item 3's `START_PENDING` → after `RUNNING`; a refusal
    line that could not be written for the missing heartbeat URL → a line of
    its own. List B (14): fixed or folded in (scope "the service", torn
    `identity.json`, the DER reader named, the SCM tests' move, the filtered
    token disclosed, the order, NULL DACL, the catalog).
  - Round 2, List A: item 3 credited the second amendment with the log's
    owner rule → this amendment states it; item 1 versus item 7 (console) →
    items 2 to 6 only; R17 → kept by the order above.
  - Round 3, List A: a write failure before any file left a state no start
    could tell from "never enrolled" → `identity\` is the marker: absent
    only when `identity\` is; the one undetectable failure (creating
    `identity\`) is §Operational §2's consumed token with a lost response,
    amended by scope; the approval classification → Manuel.
  - Round 4, List A: the catalog line gave the directories the files' rule →
    fixed. List B: the restricted token's description
    (`DISABLE_MAX_PRIVILEGE` disables, does not delete), the order of the
    `%ProgramData%` failure (code 1), "either of the first two", the setup
    half's pointer, the catalog's pending status → fixed.

## What `9053513` implements

- `src/custody.rs` (new): `%ProgramData%` from `SHGetKnownFolderPath`;
  `Policy` (owners, allowed principals, protected); `check`,
  `check_if_exists`, `read_checked` (read from the checked handle),
  `ensure_directory`, `create_new_file` (`CREATE_NEW` with the final
  descriptor, no reparse point followed), `replace_keeping_security` (FR-014's
  rewrite: checks the original, copies its owner and DACL into a new
  temporary file, moves it over). The file log's code moved here unchanged in
  behaviour (`service/log_file.rs`).
- `src/x509.rs` (new): a strict DER reader of the certificate's subject CN
  and Ed25519 SPKI (shortest-form lengths, nothing after the Certificate or
  the SPKI key), over the PEM reader TLS uses; no new dependency.
- `src/service/mod.rs`: `InstalledPaths`, `check_config_path`,
  `check_installed_config`, `is_fixed_path`, `identity_rounds_until_stopped`.
- `src/service/windows.rs` `serve()`: relative `--config` (2) →
  `%ProgramData%` (1) → fixed `--config` (2) → `CyberGuard` and the data
  root (13) → `agent.toml` read from its checked handle (2) → the plain path
  and the fixed paths and heartbeat URL (2) → the fixed paths used from here
  on → lock (10) → file log (13) → `RUNNING` → `server-ca.pem` (6) →
  identity custody, then completeness (5) → the rounds.
- `src/identity.rs` `mod installed`: `check`, `ensure_identity`,
  `check_issued`, `load`, `persist`, `hygienize_token`; the console's
  functions unchanged; `check_consistency`.
- `src/config.rs`: `load_from_str`. `src/secure_storage.rs`: the DPAPI
  buffer zeroed before `LocalFree`, an empty output handled.
- Tests: unelevated `install_ac_002_installed_config.rs` (5),
  `install_ac_004_custody.rs` (5), `install_ac_005_identity_writes.rs` (8),
  `x509_identity.rs` (2), fixtures `tests/custody_fixtures/`; the SCM
  harness moved to `tests/scm/mod.rs`, `install_ac_007_service.rs`'s 11 tests
  moved to the fixed paths, and the new SCM target
  `install_ac_004_custody_service.rs` (8: paths, configuration and anchor
  custody, the identity's owner and DACL, effective access under a created
  standard user and a restricted administrator token with positive controls,
  the restart that loads the identity, no token in the logs, partial,
  inconsistent and insecure sets, a wrong certificate in the answer, an
  unrelated file in `identity\`). `Cargo.toml`: the `[[test]]` and the
  dev-dependency feature `Win32_NetworkManagement_NetManagement` (test-only,
  no new crate); `rust-ci.yml`: the target in the scm-tests step.
- Runbook `docs/operations/install.md`: the service's fixed paths, its
  custody checks and their codes, the editor note, code 13's repair.

## Decisions

### Anticipated (Claude Code)

- **Installed mode = `--service`, fixed paths compared as strings** (ASCII
  case only), not resolved. Rejected: "absolute and inside the tree" (the
  custody of arbitrary parents is ill-defined).
- **Files created with their final descriptor by `CREATE_NEW`,
  `identity.json` last**. Rejected: a staging directory renamed into place
  (the incomplete set is already detectable, and an empty `identity\` is now
  incomplete too).
- **A DER reader of the agent's own**. Rejected: a direct dependency on an
  X.509 crate (`x509-parser`, `rustls-webpki` direct), a new dependency of
  the elevated agent and Manuel's decision.
- **Consistency checks in installed mode only**; the console keeps FR-008.
  Rejected: all modes (SPEC-002's amendment scopes it to the service, and
  `enroll_ac_010` pins the console's behaviour).
- **The SCM tests delegated to a subagent** under a fixed brief, with the
  constraint that they compile against the tree without the implementation
  (the probe); reviewed by the fresh-context lenses.

### Reactive corrections

- The contract's four review rounds (above) changed the order (directories
  first, code 13), the owner rule (SYSTEM for the files), the anchor's
  place (after `RUNNING`) and the incomplete set (`identity\` the marker).
- **CI red on `8ca5257`** (rust-ci Windows, `cargo test`): the unelevated
  custody tests assumed the creator's owner is the user; on the runner the
  elevated built-in Administrator's default owner is Administrators and SDDL
  writes it `LA`. Diagnosed from the job log; fixed with the token's
  default owner and SDDL normalised through the Win32 conversions.
- **An explicit DACL at creation comes out protected**: an unprotected
  fixture is built by inheritance (seen locally, `custody_fixtures`).
- **A SYSTEM-created directory is owned by Administrators** unless the
  descriptor says otherwise (probe 1: the old service's `identity\` was
  `O:BA` with Administrators' inherited entry, #142 itself): every object
  the service creates carries `O:SY`.
- Code review round 1 (fixed prompt, FFI/security lens, SCM-tests lens) →
  `e6c7bd0`: `Principal::Sid` length checked before Win32 reads it;
  `FILE_FLAG_OPEN_REPARSE_POINT` on every `CREATE_NEW`; the hygiene checks
  the original against the configuration's policy before copying; the fixed
  paths used after the check (`canonicalize`); a strict DER reader; the TLS
  PEM reader; DPAPI's empty output; the hygiene warning without the TOML
  parser's text; and, in the SCM tests, the code-13 test with a refused
  `agent.toml`, the case loop's drop order, owner and DACL unchanged after a
  refusal, the anchor beside a valid identity, the file log free of the
  token, a positive control inside the probed tree, a `Drop` that cannot
  panic, the unrelated file in `identity\`.

### Manuel's decisions in chat

- 2026-11-04, the amendment: "Sí, apruebo" (the recommended option of one
  question naming its six points and the compare URL).

## Self-review and fresh-context reviews

Claude Code's matrix (contract → test):

| Criterion | Tests | Verdict |
|---|---|---|
| 002: plain HTTP refused | `install_ac_007_service.rs` (plain path); `install_ac_002_installed_config.rs` | covered |
| 002: unsafe paths refused before enrollment | `install_ac_002_installed_config.rs` (all spellings for `--config`; four for each key); SCM `paths_other_than_the_fixed_ones_are_code_2` | covered (non-ASCII case: none) |
| 002: configuration/anchor others can reach | SCM `a_configuration_others_can_reach_is_code_2`, the two anchor tests; `install_ac_004_custody.rs` | covered |
| 002: no token in logs | SCM main test (every file of `logs\`), entry checks | covered (service half) |
| 002: setup half (routes, fingerprints, bootstrap ACLs, argv/env/MSI) | none | missing by venue: the package (install_ac_001/015) |
| 004: others cannot read, replace, delete, change DACL, take ownership | SCM effective-access probe (standard user, restricted token, positive controls) | covered; weak: `FILE_DELETE_CHILD` on the parent and the hygiene temporary not probed; the token is not a UAC filtered one (disclosed) |
| 004: SYSTEM can use the identity | SCM restart: heartbeat, enroll count 1 | covered |
| 004: owner SIDs and DACLs | SCM `assert_system_alone`; unelevated owner/DACL comparisons | covered (aliases compared) |
| 005: no broadly readable intermediate | final descriptors; mutation M3 | weak by construction: the descriptor is applied by `CreateFileW` itself; no window to observe |
| 005: injected persistence failure | `a_write_that_stops_leaves_a_refused_set`, `persist_never_overwrites_a_file` | covered |
| 005: injected ACL failure | broad `identity\` | weak: no failing descriptor injected |
| 005: partial/mismatched fails closed, no heartbeat, no re-enroll, nothing changed | SCM six-case loop (bytes, listing, owner and DACL); unelevated | covered |
| 005: wrong certificate in the answer → 3, nothing written | SCM test; unelevated `check_issued` | covered |
| Item 6: hygiene keeps owner and DACL | unelevated two tests; SCM main test | covered |
| Item 7: DPAPI buffer zeroed | none | read in review (the amendment says so) |

Local red evidence, by mutation of the implementation and restore
(`C:\tmp\s55-mutate.js`): eight mutations (custody check, shape check,
inheriting writes, consistency, fixed path, heartbeat URL, hygiene, the
CN count), each failing 1 to 10 of the new tests; the last needed the two-CN
test added before it failed.

Fresh-context code reviews (Workflow, isolated worktrees; CLAUDE.md's fixed
prompt plus lenses), dispositions:

- **Round 1 at `8ca5257`** — fixed prompt: no contradiction; weak/missing as
  in the matrix; List 3: `check_identity_set`'s stale doc (fixed), the
  runbook's wording (fixed), #142 in the register (this close). FFI lens: no
  blocker or major; nine minor, eight fixed in `e6c7bd0`, one operational
  (the configuration's share mode) in the runbook. SCM-tests lens: the CI
  red (fixed), creation not observed (recorded weak), the R17 test passing
  without the change (strengthened, see below), drop order, owner/DACL
  unchanged, anchor beside identity, file log, positive control, `Drop`
  (all fixed); the remaining gaps recorded in #146.
- **Round 2 at `e6c7bd0`** — fixed prompt: no contradiction; the matrix's
  weak points; the DER reader stricter than "validates nothing else" (a
  CN in a string type other than UTF8String or PrintableString refused;
  the ingest server writes `CN=${agentId}` with `@peculiar/x509`; recorded
  in #146 until a test runs the reader over a server-issued certificate).
  FFI lens: no blocker or major; three minor (the PEM read differs from TLS
  in what it rejects; the checked and the copied descriptors are two reads;
  the hygiene reads the file by path, so an administrator's edit between the
  read and the move is lost), each reachable only by SYSTEM or an elevated
  administrator → recorded in #146, not fixed, since a fix after the green
  gate would need another gate for no change at the trust boundary.
- The code-13 test still passes on the old tree (probe 2): the old tree has
  no `agent.toml` check that could come first, so the test guards the order
  against a change that would check `agent.toml` before the directories, not
  against the old code. Recorded.
- The reviewers' worktrees were removed and the branch checked after each
  round. The reviewer shares Claude Code's model: a second reading, not a
  second judge.

## The probes (CLAUDE.md compensating control 1; SPEC-024 Amendment 2026-11-01, point 4)

- **Probe 1, PR #84**, `d590854` = `c833e9a` + the SCM tests, step with
  `continue-on-error`: run 41826454875, Windows job 126562292742: the 6
  tests of `install_ac_004_custody_service` **failed for the missing
  behaviour** (the service reached `RUNNING` where it must stop; `identity\`
  `O:BAD:(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)`; code 6 where 3 is due); the
  11 migrated `install_ac_007_service` tests passed on the old tree.
- **Probe 2, PR #86**, `d5258fe` = `85ef9f5` + the review-round SCM tests:
  run 41835594082, Windows job 126593547747: 7 of 8 failed for the missing
  behaviour; `another_file_in_the_identity_directory_is_not_checked`
  passed (a guard on behaviour the old tree has); the 11 passed.
- Both closed without landing, their branches deleted.

## Gates and CI

- **Local gate** on the landed tree (`e6c7bd0`'s): `cargo fmt --all --
  --check` clean; `cargo clippy --all-targets --all-features -- -D
  warnings` clean; `cargo test --all`: **201 passed, 0 failed, 11 ignored**;
  `node .github/scripts/agent-version-matrix.mjs`: "agent version matrix:
  all steps passed"; markdownlint 0 issues. The SCM tests do not run here.
- **CI**, full SHAs: `85ef9f5` (markdown-lint → success, ALL GREEN);
  `8ca5257` FAILURES (rust-ci, run 41827257484, the CI red above);
  `e6c7bd0` ALL GREEN (markdown-lint, dependency-audit, ts-ci, rust-ci,
  deploy-ci), the scm-tests step 8/8 and 11/11 (Windows job 126595147572);
  `9053513` on `main`: see *Landing*.
- **Elevated gate s55-gate1, green.** `C:\tmp\s55-gate1.ps1` (ASCII, parser
  0 errors), run by Manuel; literal lines from the UTF-16 logs:
  - `net session exit: 0`; `docker engine: UP`, `ServerVersion=29.1.2`.
  - `HEAD  = e6c7bd02e08e4272655a259e75aa73a45b8c12f2`, `TREE  =
    c463ca3f292f393cb2207dede58125afaa313075`; `release exe:
    2026-11-04T14:32:32.0460074+01:00  commit: 2026-11-04T14:12:50+01:00`.
  - `--ignored`: 11 passed, 0 failed: `auth_ac_012_logons_on_the_real_log`,
    `capture_ac_006_elevated_shutdown_leaves_no_session`,
    `net_ac_008_a_real_connection_is_reported_with_its_probe`,
    `net_ac_009_an_accepted_connection_and_the_filter`,
    `ac_004_cache_hit_terminate_matches_launch_byte_for_byte`,
    `ac_007_parent_pid_only_when_parent_dead_at_child_launch`,
    `ac_009_events_lost_under_deliberate_etw_buffer_pressure`, and
    install_ac_009's four (`test result: ok. 4 passed; 0 failed; 0 ignored;
    0 measured; 8 filtered out`).
  - vitest: `Test Files  85 passed (85)`, `Tests  503 passed | 2 skipped
    (505)`, `Duration  483.67s`.
  - `detect_ac_001 captured image_file_name — probe:
    C:\Users\manul\AppData\Local\Temp\cg-detect-probe-isnHbn\winword.exe;
    child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`;
    `detect_ac_001 drained 88 rows in 1 cycles; capture checked at 35.27 s,
    drained at 38.088 s`; `"detect_marquee_elapsed_seconds":38.101`;
    `spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe;
    terminate: cmd.exe`.
  - `logman query CGAgent-KernelProcess -ets` → "No se encontró el Conjunto
    de recopiladores de datos." (none left).
  - Done: `=== s55-gate1 done 2026-11-04T14:42:41.9367724+01:00 ===`.

## Landing

- PR #85 was one commit, `e6c7bd0`, on `85ef9f5`. Landed by `git merge
  --squash` (guard `git diff --cached --quiet feat/s55-installed-custody`
  passed; `git diff --quiet feat/s55-installed-custody HEAD` after the
  commit) → **`9053513`**, the same tree `c463ca3` the gate and CI ran on.
  CI on `9053513`: markdown-lint, dependency-audit, ts-ci, rust-ci →
  success, **ALL GREEN**.
- PR #85 closed with a comment naming the landed SHA; the branch deleted,
  local and origin.

## Test baselines at close

- Non-elevated `cargo test --all` 201 passed / 0 failed / 11 ignored; the SCM
  suite 19/19 in CI (8 + 11); elevated `--ignored` 11 passed; vitest elevated
  85 files, 503 passed, 2 skipped; unelevated, the four capture marquees fail
  with exit 1 on this host (CLAUDE.md *Environment facts*), not re-run this
  session.

## Debts

- Closed: **#142**, by `9053513` (PR #85; contract `85ef9f5`; elevated gate
  s55-gate1 green).
- Destination changed: **#87** — the fresh service installation's custody
  is done (`9053513`); open for the legacy migration (install_ac_013), the
  package's creation of the configuration and trust anchor with owner
  SYSTEM (install_ac_001/015) and §Operational §2's disclosure of earlier
  copies.
- Opened: **#146** — custody residuals reachable only by SYSTEM or an
  elevated administrator, and test gaps: the certificate PEM is read as TLS
  reads its first block but not rejected as TLS rejects a malformed later
  one (code 6 at the next start instead of 3); `replace_keeping_security`
  checks and copies the descriptor in two reads; the token rewrite reads
  `agent.toml` by path, so an administrator's edit during it is lost; the DER
  reader is not run over a certificate the ingest server issued; untested:
  an ACE of another type, a file symlink at a `CREATE_NEW` name or as the
  leftover temporary, `FILE_DELETE_CHILD` on the parents, the hygiene
  temporary's access, an injected descriptor failure, a non-ASCII case
  variant. Destination: the next change to `custody.rs` or the identity
  (install_ac_013), with install_ac_016's run against a real server.
- Open at close: **52**.

## Waiting on Manuel

Nothing from this session. The packaging change (install_ac_015) needs his
confirmation of the .NET SDK and WiX first use, with exact versions and
commands, before it runs (his instruction this session).

## How Session 56 resumes

1. Start protocol (CLAUDE.md). `main` is this handoff's close commit; no PR,
   no branch but `main`; 52 open debts; 22 ADR / 23 SPEC. Check C:, RAM and
   Docker before builds.
2. F2's next item, in dependency order: the package (install_ac_001 and
   install_ac_015; WiX v7 under SPEC-024 Amendment 2026-11-01, point 1).
   Before any `dotnet`/`wix` command: report to Manuel the exact .NET SDK
   version, the exact WiX release and its extensions, and the commands
   (winget for the SDK, `dotnet tool restore` from a manifest), and wait for
   his confirmation. The package writes the configuration and the trust
   anchor owned by SYSTEM with protected DACLs (SPEC-024 Amendment
   2026-11-04, item 3) and never creates `identity\` (item 1).
3. Or, if Manuel prefers, install_ac_013 (the legacy migration, #87) first:
   it needs no new tool. Keep the release freeze.

## Environment and memory

The Bash tool's heredocs again mangled backslashes and once failed on a long
body; edit scripts were written with the Write tool and run with `node
C:/tmp/...`. Workflow reviewers in isolated worktrees under
`.claude/worktrees/` were removed after checking. The session-close
auto-memory entry is `project-session-55-close.md`, indexed in `MEMORY.md`.
