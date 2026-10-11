# Handoff — End of Session 35

Full state-of-the-world at the S35 close. Written so a cold or compacted session
recovers the thread in one read.

Session 35 is the first under the executor-led rules of CLAUDE.md (S34-06,
`c14b89c`). It landed the network half of roadmap **Phase D**: the SPEC-019
implementation that S34 left on `review/s34-d-net`, after a self-review that
amended SPEC-019 (`d4f2a53`), added three commits of fixes and one of
CLAUDE.md counts, and an elevated gate that now measures event 31. The branch
and draft PR #4 are closed. The session then gathered the facts of the login
half, and Manuel ratified ADR-0019 (Accepted). SPEC-020 landed Accepted
(`71f11b7`). The elevated gate on its implementation found two defects, and
on 2026-10-11 ADR-0019 and SPEC-020 were amended to fix them. The
implementation then landed as `89db206` to `45e7fb7`, after a green gate on
the tree that landed (§SPEC-020's elevated gate and the logon diagnostics).
PR #5 is closed. Roadmap **Phase D is DONE**: MVP criterion 2 is delivered
(processes, network, logins). Catalogs: ADR 19 / SPEC 20. Known CI debt:
ZERO.

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
| `63d3f8d` | `docs(adr-0019)`: ADR-0019 Accepted; catalog; roadmap §D; this handoff. |
| `71f11b7` | `docs(spec-020)`: SPEC-020 Accepted; both catalogs; roadmap §D. |
| `1f54b30` | `docs(adr-0019, spec-020)`: Amendments 2026-10-11 — the end of a read, the gate's successful logon; this handoff. |
| `89db206` | `feat(cges)`: Authentication 3002 properties and time. |
| `775e194` | `feat(ingest)`: the route accepts and stores 3002; eleven columns and the 365-day time-to-live. |
| `53387e4` | `feat(agent)`: decode logons; a third class in the ring. |
| `f191a1b` | `feat(agent)`: subscribe to the Security log. |
| `1cee990` | `test(ingest)`: the logon marquee `auth_ac_013`. |
| `c40694a` | `docs(claude-md)`: the logon tests in the elevated gate. |
| `0f8e73f` | `fix(agent)`: no past-log reader in the library; shutdown order. |
| `16767c0` | `test(ingest)`: the real TTL upgrade; a redacted, loopback-clean marquee. |
| `45e7fb7` | `fix(agent)`: a read ends on 4317; the gate's success is an S4U logon. |
| (this commit) | Session close: this handoff; README criterion 2; roadmap §D DONE; CLAUDE.md `--no-fail-fast` and `CARGO_INCREMENTAL=0`. |

The nine code commits are cherry-picks (`-x`) of the rebased branch, the
`NOT YET RATIFIED` marker stripped from the first five; before each push
`git diff --quiet <branch-sha> HEAD` returned 0, and `main`'s tree after the
ninth equals the branch tip `599424d` that CI and the elevated gate ran on.

SPEC-020's nine commits landed the same way. They are cherry-picks (`-x`) of
`feat/s35-spec020-logins` rebased onto `1f54b30` (`79d2894` to `0d03697`). CI
was green on each, on the branch and on `main`, and `git diff --quiet
<branch-sha> HEAD` returned 0 before each push. `main`'s tree after `45e7fb7`
equals `0d03697`, the tree the last elevated gate ran on.

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

### SPEC-020 and its implementation

**The SPEC text.** Claude Code's own pass, then the fixed contract prompt.
List A had seven entries, all resolved before `71f11b7` landed (its commit
message), among them:

- a departure from ADR-0019's "as written" (now `-`);
- SPEC-005's reservation of exit 9 (now amended by scope);
- an exclusion check that could pass vacuously (now run on the log's own
  recent records).

**The code, first pass** (fixed code prompt, `71f11b7..81d347c`). The
reviewer found auth_ac_001, 002, 004–006, 008, 010 and 011 covered, and
auth_ac_013 covered with the same reservations as auth_ac_012.

- **Weak:**
  - auth_ac_007 took the codes from the implementation's constant;
  - auth_ac_012's exclusion check used the agent's own predicate, so it
    could not fail; it printed no per-event delay and removed only `IPC$`;
  - auth_ac_003 simulated the upgrade on a table that already had the
    time-to-live;
  - auth_ac_009 compares byte identity as value equality.
- **Contradicted:**
  - §6, the subscription closed by the thread before the join;
  - §11, an Entra ID SID (`S-1-12-1-…`) printed whole.
- **Beyond the contract:** `query_recent`, a reader of past logons in the
  library, which ADR-0019 §8 (live only) does not allow.

Dispositions:

- **Fixed in `153986b`** (on the branch):
  - `query_recent` removed; the test renders through the public
    `RenderContext`;
  - the shutdown order;
  - the codes taken from the SPEC;
  - the exclusion judged with the SPEC's list;
  - the per-event delay;
  - every `\\127.0.0.1` connection removed;
  - every SID form redacted.
- **Fixed in `1bc43aa`:** auth_ac_003 runs the real upgrade.
- **Not blocking — auth_ac_009,** for the reason given for net_ac_007 above.
- **Not blocking — auth_ac_014:** it is the gate itself.
- **The decoding was written before its tests:** six deliberate mutations of
  it, each caught (`7d4fbdd`), stand in for the red-first run.

**The code, second pass** (`caf2288..fe62edf`, contract: SPEC-020 and the
defect statement of the first gate). It found that the signal-only loop of
`fe62edf` gave up the poll read, and with it the finding of a dead
subscription (SPEC-020 §Risks, Decision record 8). It also found that the
new test checked only a two-entry table, and that two of the comments' claims
rested on one observation. Disposition: the loop reads on the poll again,
and 4317 ends a read (`9bf6230`, then SPEC-020 Amendment 2026-10-11). The
claims not measured were removed.

**The amendments' text** (fixed contract prompt, the working tree before
`1f54b30`). List A had five entries, all fixed before `1f54b30` landed:

