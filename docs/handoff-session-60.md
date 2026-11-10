# Handoff — Session 60

Session 60, 2026-11-10, did C4 of the package (SPEC-024 Amendment
2026-11-09, items 1 to 3 and 7): `agent/cg-setup` joined the workspace as a
library, with the bootstrap file's admitted keys and their validation, the
staged configuration and anchor, and preflight's states read by
`service::layout::classify` with the service's directory policy, mapped to
item 7's codes. Three commits, each green on its PR and on `main`. C5 did
not start: it would not have fitted with the quality bar in this session.

## Start and resulting state

- Start: `main` = `origin/main` = `6517926`, clean, no stash, no branch but
  `main`, no PR open; tip CI green (markdown-lint, the rest skipped by
  path); 22 ADRs / 23 SPECs; open debts 53 (53 rows); Known CI debt none.
  The repo agreed with the handoff. Memory before the gate: 8.4 GB free of
  15.8 (Docker and the k8s stack down); `C:` 24 GB free.
- Resulting `main`: `427fa31` (C4), `023504c` (the first review's fixes),
  `a801019` (the second review's), then this close's commit. No PR open, no
  branch but `main`, no worktree left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### `cg-setup`'s preflight (`427fa31`, PR #98)

- `Cargo.toml`: `agent/cg-setup` a workspace member. `agent/cg-setup/Cargo.toml`
  has a `[lib]` alone (the executable comes with its verbs, C5), and its
  dependencies are cg-agent and crates cg-agent already has at the same
  requirement (`pem`, `toml`, `uuid`, `zeroize`, `windows-sys` 0.59):
  `Cargo.lock` gains the `cg-setup` package and nothing else.
- `src/codes.rs`: item 7's codes and `Refusal { code, line }`, lines
  `cg-setup: …`.
- `src/inputs.rs`: `parse_bootstrap`, the five admitted keys, each required
  and in its form; repeated fingerprints are one; a refusal names a key it
  does not admit only when the agent's configuration knows it, so no line
  repeats what the operator wrote; a parse error gives line and column, not
  the parser's message, which quotes the line.
- `src/staged.rs`: `staged_config` (the inputs' URLs and token, the fixed
  installed paths, the host name, a UUIDv7 placeholder, nothing else),
  `staged_anchor` (the checked certificates re-encoded as PEM, nothing else
  of the source), `check_staged` (the service's own check).
- `src/platform.rs`: code 20 (Windows on a native x86-64 machine,
  `IsWow64Process2`) and 21 (Administrators enabled in the token,
  `CheckTokenMembership`).
- `src/preflight.rs`: `Request` (the three bundle variables, an empty one
  not given), `parse_migration`, `decide` (item 3's table), `Host`,
  `assess` (state, decision, and on a route that stages the bootstrap (23),
  its inputs (24), the anchor's source against its fingerprints (25) and the
  generated configuration (24)), `preflight`.
- cg-agent, no behaviour changed: `service::anchor` (`is_fingerprint`,
  `fingerprint`, `check_certificates`, `check_anchor`) moved out of
  `migration.rs`; `service::check_staged_config` and `service::READ_LIMIT`
  moved out of `adoption.rs` and `migration.rs`, so that the adoption and
  the setup share them; `custody::read_local_checked`,
  `custody::is_remote_nt_path`, and `custody::bounded` made public.
- Tests: `agent/cg-setup/tests/install_ac_002_setup_inputs.rs` (portable),
  `install_ac_002_setup_bootstrap.rs` and `package_preflight.rs` (Windows),
  with their own `fixtures`.

### The first review's fixes (`023504c`)

- A bootstrap file with another link is taken (the link shares its owner
  and DACL); the reparse and directory refusals stay.
- The bootstrap's and the anchor source's opens carry
  `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`; the source opens with
  backup semantics, so a directory is named.
- `Host::hostname` is `Option`, Windows' read on a route that stages alone.
- `platform::check_elevation_of(token)` (`unsafe`), used by `check_elevation`.
- Tests: item 7's numbers pinned; fingerprints with letters; the DACL
  refusals' reasons with a positive control; the remote check's reason
  asserted in CI; a filtered administrator's token refused on any host.

### The second review's fix (`a801019`)

A host whose name is not given stages Windows' own (test); the console
layout's refusal no longer repeats its first clause.

## Red evidence (CLAUDE.md compensating control 1)

Before the implementation, every function of the API was `unimplemented!()`.
`cargo test -p cg-setup --no-fail-fast` ran 31 tests, and all 31 failed at
the stub of their own behaviour, none in its setup. The literal endings were
`not implemented: assess` (11), `parse_bootstrap` (9), `Request::new` (4),
`placeholder_id`, `staged_anchor`, `is_remote_nt_path`, `check_elevation`,
`check_platform`, `Host::system` and `parse_migration`. Each review fix's
test was seen failing on the code without its fix:

- a hard-link refusal put back fails `..._not_a_regular_local_file_is_code_23`
  (`matches!(assess(..linked..), Ok(Plan::Stage(_)))`);
- the source opened without backup semantics fails
  `..._an_anchor_that_does_not_match_is_code_25` (`open …\operator: Acceso
  denegado. (os error 5)`);
- an eager host name fails `..._the_machines_host_is_the_services`
  (`left: Some("ManuelPC")`);
- `check_elevation_of` accepting any token fails
  `..._a_filtered_administrators_token_is_not_elevated` (`filtered: ()`);
- a constant host name fails `..._the_staged_host_name_is_windows`
  (`left: Some("constant-host")`).

Ignoring the token instead cannot fail on an unelevated terminal; on the
elevated runner the filtered-token assertion catches it.

## Decisions

### Anticipated (Claude Code)

- **`cg-setup` a library now, on cg-agent's code.** `classify`, the
  adoption's configuration checks, the anchor's exact-set rule and the
  custody reads are cg-agent's own functions, so "the same checks the
  service applies" holds by construction. Discarded: copies in `cg-setup`;
  a stub executable before its verbs.
- **The bootstrap file's policy:** owner SYSTEM or Administrators, allow
  entries for those two alone, the DACL as it is (inherited entries
  included), protection not required. Discarded: a protected DACL, which
  §Data contracts §3 does not ask for.
- **"Local"** = the handle's NT path is not under `\Device\Mup\` or
  `\Device\LanmanRedirector\`. Discarded: refusing by the path's text, which
  a junction defeats.
- **Paths:** a relative `CG_BOOTSTRAP` is 23 and a relative
  `server.trust_anchor_path` 24: Burn's working directory is not the
  operator's. The anchor's source is read once, following links, checked by
  fingerprint, at most `READ_LIMIT`.
- **The staged anchor is the checked certificates re-encoded**, so no other
  PEM block of the source (a key, a trailing fingerprint line) is staged.
  Discarded: the source's bytes.
- **Item 3's readings** (recorded as #151 for the text): an untrusted state
  is 27 whatever is asked, since its bullet refuses it as a state; on a
  migration in progress or a console layout, a malformed request is 24 and
  a bootstrap with a migration 28; on any other state a migration request is
  28 before its form; an enrolled host's bootstrap is 30 before it is read;
  a host that cannot be read stays 27 (S59's decision), and a
  `%ProgramData%` or host name that cannot be read is 40.
- **Platform:** a native x86-64 Windows, an x86-64 process emulated on ARM64
  refused; no Windows version floor (see *Waiting on Manuel*).
- **Elevation:** Administrators enabled in the token. Discarded:
  `TokenElevation`, which an elevated token of a non-administrator also has.
- **Repeated fingerprints are one,** as the service's start arguments make
  them. Discarded: refusing them, a check stricter than §Data contracts §3.
- **Test names:** `install_ac_002_setup_*` for install_ac_002's setup half
  (the anchor's fingerprints, HTTP, the bootstrap's ACLs, no token in a
  line), `package_preflight_*` for the preconditions of install_ac_001/011.
- **No change to `check-licences.mjs`:** `cg-setup` brings no crate beyond
  cg-agent's; its notices come with C8 (#130).
- **No elevated gate:** the diff touches none of its paths (`etw/`, `cges/`,
  `delivery.rs`, `paths.rs`, `lib.rs`, `envelope.rs`, `logon/`, `services/`,
  `rules/`); `custody.rs` and `service/` were outside them in S59 too.

### Reactive corrections

- The test borrowed temporaries in the item-3 table (E0716), fixed by
  binding the requests.
- `GetTokenInformation(TokenElevation)` fails with a buffer larger than the
  structure: the fixture passes its exact size.
- Clippy (`not_unsafe_ptr_arg_deref`) on `check_elevation_of`, made `unsafe`
  with its safety contract.
- The first review (below) found the hard-link refusal stricter than the
  contract (the auto-memory's contract-narrowing rule), the opens without
  SQOS, the host name read on routes that do not use it, and weak tests; all
  fixed in `023504c`. The second found the host name's production branch
  untested after that fix; fixed in `a801019`.
- `SECURITY_SQOS_PRESENT` is `FILE_FLAG_OPEN_NO_RECALL`'s bit: kept, since
  a pipe server impersonating the elevated setup is the worse case; a
  dehydrated placeholder as the anchor's source is refused 25 (#150).

### Manuel's decisions in chat

None this session.

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix (contract: SPEC-024 Amendment 2026-11-09, items 1-3 and 7, with §Data contracts §2-§3)

| Criterion | Test | Verdict |
|---|---|---|
| Item 1: a workspace member, the existing binding, no new crate | `Cargo.lock` diff: the `cg-setup` package alone | covered by the diff |
| Item 1: no privilege enabled, the token not adjusted | none: `src` holds no `AdjustTokenPrivileges` | C5's real `cg-setup` |
| Item 1: one executable, the release version, the chain | none | C5, C6, C8 |
| Item 2: the five keys, any other refused | `..._five_keys_is_taken`, `..._any_other_key_is_refused`, `..._each_admitted_key_is_required` | covered |
| Item 2: the bundle variables | `package_preflight_an_empty_variable_is_not_given`, `..._the_migration_variables_have_the_services_form` | covered (Burn's passing: C5/C7) |
| §DC §3: fingerprints' form, exact set, no extra root | `..._each_key_has_its_form`, `..._repeated_fingerprints_are_one`, `..._does_not_match_is_code_25`, `..._the_checked_certificates_alone` | covered |
| §DC §3: a regular local file others cannot reach (23) | `..._others_can_read`, `..._inherits_access_for_others`, `..._another_owner`, `..._not_a_regular_local_file`, `..._held_for_writing`, `..._remote_bootstrap` | covered; a specific user's SID and the production owner rule against a user-owned file: weak |
| §DC §3: no echo | `assert_clean` in every refusal, `..._without_quoting_it`, `..._print_no_token` | covered |
| §DC §2-§3: fixed paths, Windows' host name, UUIDv7, no `setup` table, defaults | `..._inputs_and_the_fixed_paths`, `..._a_new_uuidv7`, `..._the_staged_host_name_is_windows` | covered |
| Item 3: HTTP and invalid input by the service's checks (24) | `..._plain_http_is_refused_by_the_services_checks`, `..._invalid_input_is_code_24` | covered |
| Item 3: platform, elevation | `..._a_supported_platform`, `..._elevation_is_the_tokens`, `..._filtered_administrators_token_...`, `..._any_other_platform_is_code_20` (Linux) | weak: 20 is never produced on Windows; `preflight()`'s order untested |
| Item 3: the states and their table | `..._each_state_takes_what_item_3_says`, `..._read_with_the_services_classification` | covered; the data root's reparse point rests on `package_layout.rs` |
| Item 3: the migration route reads no file; an enrolled host takes no token | `..._reads_no_file_under_p`, `..._refuses_a_bootstrap_without_reading_it` | covered |
| Item 3: nothing changes the host | `refused()`, `staged()`, `assess_unchanged()` tree snapshots | weak: names and sizes, not security |
| Item 3: 26, 29, the staging, the record, the stop (31) | none | C5 |
| Item 7: the numbers | `package_preflight_the_exit_codes_are_item_7s` | covered |

### Fresh reviewers

First pass, on `427fa31`: the fixed code prompt (isolated worktree) and a
read-only workflow of three lenses (custody, contract, tests), each finding
given a refuter (26 agents).

- Fixed prompt, List 1: weak the bootstrap's access (test policy only), the
  remote check (any 23 passed), the order of the input checks, 20/21, item
  7's numbers; missing the C5 items. List 2: Untrusted with a migration 27
  against 28; a malformed migration 24; code 40 for `%ProgramData%` and the
  host name; HTTPS checked after the anchor. List 3: cg-agent's public
  surface; the locality heuristic and the SMB session before refusal; the
  anchor's source opened following links, possibly remote or a pipe; the
  staged anchor re-encoded; the policy and elevation choices; nested lines.
- Lens findings standing after refutation: the host name read before the
  state (40 on an enrolled host); the hard-link refusal stricter than §Data
  contracts §3; the source's directory case passing for the wrong reason;
  the remote test; item 7's numbers unpinned; uppercase digits never tested;
  elevation's filtered branch untested in CI; the 23 reasons unasserted;
  `server must be a table` unpinned; an unused import; intermediate
  junctions followed; the elevated opens reachable by a redirected path (a
  pipe, SMB). Refuted: a planted `agent.migrating` giving 28 (item 3's
  order), the Untrusted/28, bootstrap+migration and HTTPS-order readings
  (consistent with the text), token copies not zeroed (no requirement), a
  pre-tightening read handle (the operator's procedure), `tree()` (nothing
  here writes), the reparse test (cg-agent's tests cover `classify`).
- Dispositions: fixed in `023504c`, the host name, the hard link, the SQOS
  and backup-semantics opens, and every test weakness. Recorded: the
  readings as #151; intermediate junctions, the SMB session, the
  non-MUP redirectors, pre-tightening handles and the source's links as
  #150; nested lines and cg-agent's public surface as they are (the setup
  links the library; no new behaviour in the service); Linux built in CI.

Second pass, on `023504c` (the auto-memory's rule for security fixes): the
fixed prompt again and a regressions lens with refuters. No contradiction;
List 2 repeated 40, "unreadable is untrusted" and the 27/28 ambiguity
(#151). Standing: the host name's production branch untested (fixed in
`a801019`, its test seen failing on a constant); the SQOS bit colliding with
`FILE_FLAG_OPEN_NO_RECALL` (kept, #150). Refuted: the hard link as a lost
signal (§Data contracts §3 asks for one link only of the staged files).
Weak, recorded: "regular" is not a `GetFileType` check (a pipe or device
that passes the owner and DACL check is one no ordinary user can create
with an admitted owner); `IsWow64Process2` loads only on Windows 10 1709 or
later (*Waiting on Manuel*); `Inputs`' `Debug` prints the URLs whole.
`a801019` changed a test and a message only; no third pass.

## Gates and CI

- Local Rust gate on `427fa31`'s tree: `FMT_OK`; clippy `Finished`, no
  warning; "binaries 87 passed 438 failed 0 ignored 11"; "agent version
  matrix: all steps passed"; `cargo audit` exit 0 ("2 allowed warnings",
  RUSTSEC-2026-0440 and RUSTSEC-2026-0386, as on `main`).
- On `023504c`'s tree: `FMT_OK`; clippy `Finished`; "binaries 87 passed 440
  failed 0 ignored 11"; the matrix passed.
- On `a801019`'s tree: `FMT_OK`; clippy `Finished`; "binaries 87 passed 441
  failed 0 ignored 11"; the matrix passed.
- The elevated gate does not apply (*Decisions*).
- CI, PR #98: `427fa31`, `023504c` and `a801019` each markdown-lint,
  dependency-audit, ts-ci, deploy-ci and rust-ci `success`; on `023504c`
  rust-ci's Windows job ran the CI-only assertions (the loopback share's
  "is not on a local volume", the runner elevated). On `main`: `427fa31`
  rust-ci, markdown-lint, dependency-audit, ts-ci `success` (deploy-ci runs
  on pull requests alone); `023504c` and `a801019` rust-ci and ts-ci
  `success`. ALL GREEN.

## Landing

- `427fa31`, `023504c`, `a801019`, each a fast-forward of `main`, one per
  push, CI green on the branch before and on `main` after. PR #98 closed,
  its branch deleted. The landed SHAs are the branch's, so each tree is the
  one CI ran.

## Test baselines at close

- Non-elevated `cargo test --all`: 441 passed, 0 failed, 11 ignored (cg-setup
  34: 10, 12, 12).
- The SCM suite in CI: 35 tests, unchanged.
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: none.
- Opened:
  - **#150**, `cg-setup`'s reads as an elevated administrator: a parent
    reparse point of the bootstrap's path is followed (a file on a volume an
    ordinary user controls can pass the owner and DACL check); the remote
    check runs after the open and names only MUP's volumes; a read handle
    opened before the operator tightened the DACL is not seen; the anchor's
    source follows links and may be remote, and the SQOS bit refuses a
    dehydrated placeholder. Destination C10: the runbook creates both files
    in a local directory only SYSTEM and Administrators reach, protection
    before the token; reopens if C5 reads either again.
  - **#151**, item 3's four readings (*Decisions*). Destination: the
    SPEC-024 amendment C5 brings for the record and the stop.
- Unchanged: #87 (its `cg-setup`, MSI and bundle halves remain).
- Open at close: **55**.

## Waiting on Manuel

One question, not blocking: **the platform's version floor.** §Scope says
"Windows x86-64" and the matrix names Windows 11, 10 and Server 2019; code 20
refuses another architecture but no Windows version, and `IsWow64Process2`
makes `cg-setup` load only on Windows 10 1709 (build 16299) or later, so an
older Windows 10 or Server 2016 fails to start it instead of exiting 20.
Options: (a) a floor of build 17763 (Windows 10 1809 / Server 2019), code 20
below it, the recommendation, since Server 2019 is the oldest platform the
matrix tests; (b) a floor of 16299, the binary's own; (c) no floor. It is
product scope, so it is Manuel's.

The release-tag freeze stays.

## How Session 61 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit; no PR; 55 open debts; 22 ADRs / 23 SPECs. Check memory before a
   full gate.
2. Ask Manuel's answer on the version floor, if he has not given it; it
   lands with C5's platform check.
3. C5, one commit per push, each through a PR, contract tests first: the
   staging (`<P>` and the data root created, the staged files created new
   with their security, the record), the conflicting service (26), the
   downgrade from the installed binary (29), the stop proved within 75 s
   (31), the reversal, the SCM client, the observer and the verbs of the
   `cg-setup` executable, at the release version; an SCM test of code 10 on
   the adoption route with the real lock; the real `cg-setup` staging with no
   privilege; hosted tests with a probe PR. Its SPEC-024 amendment settles
   #151 first.
4. Then C6 to C10 as `docs/handoff-session-59.md` orders them.
5. Keep the release freeze.

## Environment and memory

- With Docker and the k8s stack down, 7-8 GB stayed free through each gate;
  no gate was killed.
- `C:` had about 24 GB free at the start; three full test builds ran
  without a clean.
