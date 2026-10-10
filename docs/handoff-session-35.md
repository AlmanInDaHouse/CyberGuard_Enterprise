# Handoff — End of Session 35

Full state-of-the-world at the S35 close. Written so a cold or compacted session
recovers the thread in one read.

Session 35 is the first under the executor-led rules of CLAUDE.md (S34-06,
`c14b89c`). It landed the network half of roadmap **Phase D**: the SPEC-019
implementation that S34 left on `review/s34-d-net`, after a self-review that
amended SPEC-019 (`d4f2a53`), added three commits of fixes and one of
CLAUDE.md counts, and an elevated gate that now measures event 31. The branch
and draft PR #4 are closed. The session then gathered the facts of the login
half and Manuel ratified ADR-0019 (Accepted); SPEC-020 is not written.
Catalogs: ADR 19 / SPEC 19. Known CI debt: ZERO.

## Anchor commits

On `main`, pushed, one commit per push, CI green on each:

| SHA | What |
|---|---|
| `d4f2a53` | `docs(spec-019)`: Amendment 2026-10-10 — event 31 at the gate, the server's checks, four statements. |
| `8fd71eb` | `feat(cges)`: `actor` in the 4001 class file; `time` in `event.json` names 4001. |
| `0eba41a` | `feat(ingest)`: the route accepts and stores class 4001; six new columns. |
| `6ebe88c` | `refactor(agent)`: the ring, the batch and the envelope carry both classes. |
| `d5b330b` | `feat(agent)`: Kernel-Network in the session; decoding; dispatch. |
| `240dc52` | `test(ingest)`: the network marquee `net_ac_010`. |
| `e2c82ae` | `fix(ingest)`: a 4001 `event_id` must be a UUID and its `pid` fit 32 bits. |
| `2901169` | `test(agent)`: `net_ac_009` over IPv6 too; full failure reports. |
| `b24574b` | `test(ingest)`: `net_ac_010` logs its time on every run; 1007 readers. |
| `8fac5c3` | `docs(claude-md)`: six real-ETW tests and `net_ac_010` in the gate. |
| `850a3fe` | handoff-35; README and roadmap pointers; roadmap §D status. |
| `bee5184` | handoff-35: the logon facts and Manuel's ADR-0019 decisions. |
| (this commit) | `docs(adr-0019)`: ADR-0019 Accepted; catalog; roadmap §D; this handoff. |

The nine code commits are cherry-picks (`-x`) of the rebased branch, the
`NOT YET RATIFIED` marker stripped from the first five; before each push
`git diff --quiet <branch-sha> HEAD` returned 0, and `main`'s tree after the
ninth equals the branch tip `599424d` that CI and the elevated gate ran on.

## How the branch landed

1. **Self-review, own half** (CLAUDE.md *Compensating controls* §5). The
   matrix of SPEC-019's criteria against `a5514c2..2b0d3c9`: net_ac_001 to
   net_ac_010 covered, each by the test of its name; net_ac_011 and
   NFR-019-003 by the gate output. One weak point: no criterion measured event
   31 on real ETW.
2. **Self-review, reviewer** (fresh-context subagent, the fixed code prompt).
   Its lists and what was done with them are under *Self-review record*.
3. **The contract first.** SPEC-019 Amendment 2026-10-10 (`d4f2a53`), itself
   self-reviewed: own pass, then the fixed contract prompt, whose list A (four
   entries) the final text resolves.
4. **The fixes** on the branch, after rebasing it onto `main`: `6505239`,
   `d522fb9`, `309cf96` and `599424d` (landed as `e2c82ae` to `8fac5c3`). The
   two new net_ac_002 cases were seen failing without the schema change (500
   and 200) and passing with it.
5. **Gates.** Local: Rust and TypeScript (below). CI: the nine commits pushed
   one per push to the branch, all green, then the same nine to `main`. The
   elevated gate on `599424d` (below).

## The elevated gate (tip `599424d`, Manuel, 2026-10-10)

Manuel ran it; Claude Code read the logs (`C:\tmp\s35-gate-*.log`).

- `net session` answered `No hay entradas en la lista.`: elevated. HEAD
  `599424d`; `git status --short` printed nothing.
- `cargo build --release -j 2 -p cg-agent` compiled `cg-agent`;
  `target/release/cg-agent.exe` was written at 19:21:03 and no file under
  `agent/` is newer.