1. the handoff said every other test had passed, when cargo had stopped at
   `auth_ac_012`;
2. "fixed at `9bf6230`, first as `fe62edf`" joined two opposite loops;
3. the 1.09 s delay was read as the arrival time, but it is an upper bound,
   set by when the test first looked;
4. the ADR's heading "The loopback is not audited" clashed with its own
   audited loopback 4625;
5. the handoff's opening and resume steps were stale.

From list B, applied:

- a gate run makes two S4U logons, and a 4672 only for an administrator;
- S4U was measured in a workgroup;
- 4317 from a dead subscription would go unseen (now in SPEC-020 §Risks);
- Decision record 11's discarded binary;
- the gate's tree evidence.

Not applied:

- ADR-0019:24 says `windows-sys` gains "the feature", and the agent gained
  two, `Win32_System_EventLog` and `Win32_Security`. No crate was added
  either way (debt #49);
- `docs/product/roadmap.md:225`, fixed in this close.

**The code, third pass** (`1bc43aa..6a32b38`, contract: SPEC-020 and
ADR-0019 with their amendments). Nothing in the data contracts was
contradicted.

- **Weak:** nothing tested that `drain` ends a read on 4317. Reverting it to
  259 alone left `logon_read_end.rs` green.
- **Missing:** a test of the poll read and of the reset before the read.
- **Near-misses:**
  - the module comment promised a dead subscription found "within a second"
    without the 4317 limit;
  - "4317 with no record" was not checked;
  - the marquee's failure message did not name the Server service;
  - `11972b6` was not true at its own SHA for the elevated criteria.
- **Beyond the contract:** `read_is_exhausted` public; the Cargo comment
  imprecise; the helper's `-ExecutionPolicy Bypass` and `Add-Type`.

Dispositions, all in `0d03697`, which folds the two fix commits into one:

- `subscription.rs`'s unit test drains a real subscription to the
  `Application` channel, unelevated. Seen red with `drain` ending on 259
  alone (`left: Err(4317)` on the second read), green with the fix.
- `drain` checks `returned == 0`.
- The comment states the limit, and the marquee names the Server service.
- The Cargo comment is exact.
- The reset before the read has no test, since a race cannot be shown. The
  gate shows the loop end to end.
- The public function, `Bypass`, `Add-Type` and `CgGate`: not blocking. They
  change no host setting, and the ADR amendment states the logon.

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

## SPEC-020's elevated gate and the logon diagnostics (2026-10-11)

Manuel ran the elevated gate on `feat/s35-spec020-logins`; logs in
`C:\tmp\s35-gate2-*.log` (tip `caf2288`) and `C:\tmp\s35-gate3-*.log` (tip
`fe62edf`).

- **The tree each run covered.** In the first run, `net session` answered
  `No hay entradas en la lista.` (elevated), `git rev-parse --short HEAD`
  printed `caf2288` and `git status --short` printed nothing. The second
  run's paste began at `git rev-parse --short HEAD`, which printed `fe62edf`;
  its `net session` and `git status` lines were not pasted.

- **At `caf2288`: no logon delivered.** `auth_ac_012` printed
  `ring_logons=0 recent_4624=200 excluded_targets=196 excluded_reported=0`,
  and `auth_ac_013` printed `"logon_rows_total":0`. The agent's log in the SPEC-005
  marquee shows `Security log subscription opened` at 22:30:37.527 and
  `Security log subscription failed; reopening it from new events`,
  `"operation":"read","win32_code":4317` at 22:30:38.540.
- **Not every test ran.** The rest of the vitest suite passed in both runs
  (59 of 60 files, 196 of 197 tests). The cargo run stopped at the failing
  `auth_ac_012` both times, so the `--ignored` tests after it in order did
  not run: `capture_ac_006`, `net_ac_008`, `net_ac_009`, `process_ac_004`,
  `process_ac_007` and `process_ac_009`. The next gate runs cargo with
  `--no-fail-fast`.
- **Cause, measured unelevated** on the `Windows PowerShell` channel with a
  pulled subscription:
  - a read with no new record since returns 4317 (`ERROR_INVALID_OPERATION`),
    and a read that exhausts the records returns 259;
  - the subscription still delivers after a 4317;
  - a read finds new records with or without the signal.

  The thread's 1 s poll read took 4317 for a failure. The first fix,
  `fe62edf`, read only on the signal, and the second gate ran on it. Its
  self-review restored the read on the poll, with 4317 ending a read, and
  the commit was amended to `9bf6230`, which was later folded into
  `0d03697` (SPEC-020 Amendment 2026-10-11).
- **At `fe62edf`: the failures arrive, the success does not.** Both failures
  were in the ring when the test first looked, 1.09 s after their timestamps.
  That is an upper bound on the delay, set by the test's own steps:

  ```text
  auth_ac_012 withheld uid=S-1-0-0 status=0xc000006d sub_status=0xc0000064 name_withheld=true type=3 package=Negotiate source=none elevated=None delay=1.0923571s
  auth_ac_012 kept uid=S-1-0-0 status=0xc000006d sub_status=0xc000006a name_withheld=false type=3 package=Negotiate source=none elevated=None delay=1.0910959s
  auth_ac_012 success not found
  auth_ac_012 ring_logons=2 recent_4624=200 excluded_targets=196 excluded_reported=0
  ```

  `auth_ac_013` stored the failure: `"status_id":2,"user_uid":"S-1-0-0","name_withheld":true,"logon_type_id":3,"status_code":"0xc000006d","status_detail":"0xc0000064","auth_protocol":"NTLM","source_present":true,"elevated_token":null`.
  It stored no success (`"logon_rows_total":1`). The 4625 of an existing
  account (`0xc000006a`) also carries the null SID, which is why ADR-0019 §5
  tests the code rather than the SID.
- **`C:\tmp\s35-netuse-facts.ps1`** (elevated, redacted) ran `net use` to the
  machine three ways with the current credentials:

  ```text
  == net use to 127.0.0.1: exit=0 smb_client_connections before=0 during=1 server_sessions before=0 during=1 security_events=0
  == net use to localhost: exit=0 smb_client_connections before=0 during=1 server_sessions before=1 during=2 security_events=0
  == net use to <host>: exit=0 smb_client_connections before=0 during=1 server_sessions before=1 during=2 security_events=0
  ```

- **`C:\tmp\s35-s4u-facts.ps1`** (elevated, redacted) made one S4U network
  logon of the current user:

  ```text
  == S4U network logon of the current user: logon status=0x00000000 sub=0x00000000 token=True security_events=4
    4624 v3 type=3 sid=S-1-5-21-...-1001(current-user) name_ends_dollar=False domain=<host> logon_process=CgGate package=MICROSOFT_AUTHENTICATION_PACKAGE_V1_0 lm=- elevated=%%1842 impersonation=%%1832 ip='-' workstation='-' caller=powershell.exe subject=S-1-5-21-...-1001(current-user) linked=False virtual=%%1843
    4672 subject=S-1-5-21-...-1001(current-user)
    4634 type=3 sid=S-1-5-21-...-1001(current-user)
    other ids: 4798x1
  ```

- **At `0d03697`, the tree that landed: green.** After the amendments,
  the branch was rebased onto `1f54b30`. The two fix commits were folded
  into one with the second review's fixes, as `0d03697`. Manuel ran the
  gate again; logs in `C:\tmp\s35-gate4-*.log`, cargo with
  `--no-fail-fast` and `CARGO_INCREMENTAL=0`.
  - **The tree:** `net session` answered `No hay entradas en la lista.`,
    `git rev-parse --short HEAD` printed `0d03697`, and `git status --short`
    printed nothing.
  - **cargo `--ignored`:** all seven passed: `auth_ac_012`,
    `capture_ac_006`, `net_ac_008`, `net_ac_009`, `process_ac_004`,
    `process_ac_007`, `process_ac_009`. `net_ac_008` printed
    `refused_attempt_reported=false`. `auth_ac_012` printed:

    ```text
    auth_ac_012 withheld uid=S-1-0-0 status=0xc000006d sub_status=0xc0000064 name_withheld=true type=3 package=Negotiate source=none elevated=None delay=1.029353s
    auth_ac_012 kept uid=S-1-0-0 status=0xc000006d sub_status=0xc000006a name_withheld=false type=3 package=Negotiate source=none elevated=None delay=1.0271076s
    auth_ac_012 success uid=S-1-5-21-...-1001 status=0x0 sub_status=0x0 name_withheld=false type=3 package=MICROSOFT_AUTHENTICATION_PACKAGE_V1_0 source=none elevated=Some(true) delay=1.0254449s
    auth_ac_012 ring_logons=3 recent_4624=200 excluded_targets=197 excluded_reported=0
    ```

  - **vitest `services/ingest`:** `Test Files  60 passed (60)`,
    `Tests  197 passed (197)`, nothing skipped.
    - `detect_ac_001` printed the probe as `...\winword.exe` and the child as
      `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`.
    - The SPEC-005 marquee took 40.047 s and `net_ac_010` 25.097 s.
    - `auth_ac_013` took 25.062 s and printed two rows: the failure, as at
      `fe62edf`, and
      `{"status_id":1,"user_uid":"S-1-5-21-...-1001","name_withheld":false,"logon_type_id":3,"status_code":"","status_detail":"","auth_protocol":"MICROSOFT_AUTHENTICATION_PACKAGE_V1_0","source_present":false,"elevated_token":true}`.
- **What it settles.**
  - **ADR-0019 §10.** The codes §5 relies on, and the name withheld for a
    missing account and kept for an existing one, are shown on the real log.
    A 4625 carries the null SID in both cases. A person's successful logon is
    reported with its SID, name, domain and type, and the log's own recent
    system logons are not: 197 excluded, none reported.
  - **SPEC-020 Open question 1** stays closed: every event was in the ring
    when the test first looked, 1.03 s after its timestamp. That figure is an
    upper bound, under the reopen condition of 5 s.
- **Manuel's decision, 2026-10-11, in chat.** Asked how the gate should
  cause the successful logon, he chose "S4U of the current user"; the other
  options were a temporary account and no caused success. That answer
  ratifies ADR-0019's Amendment 2026-10-11.

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

### SPEC-020 and its implementation (2026-10-10 to 2026-10-11)

Anticipated:

1. **A pulled subscription on the agent's own thread, values rendered by
   name.** The other decisions taken when SPEC-020 was written are in its
   `## Decision record`, items 1 to 10.
2. **Stand-in for red-first on the decoding.** The decoding was written
   before its tests, so six deliberate mutations, each caught, stand in for
   the red-first run (`7d4fbdd`). The alternative was to delete and rewrite
   it test-first.
3. **One commit for the read fix and the S4U tests.** `0d03697` folds them,
   with the third review's fixes, because both implement one amendment and
   neither was true alone against it. The alternative kept two commits, the
   first false at its own SHA for the elevated criteria.
4. **A pure function plus a real subscription in the unit test.**
   `read_is_exhausted` is a pure function on every platform, tested alone.
   `drain` is tested on a real subscription to `Application`, unelevated
   and with no logon. The discarded alternative was a test seam for the
   channel in the library.
5. **The marquee's S4U helper is a PowerShell script with inline C#**
   (SPEC-020 Decision record 11). Discarded: a binary in the crate, which
   needs a gate step of its own, and a Node FFI package, a new dependency.

Reactive:

1. **The poll read (gate on `caf2288`).** No logon was delivered. The probe
   on the `Windows PowerShell` channel showed `EvtNext`'s 4317 after 259,
   which the 1 s poll read hit. The first fix (`fe62edf`) read only on the
   signal. The second review showed that gave up finding a dead subscription,
   so the poll read came back with 4317 ending a read (SPEC-020 Amendment
   2026-10-11).
2. **The loopback logon (gate on `fe62edf`).** The success never arrived.
   Two redacted diagnostics showed that a loopback `net use` with the
   caller's credentials leaves no event and that an S4U logon leaves a 4624
   of type 3. ADR-0019 §10 was amended on Manuel's choice.
3. **Disk full.** `C:` reached 0 bytes free while a worktree build relinked
   every test binary. Claude Code freed the cargo debug cache
   (`target/debug/incremental`, 18.2 GB, then `cargo clean --profile dev`,
   34.8 GB). Builds now run with `CARGO_INCREMENTAL=0`, and the worktree was
   dropped.
4. **Docker engine hung after the disk filled.** `docker desktop restart`
   timed out. With Manuel's OK, Claude Code stopped the Docker Desktop
   processes and started it again; it was up in about 20 s.
5. **`--no-fail-fast`.** Cargo stopped at the failing `auth_ac_012` in two
   gate runs, so six elevated tests never ran. The gate now runs cargo with
   `--no-fail-fast` (CLAUDE.md, this close).

Manuel's decisions, 2026-10-11, in chat:

- "S4U of the current user", as the gate's successful logon. This ratifies
  ADR-0019's Amendment 2026-10-11; the other options were a temporary
  account and no caused success.
- "Restart it yourself", for the hung Docker engine.

## Test baselines

At `45e7fb7` (= `0d03697`'s tree), on Windows:

- **Elevated** (Manuel, §SPEC-020's elevated gate): cargo `--ignored`, run with
  `--no-fail-fast`, 7 passed. Vitest `services/ingest`: 60 files / 197 tests,
  nothing skipped.
- **Unelevated** (Claude Code):
  - `cargo fmt --check` clean, and `cargo clippy --all-targets
    --all-features -D warnings` clean.
  - `cargo test --all`: 125 passed, 0 failed, 7 ignored.
  - Vitest `services/ingest`: 60 files — 55 passed, 1 skipped, 4 failed; 197
    tests — 192 passed, 1 skipped, 4 failed. The four failures are the
    capture marquees, on the agent's exit code 9 (`detect_ac_001` as 0
    alerts), as CLAUDE.md *Environment facts* expects.
  - Vitest `services/api`: 31 files / 62 tests passed.
  - `task validate-schemas`: valid.
- **CI**: green on every commit, on the branch and on `main`.

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
ratified, and so is its Amendment 2026-10-11. New: the corrections of the
debts #48 (ADR-0012:240) and #49 (ADR-0019:24, :150), both amendments of
accepted ADRs.

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

- **#48 — ADR-0012:240 says 4001 and 3002 are schema-only.** Both flow end
  to end since SPEC-019 and SPEC-020 landed; their detection rules stay
  deferred, as that line says. The correction is an amendment of an accepted
  ADR: Manuel's.
- **#49 — ADR-0019:24 and :150 say `windows-sys` gains "the feature" / "a
  feature".** The agent gained two, `Win32_System_EventLog` and
  `Win32_Security`, and no crate, as both lines state. The tests' feature
  `Win32_Security_Authentication_Identity` is a dev-dependency's. The wording
  rides the next ADR-0019 amendment.
- **#50 — A dead subscription that answered 4317 would not be found**
  (SPEC-020 §Risks). What `EvtNext` returns after the Event Log service
  restarts was not measured; measuring it restarts a service of the host.
- **#51 — The S4U success was measured on a local account in a
  workgroup.** On a domain-joined or Entra-joined developer machine,
  auth_ac_012 and auth_ac_013 may fail on the success step and need another
  way to cause it (SPEC-020 §Risks).

Known CI debt: ZERO rows.

## How Session 36 resumes

1. Run CLAUDE.md *Session protocol*, §At the start. No branch or PR should be
   open.
2. **Phase D is DONE.** Next, in the roadmap's order: **B2**, the capture of
   CommandLine and subject_user_sid. It needs its own ADR, and its capture
   source is an owner STOP. Then **E** and **F**.

   In any later report of logon data, the user appears as `<user>`, the host
   or domain as `<host>`, and each SID as its prefix and RID only. Raw files
   do not leave the machine.
3. Watch #43 in any capture run, and #51 on any other developer machine.
