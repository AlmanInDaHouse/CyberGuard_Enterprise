# Handoff — Session 83

Session 83, 2026-11-19, began `cg-setup custody` (SPEC-024 Amendment 2026-11-19, on `main` since
`b2fbb1a`): C1, the product's names in one module of `cg-agent`, and C2, the verb's handle
primitives in `cg-setup`, landed through PRs #149 and #150. The fresh-context reading of the
amendment's points 2 to 7, which round 11 had not had, found five list-A points, three of which change what
C3 classifies; they are Manuel's (the verb is on his list), so the session stopped before C3 and
records them as debt #169. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `1e143c3`, clean, no stash; `docs/spec-024-amendment-2026-11-18`
  (`1f06f31`) and `docs/c10-migration` (`1cf1b12`) pushed with no PR, their worktrees
  `C:\Users\manul\cg-s82-am` and `C:\Users\manul\cg-s80-c4`; no PR open; the tip's CI ALL GREEN
  (markdown-lint); 22 ADRs / 23 SPECs, 58 open debts (58 rows), no Known CI debt. The repo agreed
  with Session 82's handoff. `C:\tmp\s82-cleanup.log`: `elevated: yes`, both trees `left: False`.
  3.2 GB of 15.8 GB of memory free, 19 GB on `C:`; Docker Desktop down (Manuel's `aura_*`
  containers not touched).
- Resulting `main`: `6af8fb4` (C1), `d0cfa95` (C2), this close's commit.
- Branches: `docs/spec-024-amendment-2026-11-18` (`1f06f31`) and `docs/c10-migration`
  (`1cf1b12`) unchanged, pushed with no PR, as Session 82 left them.
- Catalogs unchanged: 22 ADRs / 23 SPECs.

## The commits

1. **C1, `refactor(agent): the product's names in one module`** (PR #149, branch commit `adaccd2`,
   on `main` as `6af8fb4`). `agent/cg-agent/src/service/names.rs` holds every product name and the
   lists the migration, the purge, the layout reading, the adoption, the file log, the installed
   paths, the rotation's names and the setup's verbs and record now take from it; `same`/`same_str`
   compare by UTF-16 units and ASCII case (point 4), the service's callers with case-sensitive
   lookup off, as `eq_ignore_ascii_case` did. No behaviour change. Test
   `agent/cg-agent/tests/install_ac_017_names.rs` (7 tests): the lists pinned to the literals the
   service held, the comparison against `eq_ignore_ascii_case` over a corpus, hostile names, and a
   static reading of nine service and setup modules for a name spelled outside the module.
   `deploy/docker/test/install.test.mjs` reads the purge's lists through `names.rs` (see the
   reactive corrections).