- `cargo test -j 2 -p cg-agent -- --ignored --test-threads=1 --show-output`
  (log of 20:12): 6 passed — `capture_ac_006` (elevated shutdown),
  `net_ac_008` (8.18 s), `net_ac_009` (3.07 s), `process_ac_004`,
  `process_ac_007`, `process_ac_009`. `net_ac_008` printed
  `net_ac_008 refused_attempt_reported=false`.
- `pnpm test` in `services/ingest` (log of 20:08): **54 files / 175 tests**
  passed, nothing skipped; `ac-001-marquee` ran. `detect_ac_001` passed in
  46 410 ms and logged `image_file_name — probe:
  C:\Users\manul\AppData\Local\Temp\cg-detect-probe-xPDeoI\winword.exe; child:
  C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`; the SPEC-005
  marquee logged `marquee_elapsed_seconds` 40.057 against 45 and
  `image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe`;
  `net_ac_010` logged 25.102 against 45, with 48 network rows for its agent.
- The block was run twice: the first run's cargo log stopped at its
  `Compiling` line, and the second paste was cut mid-line (a file named
  `C:\tmp\s35-g` holds a build log). The cargo line was run again on its own;
  the results above are from that log.

### What it settles

- **Event 31.** `net_ac_009` now accepts over IPv6 too and passed: the inbound
  event carries the accepting process's PID, the listener as destination and
  the peer as source. With S34's run, ADR-0018 §4 and §5 hold for all four
  events, on loopback. No statement of ADR-0018 is contradicted.
- **One element per probe.** `net_ac_008` and `net_ac_009` now require that
  every event of a probe's PID is its one connection; both passed.
- **A refused attempt** produced no 4001 element, in two runs (S34, S35), on
  loopback. ADR-0018 §2's "attempted" stands; SPEC-019 §Open questions 4 is
  not closed by loopback runs.

## Self-review record

**Code reviewer** (fixed prompt, change `a5514c2..2b0d3c9`). Verdicts:
net_ac_001 to net_ac_006 and net_ac_010 covered (net_ac_003 by a simulated
old table); weak: net_ac_007 (byte identity checked as value equality),
net_ac_008 and net_ac_009 (the failure report printed a filtered subset),
NFR-019-003 (the elapsed time not logged when the run threw early);
net_ac_011 a gate fact. Contract statements contradicted, and dispositions:

- §6 "nothing else" while unlisted members are ignored — SPEC restated
  (`d4f2a53`): as for 1007.
- §6 "checks every member": a non-UUID `event_id` answered 500, `pid`
  unbounded — fixed (`e2c82ae`), SPEC states the check.
- §4 own-PID records counted as discards when their id is another — SPEC
  restated: such a record is evidence of an unhonoured filter whoever wrote it.
- §8 two readers without `class_uid = 1007` — fixed (`b24574b`).
- §3 / NFR-019-001 locks — SPEC restated (no lock of the callback's own
  beyond the cache's and the ring's).
- Weak net_ac_008/009 reports and NFR-019-003 — fixed (`2901169`,
  `b24574b`). net_ac_007 — not blocking: the agent serializes the same
  rendered values on each attempt, and serde_json writes equal values as
  equal bytes; a re-render that changed content would differ in value.

