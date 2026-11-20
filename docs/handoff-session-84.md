# Handoff — Session 84

Session 84, 2026-11-19 to 2026-11-20, settled debt #169: it drafted SPEC-024 Amendment
2026-11-19 (second), the custody verb's readings of four states, on an owner-review branch,
reviewed it in eleven rounds of the fixed prompt for a text and a security lens and two closing
rounds of the fixed prompt alone, and landed it on Manuel's approval (`0a3c64a`). No code changed;
C3 did not start (*How Session 85 resumes*). No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `3423035`, clean, no stash; `docs/spec-024-amendment-2026-11-18`
  (`1f06f31`) and `docs/c10-migration` (`1cf1b12`) pushed with no PR, their worktrees
  `C:\Users\manul\cg-s82-am` and `C:\Users\manul\cg-s80-c4`; no PR open; the tip's CI ALL GREEN
  (markdown-lint); 22 ADRs / 23 SPECs, 59 open debts (59 rows), no Known CI debt. The repo agreed
  with Session 83's handoff. 3.3 GB of 15.8 GB of memory free, 14.9 GB on `C:`; Docker Desktop
  down (Manuel's `aura_*` containers not touched).
- Resulting `main`: `0a3c64a` (the amendment), this close's commit.
- Branches: `docs/spec-024-amendment-2026-11-18` (`1f06f31`) and `docs/c10-migration` (`1cf1b12`)
  unchanged, pushed with no PR. The owner-review branch `docs/spec-024-amendment-2026-11-19-second`
  (`3fd1f51`) landed by `git merge --squash` with `git diff --cached --quiet` as the guard (0), and
  was deleted, local and on origin.
- Catalogs: 22 ADRs / 23 SPECs; `docs/specs/README.md` gains the amendment's line.

## The commit

**`docs(spec): SPEC-024 Amendment 2026-11-19 (second), the custody verb's readings of four states
(#169)`** (`0a3c64a`, straight to `main` after Manuel's OK, as *Integration path* §1 and §4 give
it). It reads the Amendment 2026-11-19's points 2 to 7 and 12 anew (§1 to §4 of the new text):

- §1, seven readings in an order with no cycle: a *SYSTEM mark* (an `agent.toml` of any kind, or an
  `agent.toml.setup` or `agent.toml.hygiene-tmp` owned so by SYSTEM, each with a protected DACL); a
  `logs` is the service's only as a directory owned by SYSTEM or Administrators whose DACL the file
  log admits; a `cert.pem`, `key.dat` or `identity.json` beside a SYSTEM mark and a SYSTEM `logs`
  that is not owned so by SYSTEM with one link is foreign; a staging directory Administrators cannot
  read is foreign, a legacy directory so only beside a `migration.json` SYSTEM owns, in a closed
  `<P>`; an `identity` is the service's only beside a SYSTEM mark or a legacy directory taken for
  the agent's; the setup's and migration's six names are foreign unless owned so by SYSTEM or
  Administrators, another link repaired as before and a temporary item 2.2 refuses deleted (moved
  when foreign).
- §2, A2: the first sentences of points 6 and 7 name the runbook's routes, not conditions; the
  service conditions stand; `in-place` with the service absent also takes a data root of the
  service's layout with a SYSTEM mark (a host uninstalled without the purge); point 7's refusals
  hold only under a `<P>` SYSTEM or Administrators owns, and refuse a configuration only when it
  passes `agent.toml`'s policy, so `aside` takes the setup's 27 (or 28 with a planted
  `agent.migrating`) on a console installation whose agent never enrolled. The residual: an object
  SYSTEM or Administrators owns, moved or linked in by another account or made there by an
  administrator, which the verb cannot tell from the product's, with its three effects.
- §3, A3: the purge's leftover `identity` is `in-place`'s, a service's `identity\` the purge had
  not removed kept in the inputs' directory with its sealed key; the runbook runs `in-place` there
  only after the purge run again leaves it.
- §4, A4: the link count of an `identity` taken for the service's whose owner Administrators cannot
  read is printed as not read.
- *Alternatives*, *Tests* (install_ac_017 gains them), *Effect* (the threat model's Amendment
  2026-11-18 on its branch is revised to §1 to §3 before it lands, for Manuel's approval; the
  runbooks' purge statements are revised with the runbook).

## Decisions

### Manuel's

- 2026-11-20, in chat, on the compare URL at `0610039`: "doy mi ok y acepto recomendaciones". Read
  and stated back in chat as: the text is approved as it stands; each point A1 to A4 takes its
  proposal over its alternative; the two readings beyond A1 to A4 (`in-place` for a host
  uninstalled without the purge, the console's names beside a SYSTEM mark) and the residuals §2 and
  §3 disclose are accepted; the threat model's revision is not approved by it and comes back with
  the runbook. The approval sentence in the amendment records it.

### Anticipated (Claude Code)

- **A1**: a staging directory Administrators cannot read is foreign (every staging directory of the
  product's gives Administrators full access, `migration.rs:1397`); discarded: refusing it, which
  enrolls a host anew over a planted name.
- **A2**: `aside` takes a console installation whose agent never enrolled; discarded: `in-place`,
  which keeps in place a configuration whose token every account could read and which the service
  refuses anyway.
- **A3**: `in-place` moves the purge's leftover `identity`; discarded: `aside` after a default
  uninstallation, which keeps everything the purge would erase.
- **A4**: the link count printed as not read; discarded: an open of `identity\` beyond point 3's.
- **Owner review on one squashed commit** (`0610039`), so that Manuel read one diff; the branch's
  thirteen review commits were squashed before the push.

### Reactive corrections

Each round's findings changed the design; the load-bearing ones:

- **Round 1** broke the owner-only readings (hard links to SYSTEM-owned files, the other four names
  closed to Administrators, the purge's promise): the readings moved to the service's
  configuration as the gate.
- **Round 2** showed a link-count gate on the configuration lets a hard link or an administrator's
  grant move an enrolled host's `identity\`: the gate became the SYSTEM mark (owner SYSTEM), which
  no ordinary user can create or remove; refusals it can be forged into were disclosed instead.
- **Round 3**: the mark needs a protected DACL (a console agent run as LocalSystem leaves an
  inherited one); "unenrolled console configuration" first defined, later (round 5) dropped as a
  verb condition.
- **Round 6** found a cycle (identity ← legacy ← layout ← identity): the readings got an order.
- **Rounds 4 to 6**: the console-names guard went from "beside a service `identity`" to "beside a
  SYSTEM mark and a `logs` SYSTEM owns", which also covers a pending host and leaves a console
  installation item 2.4 left part-restricted a console installation.
- **Round 8** found my own regression: one link required of the product's records made a planted
  second link move `migration.json` and, by cascade, a kept legacy directory; reverted to ownership,
  another link repaired as before. **Round 10** found a second one: a temporary refused for its
  links alone had lost its deletion; restored.
- **Round 9**: point numbering clashed between the two amendments: the new text's points are §1 to
  §4, and the threat model's own §1 is named so.
- A `node -e` edit script with backslashes failed inside a `&&` chain whose later commands ran and
  committed the round's text without it (local only, never pushed); amended before the review. The
  later scripts were written with Write and run alone.

## The reviews

Each reviewer in its own isolated worktree, given no summary; the journals are under the session's
`subagents/workflows/` directory (`wf_7cf3b27e-676` to `wf_c524df5c-cd6`). The security lens's
prompt, beside the fixed prompt, read the amendment as an ordinary local user who planted names,
links, transactions and open handles, and as an auditor of data retention.

### Claude Code's pass (text)

Each cited `path:line` was opened: `migration.rs:1397` (staging, `PRIVATE_DIRECTORY`),
`log_file.rs:31` and `:33`, `migration.rs:1712-1721` with `identity.rs:483` (the legacy directory
to SYSTEM alone), `install.md:7`, `:303` and `:363`, the Amendment 2026-11-09's adoption steps 3 and
5, the Amendment 2026-11-04's items 3 and 6, the second Amendment 2026-11-01's item 1, the purge's
order (the Amendment 2026-11-15, point 6). Claims made twice (the *Effect*'s threat-model list
against §1 to §3; the catalog line against the section) were checked against each other after each
round.

### The fixed prompt, last two rounds

At `4141d9a`: three list-A points (the temporary's precedence between two rules; `legacy`'s
leftover reaching §3's state by a product route; the *Effect*'s bullet not following §1), fixed in
`525a1f9`. At `525a1f9`: **list A empty**. Its list B, recorded for C3 to C8 (none blocks):

1. The catalog line omitted the legacy refusal (fixed before the push).
2. "which a host uninstalled without the purge keeps" is broad: a purge that ended at its later
   steps, then a default uninstallation, leaves `logs\` with no SYSTEM mark, no `in-place` and an
   `aside` refused by that `logs`, if an administrator opened the directories.
3. §3's "no other route" has one more: `in-place` deleting a two-link `agent.toml.hygiene-tmp` that
   was the only mark (only an administrator or SYSTEM makes the second link).
4. The definitions paragraph's repair names files owned so by SYSTEM or Administrators; whether it
   narrows the Amendment 2026-11-18, point 4's repair of a file another account owns is unclear.
5. The console-names guard fails for a pending host whose file log never opened, and where an
   administrator took ownership of `logs\`.
6. The residual list's effects omit a moved-in SYSTEM `agent.toml.hygiene-tmp` keeping a planted
   `identity` the service's (covered by the general sentence).
7. Effect 3 of the residual is looser than §1 (no DACL condition on `logs`, no one-link condition).
8. An `agent.toml` of any kind is a mark and also foreign by kind (a residual case).
9. The list of who makes a configuration file SYSTEM's omits item 2.4.
10. Point 11's "takes that directory's security" against the runbook's copy owned by Administrators
    (an owner is not inherited; C8's wording).
11. The A4 alternative's mention of `logs` is moot (its owner is always readable).
12. The `package-ci` fixture's "the runner's Administrator's" owner: RID 500 or the group.
13. Not checkable from the repo: `FILE_READ_ATTRIBUTES` from the parent under a deny entry on the
    child; a `READ_CONTROL|FILE_READ_ATTRIBUTES` open denied as a whole; an ordinary user's hard
    link to a SYSTEM-only file; a deny or `OWNER RIGHTS` DACL blocking SYSTEM.
14. The approval record (added at landing).

### The security lens

It never ran dry, as on the Amendment 2026-11-05 (Session 56): its last list A (round 11) held a
preserved host left without a route (fixed: `in-place` with a SYSTEM mark), a hard link to the
administrator's own file in a never-enrolled data root (within the residual "linked there"; the
sentence that denied it fixed), a `logs` whose DACL the file log refuses (fixed: the DACL rule), and
the routes-versus-conditions reading (fixed: the service conditions stand). Its remaining points
for the code, from rounds 7 to 11:

- Map "cannot read" from the NTSTATUS, not Win32: `STATUS_DELETE_PENDING` maps to
  `ERROR_ACCESS_DENIED` and is "in use".
- The open that reads an owner is by the listed name relative to the held parent; a denied open has
  no handle to compare file IDs with, which binds only in a closed directory.
- The unelevated stand-in for "Administrators cannot read" (an `OWNER RIGHTS` entry granting
  nothing) is unverified; so is the zero-mask entry surviving the security APIs.
- Whether a standard user can hard-link a file it cannot write, on builds 17763 and 26200, decides
  the reach of the residual.
- A case-sensitive data root (`identity` and `Identity`); the mark compared as point 4 compares.
- Users keep create rights inside a never-enrolled `<P>` moved whole into the inputs' directory.
- A planter's oplock, pending deletion or open handle forces 48 or 49; only the runbook's restart
  bounds it.
- The Amendment 2026-11-19, point 12's citation `migration.rs:79-87` now points at the `Choice`
  enum; the repair lines are at `:69-76` (anchor drift since `6af8fb4`, left for a Class-A sweep).

## Gates and CI

- No code changed: no Rust, TypeScript or deploy gate ran; no elevated gate.
- markdownlint, locally, on the two files, each round and before each push: "Summary: 0 issues in 0
  files".
- Pushes:
  - `0610039`, then `3fd1f51` (the approval record), to `docs/spec-024-amendment-2026-11-19-second`
    with no PR open: no workflow ran ("runs: 0").
  - `0a3c64a` (`main`): markdown-lint → success. ALL GREEN.
  - This close's commit: its post-push report.

## Test baselines at close

Unchanged from Session 83 (no code): `cargo test --all` unelevated on this host, 642 passed, 0
failed, 11 ignored.

## Debts

- Closed: **#169**, by `0a3c64a` (the amendment).
- Opened: none. The fixed prompt's list B and the security lens's points above are C3 to C8's inputs,
  and the residuals are the amendment's own disclosures.
- Open at close: **58**.

## Waiting on Manuel

- Nothing now. The threat model's Amendment 2026-11-18 (`1f06f31`) comes back to him, revised to
  this amendment's §1 to §3, with the runbook (C8).

## How Session 85 resumes

1. Run the start protocol. `main` is this close's commit; the two docs branches unchanged; 58 open
   debts; 22 ADRs / 23 SPECs. Check memory and `C:` before builds (`CARGO_PROFILE_*_DEBUG=0` if
   under 15 GB).
2. Re-derive the classification for C3 before code, in `C:\tmp\s85-c3-design.md`:
   `C:\tmp\s82-verb-design.md` §2.3 predates both amendments' readings. Map SPEC-024 Amendment
   2026-11-19 (second) §1's seven readings, in their order, onto `names.rs`'s per-directory and
   per-layout table with each name's kind; the verdicts (the agent's, the service's, foreign,
   twice); the owner-and-DACL open (`READ_CONTROL|FILE_READ_ATTRIBUTES`, by the listed name
   relative to the held parent, file ID compared) and its NTSTATUS mapping (access denied: cannot
   read; delete pending, an oplock, the 30 seconds: in use, 48/49); point 7's refusals only under
   a `<P>` SYSTEM or Administrators owns; `in-place`'s service-absent condition with a SYSTEM mark;
   §4's link count. Fold in this handoff's list B and the lens's points.
3. C3, `feat(setup): cg-setup custody, the reading`, as `C:\tmp\s82-verb-design.md` §9 orders it:
   `cli.rs` `Command::Custody { mode, plan }` and its 40s; `codes.rs` 48 and 49; `main.rs`; the
   modules for the inputs' check (final path, volume identity, `GetModuleFileNameW`), the lock,
   the SCM state, the downgrade (29), the token's privilege line, show and the dry runs and the plan
   (SHA-256 at `sha2`, cg-agent's requirement), every act 48; tests red first (cli, class, escape,
   plan, inputs). The Rust gate (fmt, clippy, test, version matrix) and the deploy bullet; the
   self-review in two halves and a fresh security pass after its fixes.
4. Probe A on `package-ci` (the host job running the real verb, seen red, closed without landing).
   Then C4 to C9 as Session 82's handoff lists them.
