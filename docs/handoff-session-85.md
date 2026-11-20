# Handoff — Session 85

Session 85, 2026-11-20, took Manuel's statement of what the product is for (an MVP that works on
client companies' computers once installed with `setup.exe`, validated on machines that simulate
them, then sold; no client ever has a console installation) through a read-only investigation, four
owner decisions and an owner-review text: SPEC-024 Amendment 2026-11-20, the console installation is a
development layout, with the threat model's Amendment 2026-11-20 (SPEC-024), the roadmap's remaining
order to sale and the register, reviewed in twelve rounds and landed on Manuel's approval (`9f34228`).
No code changed; F3's SPEC did not start (*How Session 86 resumes*). No tag was pushed: the release
freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `4de7ef6`, clean, no stash; `docs/spec-024-amendment-2026-11-18`
  (`1f06f31`) and `docs/c10-migration` (`1cf1b12`) pushed with no PR, their worktrees
  `C:\Users\manul\cg-s82-am` and `C:\Users\manul\cg-s80-c4`; no PR open; the tip's CI ALL GREEN
  (markdown-lint); 22 ADRs / 23 SPECs, 58 open debts (58 rows), no Known CI debt. The repo agreed
  with Session 84's handoff. 3.7 GB of memory free, 15 GB on `C:`; Docker Desktop down.
- Resulting `main`: `9f34228` (the amendment), then this close's commit.
- Branches: `docs/spec-024-amendment-2026-11-18` (`1f06f31`) and `docs/c10-migration` (`1cf1b12`)
  kept, pushed with no PR, with their worktrees: the first is superseded by `9f34228` and the second
  is not landed, but `51ba811` and `2c9f74e` on it hold #167's fixes for every installation, which a
  change of their own carries first (*How Session 86 resumes*, step 3); both are deleted, local and on
  origin, once that change lands. The owner-review branch `docs/console-unsupported` (`fe29e57`)
  landed by `git merge --squash` with `git diff --cached --quiet` as the guard (0), and was deleted,
  local and on origin.
- Catalogs: 22 ADRs / 23 SPECs; `docs/specs/README.md` gains the amendment's line and the edge
  SPEC-024 → SPEC-002 (by scope).
- The investigation's nine reports are under `C:\tmp\s85-research\` (outside the repo, for Session
  86's convenience; this handoff is the record).

## The investigation

A workflow of nine read-only agents (`wf_5b20b909-af7`): SPEC-024's console clauses; the threat
model and CLAUDE.md; the runbooks and the deploy tests that read them, with `1cf1b12`; the code and
its tests; the 58 debts in two halves; the custody verb reduced to the setup's hosts; the MVP's
remainder and an enterprise buyer's gaps; and a completeness critic, which checked the load-bearing
claims and that the two debt tables' union was exactly the 58 open rows. What changed the framing:

- The console's names reach a host the setup installed: a `%ProgramData%\CyberGuard` an ordinary
  user created first, holding a directory `agent.migrating` or a `cert.pem` in `agent\`, is the
  setup's 28, not 27 (`agent/cg-agent/src/service/layout.rs:150-158`).
- No code is a risk worth deleting: the only route to the migration's code as SYSTEM needs an
  elevated operator setting `CG_MIGRATE_LEGACY` and `CG_TRUST_ANCHORS`, which the 28's line offers
  (`agent/cg-setup/src/wizard.rs:75-83`).
- R1 closes by scope for its title; its residuals need a home (machine-scope DPAPI copies, clones,
  UAC); no ADR decides the migration.
- deploy_ac_020 runs from the published bundle, after the release (`SPEC-023:334`); the validation
  on the test machines uses the release's dry-run setup before it.
- Host states no document covered: a policy that re-applies `%ProgramData%`'s ACLs, a changed service
  account, a pre-created `C:\CyberGuardSetup` (`install.md:336`), WDAC on unsigned binaries, and an
  enrolled endpoint imaged or cloned (R18).

The debts by category, at the start (58): console-only #87, with console parts in #143, #164, #166
and #167; the verb for clients #144 and #166; F3 #26, #83, #98, #102, #136; F4 #88, #89, #119; the
register's "F4 pre-release" load and delivery items #25, #32, #35, #42, #61, #114; release debts
are #124, #125, #130, #133, #162; test phase #50, #59, #145, #167; threat model alone #164, #168;
post-MVP the rest. #99, #124 and #125 wait for Manuel's acceptance or control, which no delegation gives.

## Decisions

### Manuel's

- 2026-11-20, in the session's prompt: "Ningún cliente tendrá nunca una instalación de consola …
  solo existen en máquinas de desarrollo, que se pueden formatear. Las actualizaciones llegan por el
  upgrade del setup (upgrade.md), sin auto-update".
- 2026-11-20, on D1 to D4 as presented: "sigo las recomendaciones y lo que tu intuyas que es lo mas
  beneficiable para que el proyecto sea lo mejor del mundo, solamente ten en cuenta que todo esto es
  para vender a empresas y que funcione perfectamente". Stated back in chat as: D1, the console
  unsupported, the migration's triggers refused and its code kept, 28 kept and read anew, R1 and #87
  closed by scope; D2, the custody verb reduced (`show`, `in-place`, `aside`), built after F3 and F4
  and before the test machines, its contract restated first; D3, `1f06f31` split (the threat model's
  amendment now, the verb's entries later) and `docs/c10-migration`'s client fixes salvaged; D4, the
  recommended order; and, under the delegation, F3 taking the triage and a read view of `audit_log`.
  The decisions that still need his explicit yes stayed his: F4's departures from ADR-0004 and the
  acceptance of #99, #124 and #125.
- 2026-11-20, on the compare URL at `5198ce6`: "ok". Read as approving the text as it stood,
  covering its three accepted threats, F3's scope and point 7's reading of #87. The branches' deletion
  was deferred by Claude Code in the same turn, until #167's fixes are carried.

### Anticipated (Claude Code)

- **Refuse the triggers, keep the code**; discarded: removing about 1,500 lines of code and 6,700 of
  tests, which no risk requires and which the deploy tests read.
- **Keep 28, read anew**; discarded: folding it into 27, or reordering the setup's table.
- **R18 as an open risk with a debt (#170)**, not an accepted threat; for an enterprise buyer a
  golden image is a common route.
- **The register updated in the landing**, with a rule for an owner-approved landing before the
  session's close; discarded: citing #170 before the register held it.
- **One squashed commit for owner review** (`5198ce6`), as in Session 84.
- **The bundle's variables kept**, refused by the setup; discarded for now: removing them, which
  changes the chain's command lines and the package's tests.

### Reactive corrections

Each round's list A changed the text; the load-bearing ones:

- **Round 1**: install_ac_013's new reading claimed refusals the code does not give; the service now
  also refuses, at a start without arguments, a staging or legacy directory that counts (code 5),
  since an object of SYSTEM's moved in can make one on an endpoint; 28 read anew to cover a variable
  on a fresh host; 47 does change the host; R1's evidence named; SPEC-002 read by scope; antivirus
  quarantine disclosed.
- **Round 2**: the code 5 limited to a start without arguments (it clashed with the purge's and the
  adoption's starts); item 6's reason reworded, not only re-read; `legacy` kept until the verb's
  restatement (it serves a purge's 47); `configure` refuses with 40 (a 28 after the MSI clashed with
  the codes' grouping); machine-scope DPAPI lets any account on the machine unseal a copy it can read,
  and a dump holds the key in clear.
- **Rounds 3 and 4**: R1's closing excepts an administrator's and an administrative privilege's
  unelevated processes (UAC, Backup Operators, impersonation), accepted; R18 widened to an offline
  read of an unencrypted volume, with disk encryption as the deployment's control; the interim names
  the standard user whose setup an administrator elevates; the register's closures and destinations
  moved into the landing.
- **Rounds 5 and 6**: #87 is closed by this text on the landed code, read against "A contract-only
  change closes neither #87 nor #131"; the verb's uses for item 2.2's repairs and item 6's route have
  no subject; the Amendment 2026-11-18's tests superseded; the *New risks* line updated in place; the
  interim bounded by the release freeze; one remedy for a suspected compromise (the purge, then a new
  installation with a new token).
- **Rounds 7 to 11**: the adoption's code 2 stated as the code gives it; the moved-in object, which
  Manuel had approved for the verb in the superseded threat-model text, accepted for the setup, the
  service and the purge, with both of his approvals cited; the runbook's lines stated as what point
  2's change brings; the status lines as decided and pending.
- A `markdownlint … | tail -1 &&` chain hid the linter's failure and committed and pushed an MD018
  error to the owner-review branch (never to `main`); fixed by a new commit before the landing. The
  linter's exit code is now read on its own.

## The reviews

Each reviewer in its own isolated worktree, given no summary; the journals are under the session's
`subagents/workflows/` directory: rounds 1 to 4, the fixed prompt for a text with a security lens
(`wf_d7b9da4e-d57`, `wf_6f0ec0bf-21b`, `wf_9ba72d98-992`, `wf_7279bd18-de3`); rounds 5 to 12, the
fixed prompt alone (`wf_1bc16790-cc8`, `wf_f783e0c1-aa7`, `wf_8ca5e252-829`, `wf_f1f0391f-ad5`,
`wf_bbdd6e0e-06e`, `wf_6d15cce6-b79`, `wf_39443fa5-ce1`, `wf_1b88dbfe-175`). The security lens read
the change as an ordinary local user who plants names, links, transactions and open handles on a
host only the setup installed, and as an enterprise buyer's security lead and DPO.

### Claude Code's pass (text)

Each cited `path:line` was opened: `layout.rs:140-185` (the order of the readings), `wizard.rs:60-125`
(the route and the 28's line), `identity.rs:482-485` (`identity\` SYSTEM's alone),
`secure_storage.rs:80-92` (`CRYPTPROTECT_LOCAL_MACHINE`, no entropy), `migration.rs:55-150` and
`:600-660` (the default refusal, the start arguments, `prepare`'s arms), `adoption.rs:40-64` and
`:232-256`, `verbs.rs:290-320` (the purge's 27 and 28), `codes.rs` (27, 28, 40, 42, 47),
`windows.rs:345-375`, `Bundle.wxs:39-83`, `fdd0ea5^:docs/operations/install.md`, SPEC-002's Amendment
2026-10-27, and SPEC-024's §Scope, §Operational §2 and §4, §Acceptance criteria, §Risks, the Decision
record and the amendments the text cites by item. Claims made twice (the SPEC's *Effect* against the
threat model's amendment and the catalog line; the five lines updated in place against their quotes;
the register's count against its rows) were checked against each other after each round.

### The fixed prompt, last rounds

Round 9 (`649ea34`) and round 12 (`5f3200f`): **list A empty**; rounds 10 and 11 found two and one
small list-A points, fixed. Round 12's list B, recorded for the code and the verb's restatement (none
blocks):

1. Item 6's "It completes a migration only from the first rename on" is covered by point 2's
   "completes, reverses or continues" but not named.
2. The threat model's `:282` (*Context*) still says "run by hand as an elevated administrator, or as a
   Windows service"; `:294` now calls the run by hand a development surface.
3. "No subject on an endpoint" is broader than some sentences of the Amendment 2026-11-05's entries,
   which still hold (the start right, a start without arguments beside a planted name).
4. The R5 shortcut "until it expires or an operator deletes its row" omits that the enrollment
   token's reference blocks that deletion until the token row is cleared.
5. "the runbook discloses them (R1 and R5)" cites the closed R1, as a record pointer.
6. "None is accepted" and §Risks' "No new risk is accepted by this text" sit beside three accepted
   threats: consistent only with "new risk" meaning the R-list.
7. `:312`'s accepted copies "on that machine" against R18's images, which can stay on that machine;
   the accepted bullet carves R18 out, `:312` does not repeat it.
8. R17's destination is a debt (#166) where other entries name a SPEC or a phase.
9. The register's interim entries (amendment-first, then this handoff) depart from its header until
   this close, which adds the handoff.
10. "Four of them run only where CLAUDE.md's control §1 confines them": the confinement is SPEC-024's
    Amendment 2026-11-01, point 4; control §1 says where they are seen red.
11. The *Alternatives*' reliance on `install.test.mjs:193-197` is weak: it reads `migration.rs` for
    the purge's argument line, which removing the route need not touch.
12. Not checkable from the repo: Manuel's words; the cited tests passing in CI; item 2.1's close on an
    administrator-opened `<P>`; the Blueprint's P0 triage rows against "acknowledge, assign, close".
13. Point 3 changes every refusal line that offers the migration, some not named: `migration.rs:139`
    ("--trust-anchor-sha256 is given without --migrate-legacy"), `wizard.rs:85-86`
    (`MIGRATION_IN_PROGRESS`), `verbs.rs:304` ("finish or reverse it first"), the adoption's line.

### The security lens's residuals

It did not run dry, as in Sessions 56 and 84; its points after round 4 that the text discloses or
leaves for later, none blocking:

- Nothing in the product enforces "outside support" for the installed binary's console mode: an
  elevated administrator who runs it by hand on a pending host spends the token and writes an
  identity the service then refuses.
- TPM-only BitLocker against a stolen device is weaker than TPM with a PIN (R18's control).
- The editor's copy in the administrator's profile (`configuration.md:585`) is not in the carried
  token entry.
- R17 has no repair until the custody verb, after F3 and F4: on the client-simulation machines an
  ordinary user's pre-created `<P>` blocks the host.
- ADR-0010 §Decision part 1 (an elevated user process as the MVP floor) is not read against the run by
  hand being outside support.
- About 1,800 lines of migration code stay in the LocalSystem binary with no test that runs it as
  SYSTEM through the service once point 2's code lands.

## Gates and CI

- No code changed: no Rust, TypeScript, deploy or package gate ran; no elevated gate.
- markdownlint, locally, on the six files after each round and before each push: "Summary: 0 issues
  in 0 files", but for `18dccdd` on the owner-review branch (MD018, fixed in `fe29e57`).
- Pushes:
  - `5198ce6`, `18dccdd` and `fe29e57` to `docs/console-unsupported` with no PR open: no workflow ran
    ("runs: 0" at `5198ce6`).
  - `9f34228` (`main`): markdown-lint → success. ALL GREEN.
  - This close's commit: its post-push report.

## Test baselines at close

Unchanged from Session 83 (no code): `cargo test --all` unelevated on this host, 642 passed, 0
failed, 11 ignored.

## Debts

- Opened: **#170**, R18 (an enrolled endpoint's disk imaged, cloned or read offline carries its
  identity).
- Closed by `9f34228`: **#87** (R1 closed by scope, on the fresh installation's custody), **#144**
  (its destination, the package, met by `3adfcd2`), **#164** (R17 corrected), **#168** (the
  Application log's entries).
- Re-routed: **#166** (the custody verb for the setup's hosts, after F3 and F4), **#167** (its retries
  and uninstallation fixes in a change of their own, then install_ac_016; its 27 "when the agent never
  enrolled there" has no subject).
- Open at close: **55**.

## Waiting on Manuel

The owner items of the roadmap's Part (b) §12, which have calendar time and are best started now: the
Authenticode decision and its purchase; #130's legal readings; the product's commercial licence and
terms; the security contact of `SECURITY.md`; the isolated Windows 10, Windows 11 and Server 2019
machines (M3); a penetration test; and the acceptance or control of #99, #124 and #125.

## How Session 86 resumes

1. Run the start protocol. `main` is this close's commit; branches `docs/c10-migration` (`1cf1b12`)
   and `docs/spec-024-amendment-2026-11-18` (`1f06f31`) unchanged, with their worktrees; 55 open
   debts; 22 ADRs / 23 SPECs. Check memory and `C:` (15 GB at this close: builds with
   `CARGO_PROFILE_*_DEBUG=0`).
2. **SPEC-024 Amendment 2026-11-20, point 2's code** (the roadmap's step 1), on a branch with a PR:
   - `cg-setup preflight`: a non-empty `--migrate-legacy=` or `--trust-anchors=` is 28 after the
     platform, the elevation and the host's state, an untrusted state staying 27, before 26 and 29;
     the 24 for a malformed request becomes 28. `configure`: 40 for a non-empty value.
   - `cg-agent`: `--migrate-legacy=` is code 2; a start without arguments beside a staging or legacy
     directory that counts is code 5, before any change (`prepare`'s arms in `migration.rs:620-631`;
     the committed host's old-copies step at `windows.rs:421-430` becomes unreachable).
   - Every refusal line that offers the migration reworded (point 3; round 12's list B, item 13).
   - The runbooks' lines (`install.md:7`, `:467`, `:468`, `:635`, `:681-738`, `:763`, `upgrade.md`'s
     route for an agent installed before SPEC-023, `configuration.md:532`, `data-protection.md:41`),
     and R18's lines (an endpoint imaged before the setup, never after; a clone purged and installed
     anew; disk encryption), with the deploy tests that pin them (`upgrade.test.mjs:132`, `:145`,
     `:153`, `:154`; `install.test.mjs:168`, `:193-197`).
   - Tests red first; the four CI-confined tests (install_ac_013's service test and the adoption's in
     rust-ci's Windows job, `cg-setup`'s service test, a new case of `package_bundle_host`) seen red on
     a probe pull request closed without landing. The Rust gate, the deploy bullet and the Windows
     package bullet; no elevated gate (no path of *Developer-local SPEC-005 marquee validation* §5).
     The self-review in two halves, and a fresh security pass after its fixes.
3. **#167's fixes for every installation**, from `51ba811` and `2c9f74e` (the "not read" marks on
   the retries after 40 to 45 and 1602; the uninstallation's `$dir` check; the entry-by-entry removal
   of `C:\CyberGuardSetup`), in a change of their own from `main`, the console clauses left out; then
   delete `docs/c10-migration` and `docs/spec-024-amendment-2026-11-18`, local and on origin, and
   their worktrees (check the junction targets before `rmdir`).
4. **F3's and F4's contracts together** (step 3): F3 with the triage and the audit log's read view
   in scope; F4's departures from ADR-0004 (a CSR, the CA's lifetime, a revocation store across
   Valkey's prefixes) as an ADR amendment for Manuel's yes; the token route's trust boundary as a
   question to him if it changes one.