**Contract reviewer** (fixed prompt, the amendment draft). List A: the
enumerated lock list was itself incomplete (the uuid v7 generator, ferrisetw's
parser cache); NFR-019-001 not amended; §2 and §4 clashed on an own-PID record
with unreadable fields; the pre-1970 `error` line is I/O. All four resolved in
the text that landed. From list B, applied: the gate rerun, the input that is
not backward-compatible, the destination of the 1007 gap (debt #31), "0 to
4294967295", one probe per listener, "measures".

## Logon facts (for ADR-0019)

After the landing the session went on to the login half of §D. Facts read on
Manuel's machine on 2026-10-10 (Windows 11 Home 10.0.26200, workgroup). The
elevated part came from `C:\tmp\s35-logon-facts.ps1`, which Manuel ran and
which wrote only redacted aggregates (`C:\tmp\s35-logon-facts.txt`): user
names as `<user>`, the host as `<host>`, SIDs as prefix and RID, any field off
an allow-list of system codes as `<redacted>`. Claude Code read only that file.

- **Manifest** (unelevated): 4624 has versions 0 to 3; version 3 has 28
  fields, `ElevatedToken` from version 2. 4625 has one version, 21 fields,
  with `Status`, `FailureReason`, `SubStatus`.
- **Access**: unelevated, reading the Security log is denied
  (`UnauthorizedAccessException`); an elevated Administrator token reads it.
  Channel access `O:BAG:SYD:(A;;0xf0005;;;SY)(A;;0x5;;;BA)(A;;0x1;;;S-1-5-32-573)`.
  `SeSecurityPrivilege` present, disabled.
- **Audit policy** (effective): Logon success and failure; Logoff success;
  Special Logon success; Other Logon/Logoff, Credential Validation, Kerberos
  and Process Creation not audited — yet the log holds 112 events 4688, not
  explained (a B2 fact).
- **The log**: circular, 20 MB, 33 444 records over 151.5 h; 5379 64 %, 4798
  22 %, 4624 1532, 4672 1486, 4799 715, 4648 49, 4634 28, 4647 8; no 4625,
  4800, 4801, 4778 or 4779.
- **4624**: all version 3; 1429 of type 5 for SYSTEM, 1445 from
  `services.exe`; the account `S-1-5-21-...-1001` (`MicrosoftAccount`) 44
  times — type 11 by `svchost.exe` from `127.0.0.1` (30) and type 7 by
  `lsass.exe` (14), each once with `ElevatedToken` `%%1842` and once with
  `%%1843`; type 2 only for `DWM-<n>` and `UMFD-<n>`; 3 for `S-1-5-83-...`.
- The ETW providers with logon in their name (Security-Auditing, Winlogon,
  LSA, LsaSrv, AuthenticationProvider, Kerberos, NTLM) were listed, not
  examined.

### Manuel's decisions on ADR-0019 (2026-10-10, in chat)

1. The 4624s reported are people's accounts; every 4625 is reported.
2. The fields: SID, name, domain, logon type, source (workstation and
   address), authentication package, elevated token.
3. A failed logon keeps its name only when the account exists.

The third was offered as "the SID is null"; a 4625 commonly has the null SID
whether or not the account exists, so the draft tests the failure code
instead, and the gate settles it (ADR-0019 §5, §10).

## Decisions taken

Anticipated:

1. **Measure event 31 before landing** rather than land with it recorded as a
   risk: one more gate run against an unverified src/dst assignment for
   inbound IPv6.
2. **Amend SPEC-019 where the code was right** (§2, §3, §4, §6, NFR-019-001)
   rather than change the code to the text; for §4 the discard count's job
   (detect an unhonoured filter) decided it.
3. **Server checks a UUID of any version**, not v7: the fixtures use v4,
   ClickHouse takes any, and the check exists to answer 400 instead of 500.
4. **net_ac_007 left as value equality** rather than capturing raw bodies in
   the mock (reason above).
5. **Comments describe H1–H3 as "checked by the elevated gate"** rather than
   "measured", so they are true whatever the order of commit and gate.
6. **Cherry-pick with `-x`**, keeping the PR #4 commit each landed commit
   came from.

Reactive:

1. The amendment's first draft listed locks; the reviewer showed the list
   could not be closed, and the text became a requirement.
2. The TypeScript gate was stopped by the system for low memory; Manuel chose
   to re-run `services/api` alone (31 files / 62 tests, green).
3. The second gate paste was cut; the cargo line was re-run alone.
4. CLAUDE.md *Environment facts* (`c14b89c`) named `net_ac_010` before it was
   on `main`; it is true from `240dc52`.

Manuel's decisions this session (2026-10-10): re-run only the `services/api`
gate after the memory stop; on ADR-0019, the three of §Logon facts; then he
delegated the four points the draft left open ("esas 4 decídelas tú en factor
de lo más beneficioso en el contexto del proyecto y de cara a que sea producto
vendible a una empresa"), which Claude Code decided as ADR-0019 §Decision
record states — retention of 365 days from arrival, the marker without a
length, ANONYMOUS LOGON reported, no test account; and he ratified ADR-0019
("si"). It landed from the owner-review branch `adr/s35-0019-logins`, squashed
without the marker; its self-review (a fresh-context reviewer, list A of six)
is in the branch's first commit message.

## Test baselines

At `8fac5c3` (= `599424d`'s tree), on Windows:

- **Elevated** (Manuel, above): cargo `--ignored` 6 passed; vitest
  `services/ingest` 54 files / 175 tests, nothing skipped.
- **Unelevated** (Claude Code): `cargo fmt --check` clean; `cargo clippy
  --all-targets --all-features -D warnings` clean; `cargo test --all` 57
  targets, 100 passed, 0 failed, 6 ignored. Vitest `services/ingest`: 54
  files — 50 passed, 1 skipped, 3 failed; 175 tests — 171 passed, 1 skipped,
  3 failed: the three capture marquees, exit code 9, as CLAUDE.md
  *Environment facts* expects. Debt #41 did not recur. Vitest `services/api`:
  31 files / 62 tests passed. `task validate-schemas`: valid.
- **CI**: green on every commit, on the branch and on `main`.

## Owner-STOP decisions pending (waiting on Manuel)

Unchanged from handoff-34, minus "Landing `review/s34-d-net`" (done, under the
new rules): ADR-0002 Go→TS reconciliation; the criterion-7 deployment
contract; forensic trust anchoring; B2 capture source; the compose basename
collision #12; amending SPEC-004 with `INGEST_DETECT_*` (#17); the optional
items of handoff-29; and, if a run ever reports a refused attempt, what an
outbound Open asserts (SPEC-019 §Open questions 4, ADR-0018 §2). ADR-0019 is
ratified.

## Debts

- **#1–#14, #17–#21, #24–#30, #32, #33, #35–#41:** unchanged — see
  [handoff-session-26.md](handoff-session-26.md) to
  [handoff-session-34.md](handoff-session-34.md), #25 and #26 as
  handoff-34 restates them.
- **#31 — widened** by SPEC-019 Amendment 2026-10-10: besides `time`, the
  1007 shape's `event_id` is checked only as non-empty (a non-UUID is answered
  500) and its `process.pid` and `parent_pid` are unbounded (a value above
  4294967295 is stored wrapped).
- **#42 — A POST per batch interval when the server shares the agent's
  host.** The agent opens a new TCP connection per POST; the server's accept
  is reported (SPEC-019 §Operational §4), and that event rides the next POST.
  On a co-located install the agent posts about every 5 s indefinitely.
  Connection reuse in the agent's TLS client would end it.
- **#43 — Launch and connect may reach the callback out of order.** A
  real-time ETW session does not order events across CPUs; a connection
  dispatched before its process's Launch gets no uid, which SPEC-019 allows
  and `net_ac_008` and `net_ac_010` would report as a failure. Not seen in
  the two gate runs (S34, S35). If either fails on the uid alone, this is the first suspect.
- **#44 — SPEC-017 §Operational §6 says the process callback takes no lock
  beyond the cache's and the ring's and does no I/O.** Like SPEC-019's text
  before its amendment, that omits the locks inside ferrisetw and uuid and the
  pre-1970 `error` line.
- **#45 — The local gate does not mirror CI's build steps.** `ts-ci` also runs
  `pnpm run build`, an entrypoint check and `next build`; CLAUDE.md *Local
  pre-commit gate* names typecheck, lint and test only.
- **#46 — Two rules on the validity of a marquee run clash.** CLAUDE.md
  *Developer-local SPEC-005 marquee validation* §2 says the gate is evidence
  only for its tree; the SPEC-006 section and the prod-driver gate let it
  stand over later doc-only commits, and that gate's own known case touched
  comments under `services/`.
- **#47 — No retention for the classes other than 3002.** ADR-0019 §9 gives
  logon rows a time-to-live of 365 days; `cges_events` keeps every other
  class without limit, and ADR-0003's retention table is not implemented.

Known CI debt: ZERO rows.

## How Session 36 resumes

1. Run CLAUDE.md *Session protocol*, §At the start. No branch or PR should be
   open.
2. **SPEC-020 (logins).** ADR-0019 is Accepted; SPEC-020 implements it,
   contract first (CLAUDE.md *Compensating controls* §1). Its accounts,
   fields and failed-name rule are ADR-0019 §3–§5; its storage the columns
   and the 365-day time-to-live of §7 and §9; its elevated criteria the
   logons of §10 (the built-in Administrator, a random name, a loopback
   network logon). SPEC-020 is Claude Code's to write and accept while it
   stays inside ADR-0019; it touches the capture path, so the elevated gate
   applies. In any later report of logon data the user appears as
   `<user>`, the host or domain as `<host>`, and of each SID only the prefix
   and the RID; raw files do not leave the machine.
3. Watch #43 in any capture run.
4. After D: **B2**, **E** and **F**, in the roadmap's order.