2. **C2, `feat(setup): the custody verb's handle primitives`** (PR #150, branch commit
   `aedc6f1`, on `main` as `d0cfa95`). `agent/cg-setup/src/custody/nt.rs`: one `NtCreateFile` call,
   always `FILE_OPEN_REPARSE_POINT | FILE_COMPLETE_IF_OPLOCKED`, the caller choosing only the kind
   (no open by file ID, backup intent or delete-on-close), one-component names relative to a held
   parent, roots by `\GLOBAL??\` drive paths alone; `hold` (a plain directory, `HOLD`, sharing no
   deletion), `list` (`FileIdBothDirectoryInformation`, bounded by `io.Information`, one listing
   per handle, refused on a handle without `SYNCHRONIZE`), `info`, `volume`, `names`
   (`FileHardLinkInformation`), `move_into` (never over a name), `dispose` (POSIX, no fallback),
   `create_directory` (security at creation), `set_security` (refuses an absent or NULL DACL) and
   `set_owner`, each under a bounded worker of its own (`Failure::Unfinished` for a cancelled call or
   an oplock, `Failure::Internal` for the verb's own fault). cg-agent's `custody` exposes `Handle`
   from an `OwnedHandle`, `AsRawHandle`, `Descriptor` with `from_sddl`/`as_ptr`, `security_of` and
   `last_error`; cg-setup takes windows-sys's `Wdk_Foundation`, `Wdk_Storage_FileSystem` and
   `Win32_System_IO` (ntdll; no crate added, `Cargo.lock` unchanged). Test
   `agent/cg-setup/tests/install_ac_017_primitives.rs` (18 tests): prim_01 to prim_08, refusals,
   owner and security, POSIX deletion and another name's deletion, a directory with a file open
   beneath it, a batch oplock refused at once, the creation, the NT names, and a static reading.

## Decisions

### Manuel's

- #169, 2026-11-19, in chat, to the question whether Session 84 drafts a SPEC-024 amendment for A1
  to A4 on an owner-review branch, reviewed with the fixed prompt and a security lens, and gives
  him the compare URL: "digo que si". Read as: the amendment is drafted in Session 84 as that
  question states it; its text, and how each of A1 to A4 is settled, wait for his OK on the
  compare URL. It is not an approval of any text.

### Anticipated (Claude Code)

- **C1 holds the names alone**: the design's §9 put `custody`'s visibility changes in C1 too;
  they went into C2, where they are first used, so that C1 is the names and nothing else.
- **C1 converts every service module that spells a product name** (adoption, file log, rotation,
  installed paths) and the setup's verbs and record, beyond the three the amendment names, so that
  the static test can read "one module" literally; discarded leaving them.
- **C2's handles are `Arc`-shared with their bounded worker**, which keeps an abandoned call's
  handle open until its thread ends; discarded passing raw handles, which a timed-out caller could
  close under the worker.
- **C2's `within` is its own**, not `custody::bounded`, so that a timeout and the verb's own fault
  are told apart (review 1).
- **Red runs**: C1's two source tests failed with the callers unchanged (`C:\tmp\s83-c1-red.txt`);
  C2's eleven first tests failed against a stub of the module's API, each at the primitive, after
  its fixtures (`C:\tmp\s83-c2-red.txt`). Local mutants (`C:\tmp\s83-c2-mutants.txt`): no
  `FILE_OPEN_REPARSE_POINT` (static_01, prim_01, prim_04 red), no reparse check in `hold` (prim_01
  red), no oplock branch (prim_oplock red).

### Reactive corrections

- **deploy-ci red on C1's first SHA `260c5fe`** (run 44553577241, "The four images and the
  bundle"): `install.test.mjs:200` read the purge's lists as string arrays in `purge.rs`, which C1
  moved to `names.rs` (`TypeError: Cannot read properties of null (reading '1')`). The local gate
  had not run the *bundle and runbooks* bullet, though `agent/` is in deploy-ci's filter. Fixed in
  C1 itself (amended, `--force-with-lease`, before landing): the test reads each purge list through
  its `names::` constant; the local bullet then ran on C1 and C2 (61 tests, 57 passed, 4 skipped).
- **A static pattern caught `FILE_DISPOSITION_INFORMATION`'s field `DeleteFile`**: narrowed to the
  calls `DeleteFileW` and `DeleteFile(`.
- **Clippy's `manual_is_multiple_of`** on C2's bounds checks, fixed before the push.
- **rust-ci's Windows job red on C2's first SHA `afdea5e`** (run 44572734064, `cargo test`): three
  tests compared a DACL's SDDL text with the current user's full SID, which the runner's built-in
  Administrator (RID 500) renders as `LA` (`left: "D:P(A;OICI;FA;;;LA)"`), the trap CLAUDE.md's
  *Windows CI job* names (S32). The expected DACL is now rendered by the same conversion
  (`expected_dacl`); fixed in C2 itself, pushed again with `--force-with-lease` as `aedc6f1`.
- **C2's first review** (below) found the defects its fixes list; the fixes went into C2 before its
  first push.

## The reviews

Each reviewer in its own isolated worktree, given no summary; returns in
`C:\tmp\s83-review-c1.jsonl` and `C:\tmp\s83-review-c2.jsonl` (and the second C2 pass's journal).

### C1: Claude Code's pass

| Criterion | Test | Verdict |
|---|---|---|
| Point 4: names in one module the migration, purge and layout use | `install_ac_017_names.rs` `one_module`, `compared_in_one_place`, `lists_are_the_services` | covered |
| Point 4: UTF-16 units, ASCII case when lookup is off | `compare_utf16_units`, `product_name` | covered |
| *Effect*: the service's comparison moves, behaviour unchanged | `keep_the_services_comparison`; the service's suites (`cargo test --all`) | covered |

### C1: the fixed prompt (code)

Verdicts: one module weak, use by the three callers weak, UTF-16 covered, ASCII case weak, *Effect*
covered; no *Data contracts* statement contradicted. Dispositions: the per-directory and per-layout
table and each name's kind belong in `names.rs` when the verb classifies (C3); the duplicate check
`seen.insert(entry.to_ascii_lowercase())` in `migration.rs` stays (equivalent, the *Effect*);
every service caller passes case-sensitive lookup off, as before (the *Effect*); the module is
portable and its unused public items are the verb's. None blocks.

### SPEC-024 Amendment 2026-11-19, points 2 to 7: the fixed prompt (text)

Round 11's fixes had no reviewer; this reading had none of the session's reasoning. List A:

- **A1**: point 4's fallback for a staging or legacy directory Administrators cannot read
  (SPEC-024:3699-3703) makes a planted, unreadable `agent.migrating` the agent's once item 2.1 has
  closed `<P>`, so `in-place` never makes item 2.2's move (3712-3713, 3777); a real staging
  directory is always readable by Administrators (item 2.5).
- **A2**: point 6 names the setup's 27 on a console installation (3712), but a console
  installation whose agent never enrolled holds no identity file, so point 4 (3692-3694) gives its
  data root the service's layout, and with the service absent no mode takes it.
- **A3**: an `identity` or `logs` whose owner Administrators cannot read, planted in a data root an
  ordinary user could write, is taken for the service's (3680-3684): `in-place` leaves it, `aside`
  refuses (3732-3734), the purge leaves it with 47; no route removes it.
- **A4**: point 5's link count "for each entry it lists" (3706-3707) needs an open of `identity`
  and `logs` beyond reading their owner (3683).
- **A5**: a copy of `cg-setup.exe` on a host whose policy makes the administrator's account the
  owner of what it creates fails point 2's owner check (3637-3639).

Dispositions: A1 to A4 change what the verb classifies and are Manuel's (the amendment's *Effect*
puts the verb on his list): **debt #169**, a contract amendment before C3. A5 needs no contract
change: the runbook (C8) creates the copy owned by Administrators, as it already creates the
inputs' directory (`install.md:327-331`). List B (17 points) is for C3 to C8; its notable points:
the plan binds nothing of a `<P>` that is a link or unlistable (B3); a lookup turned off fails
with case-colliding entries (B4); another name of a read-only file cannot be deleted (B5); an
ordinary user's handle beneath a closed `<P>` makes `aside` 48 or 49 (B6, V5); `aside` hashes an
`agent.toml` that may hold the token (B7); the refusal list omits two temporaries (B8); a log
written into the inputs' directory changes its listing between dry run and act (B9); the NTFS
volume of an absent `<P>` (B10); the final-path comparison rule (B11); the lock's other outcomes
(B12); NTFS's upcase table against ASCII folding (B14); the setup's 27 for a legacy directory
beside a data root with no `identity` (B15); `agent.toml` given owner Administrators on an enrolled
host still refused with code 2 (B16).

### C2: Claude Code's pass

| Criterion (points 2 and 3, *Tests*) | Test | Verdict |
|---|---|---|
| `DELETE`+`FILE_READ_ATTRIBUTES` granted by the parent on a file, read-only file, hard link, junction, directory admitting no one | prim_02, prim_03, prim_01 | covered |
| Never following a reparse point, never backup intent, never by file ID | prim_01 (hold refuses a junction), prim_refusals, static_01, mutants | covered |
| Rename never over a name, descriptor, attributes, contents kept, a junction as a link | prim_03, prim_01 | covered |
| Deletion of a file, a junction, another name; read-only and non-empty refused; POSIX | prim_04, prim_dispose_names | covered |
| A file's names with parents' IDs through `FILE_READ_ATTRIBUTES` | prim_07 | covered |
| A hold refuses deletion and rename; refused while another may delete | prim_08 | covered |
| A directory with a file open beneath it is not moved | prim_open_beneath | covered |
| A close by handle reaches no other object | prim_05 (exact DACL, canary, unlistable child, contrast) | covered |
| Owner and DACL written through the handle; no NULL DACL | prim_owner_and_security | covered |
| An oplock refused at once as in use | prim_oplock | covered |
| A call cancelled after 30 s | none (the worker's cancellation is `custody::bounded`'s, tested there) | weak |
| Hostile names acted on exactly | prim_06 | covered; their printing is C3's |

### C2: review 1 (fixed prompt and security lens, on `0d1f6b7`)

Blocking (security lens): no test refused a junction held as a directory. Fixed in C2: that test,
and the defects found with it: `list` on a handle without `SYNCHRONIZE` (a pending query into a
freed buffer); two listings sharing a handle's position; `set_security` writing a NULL DACL or
dropping an owner; caller options unfiltered; names of more than one component; parsers unbounded
by `io.Information`; `nt_name` mapping shares, devices and relative paths, and `\??\`; a silent
non-POSIX fallback in `dispose`; a panic reported as in use; and the tests the fixed prompt found
weak or missing (owner writes, another name's deletion, a file open beneath a directory, the
oplock, the exact DACLs, a test reset that followed a junction). Left to the verb, recorded for
C3/C4: an act whose call is `Unfinished` may have acted (49, never 48), and its `Object` is not
reused; a directory closed through a handle is read again before a verdict (V1: a writer's handle
opened before the close can turn an empty directory into a junction); a file's names are checked
before its security is written (point 3); `set_owner` is given Administrators alone, and
`create_directory` the fixed DACL; the volume's identity for point 2 (the 32-bit serial) and the
final path are C3's.

### C2: review 2 (fixed prompt and security lens, on `ac5d4d1`)

No blocking point; no *Data contracts* statement contradicted. Fixed in C2: the comment that a
hold keeps its entries' names (false: only the parent's closed DACL does, and a handle opened
before the close keeps its access, V1); `names()` refusing a file with no name left; the
creation's and `set_security`'s SDDL both required to give a present, non-NULL, protected DACL;
`Failure::Oplocked` apart from `Unfinished`; `STATUS_DELETE_PENDING` read as in use; the pending
paths with their memory on the heap and never freed; `list()`'s structure bounded by the buffer;
the module's comments on intermediate components and case. Tests added: a NULL and an unprotected
DACL refused, the link's own DACL read through its handle, the static test's exact line, a listing
of several buffers and a file of 41 names. Left, recorded: no test of the 30-second cancellation
(the worker's, as `custody::bounded`'s); the final path, the volume's identity and a remote volume
are C3's inputs check; the moved junction's reparse data and other reparse kinds (symbolic links,
AF_UNIX sockets) are not exercised; the owner given at creation is the caller's SDDL.

### C2: review 3 (security lens on the fix delta, `ac5d4d1` → `38b78f6`)

No blocking point. Fixed: the NULL-DACL check given a test of its own (`D:PNO_ACCESS_CONTROL`;
mutant m-null); `Oplocked`'s comment (the open succeeded and was closed; a creation that meets an
oplock is `Unfinished`); a pending request mapped to `Unfinished`; `volume()`'s queries on the heap;
the test of a file's 41 names comparing the names; `in_use()` asserted for `Oplocked`, `Unfinished`
and `STATUS_DELETE_PENDING`. Left: `checked_descriptor` does not refuse owner-rights, creator or
inheritable entries, nor a missing owner (the caller's SDDL is fixed by point 2).

## Gates and CI

- C1 (`adaccd2`), local: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets
  --all-features -- -D warnings` clean; `cargo test --all` 624 passed, 0 failed, 11 ignored;
  `agent-version-matrix.mjs` "agent version matrix: all steps passed"; deploy bullet 61 tests, 57
  passed, 4 skipped; licences "licences admitted: 431 packages".
- C2 (`ac5d4d1`), local: fmt and clippy clean; `cargo test --all` 640 passed, 0 failed, 11
  ignored (before the lint-only change, after which `install_ac_017_primitives` passed 16/16);
  version matrix all steps passed; deploy bullet 61, 57 passed, 4 skipped.
- CI, each SHA pushed alone:
  - `260c5fe` (C1's first, PR #149): deploy-ci failure (run 44553577241), ts-ci success; rust-ci
    and package-ci superseded by the force-push. Fixed in `adaccd2`.
  - `adaccd2` (PR #149): deploy-ci, package-ci, rust-ci, ts-ci success. ALL GREEN.
  - `6af8fb4` (`main`, C1): rust-ci, package-ci, ts-ci success. ALL GREEN.
  - `afdea5e` (C2's first, PR #150): rust-ci failure (run 44572734064, the Windows job's
    `cargo test`); deploy-ci, ts-ci, dependency-audit success. Fixed in `aedc6f1`.
  - `aedc6f1` (PR #150): dependency-audit, deploy-ci, package-ci, rust-ci, ts-ci success. ALL GREEN.
  - `d0cfa95` (`main`, C2): dependency-audit, package-ci, rust-ci, ts-ci success. ALL GREEN.
  - This close's commit: its post-push report.
- No elevated gate: `service/`, `custody.rs` and `cg-setup` are outside the paths of the marquee
  validations' §5 and §6; C1 changes no behaviour.

## Test baselines at close

`cargo test --all` unelevated on this host: 642 passed, 0 failed, 11 ignored.

## Debts

- Opened: **#169**, SPEC-024 Amendment 2026-11-19's points A1 to A4 of Session 83's reading
  (above): an amendment, Manuel's, before C3.
- Closed: none.
- Open at close: **59**.

## Waiting on Manuel

- #169: his OK on the compare URL of the amendment Session 84 drafts (he chose that route on
  2026-11-19), which C3 needs.

## How Session 84 resumes

1. Run the start protocol. `main` is this close's commit; the two docs branches unchanged; 59 open
   debts; 22 ADRs / 23 SPECs.
2. Draft SPEC-024's amendment for A1 to A4 on an owner-review branch (`NOT YET RATIFIED`), as
   Manuel chose on 2026-11-19 (#169), the fixed prompt and a security lens on it, then the compare
   URL.
3. After his OK, C3 as `C:\tmp\s82-verb-design.md` §9 orders it, with this handoff's C2 caller
   duties and review list B: `names.rs` gains the per-directory and per-layout table with each
   name's kind; the inputs' check (final path, volume identity, `GetModuleFileNameW`), the lock, the
   SCM state, the downgrade, the token line, show and the dry runs, every act 48; `sha2` at
   cg-agent's requirement. Then probe A, C4 to C9 as Session 82's handoff lists them.
