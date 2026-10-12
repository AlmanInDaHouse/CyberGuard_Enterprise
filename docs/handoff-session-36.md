# Handoff — Session 36

Session 36 closed on 2026-10-12 at Manuel's request ("vamos a arar aqui y
lo dejamos pendeinte para mañana"), with ADR-0020 drafted and waiting for
his ratification. Its work order for Session 37 is *How Session 37 resumes*
at the end.

## State at close

- `main` = `origin/main` = the commit that adds this section (CI ALL
  GREEN, *CI monitoring*); tree clean; no stash.
- Catalogs: ADR 19 / SPEC 20 on `main` (ADR-0020 exists only on its
  branch). Known CI debt: zero.
- One branch open, by design: `adr/s36-0020-cmdline-user` at `b751bdc`,
  owner review, no PR (no workflow runs on it). Five commits, each
  `NOT YET RATIFIED`: ADR-0020 (Proposed) and its co-located amendments to
  ADR-0006, ADR-0009, ADR-0011, ADR-0012, ADR-0019 and the threat model.
  It was cut from `bdd1d74`; `main` has moved since (handoff-only
  commits), so it is rebased at landing. Compare URL:
  <https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...adr/s36-0020-cmdline-user>.
- No code changed this session: no file outside `docs/`, `CLAUDE.md`
  and `.github/workflows/README.md`. No elevated gate ran, and none was
  needed.
- Outside the repository, kept for SPEC-021's measurement (Open
  questions 5 of ADR-0020): the probe crate `C:\tmp\s36-b2-probe`
  (ferrisetw `=1.2.0`, built into the repo's `target/release` as
  `s36-b2-probe*.exe`) and the redacted outputs `C:\tmp\s36-b2-*.txt`.

## Start

- `main` = `origin/main` = `cbd3742`, no stash, no branch or PR open, CI
  green; catalogs ADR 19 / SPEC 20; Known CI debt ZERO.
- The working tree was not clean:
  `schemas/cges/v0.1/classes/1001_filesystem_activity.json` was modified at
  03:42 on 2026-10-11, ten minutes after the S35 close commit. The change is
  formatting only (the JSON expanded, the final newline removed) and the
  parsed content equals `HEAD`'s. No commit of this session includes it.

## Progress

Anchor commits on `main`, one per push, CI green on each:

| SHA | What |
|---|---|
| `cf54945` | `docs(spec-017)`: Amendment 2026-10-11 — the dispatch callback's locks and output (debt #44); this handoff started, debts #52 and #53. |
| `a293c27` | `docs(claude-md)`: the local TypeScript gate runs ts-ci's build steps (debt #45). |
| `f37b243` | `.github/workflows/README.md` describes the four workflows (debt #28). |
| `be69547` | `docs(adr-0012)`: Amendment 2026-10-11 — classes 4001 and 3002 flow end to end (debt #48), from the owner-review branch `adr/s36-0012-classes-flow`, ratified by Manuel. Its tree equals the reviewed tip `3d88b5c` rebased (`f2fc6c6`) plus one sentence, the ratification record. |
| `3b21b5f` | SPEC-006, SPEC-015 and SPEC-016 Amendments 2026-10-11, which follow ADR-0012's; roadmap §B1; this progress record. |
| `cb82b39` | This record: B2 decision 1 corrected and accepted; the measured figures. |
| `7a85212` | This record: two measured figures ADR-0020 cites. |
| `dbb93b9` | ADR-0012's ratification sentence and this record quote Manuel literally. |
| `bdd1d74` | Roadmap §B3 and Part (b) §8–§9; SPEC-015 and SPEC-016 Amendments 2026-10-12, the destination of rules over the command line and the user; this record. |
| `2b4532e` | This record: the reviews of ADR-0020's branch after the threat-model entry; debts #55 and #56. |
| (this commit) | The session's close: this record completed; README and roadmap point at it; roadmap §B2's status. |

Manuel's decisions this session, in chat:

- 2026-10-11, asked whether to ratify ADR-0012's amendment and to close
  debt #49 without amending ADR-0019: "1-2 elije tu en base al proyecto".
  Claude Code chose to land the amendment and to close #49; Manuel then
  ratified the amendment ("si").
- 2026-10-11: the reformatted `1001_filesystem_activity.json` in the working
  tree was not his work to keep ("restaura"); Claude Code restored it with
  `git checkout --`.
- 2026-10-11, B2 decision 1, the capture source: option 1, the kernel's
  process events as the single source of process events ("creo que lo mejor
  ligado a lo que es cyberguard y destinado a ser vendido a empresas … es la
  primera opcion").
- 2026-10-11, B2 decision 2, what is kept of the command line: option B, the
  text as the source delivers it (at most 1024 characters), with secret
  values replaced on the agent by a closed list of patterns before it leaves
  the host, visible markers for "redacted" and "possibly truncated", and a
  retention of 365 days from arrival for the command line ("opcion B si es
  la recomendacion, vamos a intentar lllegar a la perfeccion total en el
  proyecto").
- 2026-10-11, B2 decision 3, the user: "1 + i" — the SID of the process's
  own user only, no name; the wire field renamed `process.user_sid` and the
  column `process_user_sid` before it carries data, the old always-empty
  column left in place.
- 2026-10-11, B2 decision 1 corrected: the third probe (below) showed that
  the kernel's process event carries no image path, so it cannot be the
  single source. Proposed to Manuel as option 1′ — Kernel-Process stays the
  source of process events and their path, the kernel's process events add
  the command line and the user SID, joined on the agent by PID and event
  timestamp — he accepted it: "sigo tu recomendacion sobre esa decision".
- ADR-0020 is drafted on the owner-review branch `adr/s36-0020-cmdline-user`,
  waiting for his "sí". Its review changed two things he had decided the
  shape of, both stated in its Decision record: the renamed field is
  `process.user.uid` (the OCSF path the newer classes use on the wire)
  rather than the `process.user_sid` proposed to him, and the parent at the
  edge is its own contract inside §B2, after SPEC-021.
- 2026-10-12, in answer to a message with three points — his "sí" to
  ADR-0020; whether to add the new asset to the threat model (recommended:
  yes, on ADR-0020's branch); and where the rules over the command line and
  the user go (recommended: a phase of their own right after B2) — Manuel
  wrote: "sigo tu recomendacion sobre esa decision". Claude Code read it as
  accepting the two recommendations: the threat-model entry is drafted on
  the owner-review branch, and the rules are roadmap §B3 (Part (b) §8). It
  is not read as the "sí" to ADR-0020, which the message asked for as a
  separate explicit answer and which stays pending.

B2, the facts measured on Manuel's machine (Windows 11 Home 10.0.26200,
workgroup), each from a script or probe that wrote only redacted
aggregates (`C:\tmp\s36-b2-*.txt`):

- **Security 4688.** Process Creation is not audited and
  `ProcessCreationIncludeCmdLine_Enabled` is absent. The log held 112 records
  4688, from 2026-10-06 to 2026-10-11, in 14 per boot over 8 boots: the boot
  processes (`smss.exe`, `csrss.exe`, `wininit.exe`, `lsass.exe`, …), all
  `S-1-5-18`, none with a command line. The log is circular and its window
  has moved since S35, so these are not the same 112 records S35 counted;
  they are of the kind that explains them.
- **Reading a process's command line from outside it**
  (`NtQueryInformationProcess`, class 60). Elevated, 347 of 354 running
  processes were read, at about 32 µs each. A `cmd /d /c rem` probe was read
  0 and 10 ms after `Start-Process` returned and could not be read from 50 ms
  on (`0xC000010A`).
- **Kernel-Process** (the agent's provider). ProcessStart v4 carries no
  command line and no user SID. With the SID enable property, no header SID
  was seen in two runs.
- **Process lifetimes.** Of the processes that started and ended in a 90 s
  window, 42 of 53 lived under 50 ms, median 35 ms.
- **The kernel's process events through ferrisetw 1.2's `KernelTrace`**, a
  session of its own name, opened while another tool held the "NT Kernel
  Logger":
  - `CommandLine`, `UserSID`, `ImageFileName` and `ParentId` were read in
    every record (404 of 404), with no schema error, and the SIDs decoded to
    their string form;
  - all 20 probes were seen at Start with their marker;
  - the session's rundown (DCStart) listed the 357 processes running when it
    opened;
  - the command line is cut at 1024 characters in Start, End and DCStart
    alike (probes of 3000, 6000 and 8000 characters all arrived with 1024);
  - `UserSID` is the process's own user, not its creator's: in 72 records it
    differs from the parent's (services of `S-1-5-19` and `S-1-5-20`, and the
    user's processes, under `S-1-5-18` parents);
  - delivery latency: p50 about 0.73 s, p90 and p99 about 1.0 s, the
    session's flush timer;
  - of the rundown's 352 non-empty command lines, 2 contained the word
    "token" and none matched a password pattern (counts only).
- **The kernel's `ImageFileName` against Kernel-Process's `ImageName`** (third
  probe, both sessions at once for 40 s): the kernel's holds the base name
  only — no backslash in 81 of 81 — and does not truncate it (a 52-character
  file name arrived whole); Kernel-Process's holds the device path in 80 of
  80. Matched by PID, the base names agree in 79 of 79, and the two event
  timestamps differ by at most 0.01 ms. Their arrival at the callbacks differs
  by a median of 0.14 ms; at the 10th percentile the kernel's arrived 794 ms
  earlier, so in a tenth of the pairs about 0.8 s earlier or more; at the
  90th percentile the Kernel-Process event arrived 0.02 ms earlier. Neither
  longest lead was measured.

Debt record changes so far:

- **#44 — CLOSED** (`cf54945`). **#45 — CLOSED** (`a293c27`). **#28 —
  CLOSED** (`f37b243`). **#48 — CLOSED** (`be69547`).
- **#49 — CLOSED, no defect.** The agent's resolved `windows-sys` features
  gained one, `Win32_System_EventLog`: `Win32_Security` was already on
  through `Win32_Security_Authorization` and `Win32_Security_Cryptography`
  (`windows-sys-0.59.0/Cargo.toml:146,148`), so the line `f191a1b` added names
  a feature already enabled. ADR-0019:24 and :150 are true. Delegated by
  Manuel, decided by Claude Code: no ADR change.
- **#2 — CLOSED, no skew.** `markdownlint-cli2-action@v23` resolves to 23.2.0,
  whose `markdownlint-cli2` is 0.22.1, the version CLAUDE.md pins locally.
  The tag floats within v23; a later v23 release could move it.
- **#14 — CLOSED.** handoff-27 recorded that no live instance remained; the
  one left is Class H-inline.
- **#54 — Stale destinations "roadmap §D" for rules over other classes.**
  Phase D delivered capture, delivery and storage, and no rule. This commit
  amends SPEC-006 (`:40`), SPEC-015 (`:25`, `:37`, `:38`; `:16` and `:110`
  stated) and SPEC-016 (`:43`) and restates `roadmap.md` §B1; `SPEC-018:56` is not stale (SPEC-019 `:10` moved it by
  scope). Still stale: `services/ingest/src/detect/engine.ts:30` and `:351`
  ("roadmap §D") and `rules/windows/README.md:5` ("rules over other event
  classes wait for roadmap §D"). Widened by the Amendments 2026-10-12 of
  SPEC-015 and SPEC-016 (rules over the command line and the user go to
  §B3): `engine.ts:22-23` and the refusal message at `:257` ("deferred to
  B2"), `eval-ac-003-fail-closed.test.ts:12-18`, which asserts `/B2/`, and
  `rules/windows/README.md:5` ("Rules over the command line or the user
  wait for B2"). Destination: B2's implementation branch, which corrects
  the refusal message anyway and runs the elevated gate; a change under `rules/` alone would void a
  green `detect_ac_001` run (CLAUDE.md *Developer-local SPEC-006 marquee
  validation*).

## Self-reviews

Each SPEC or ADR text had Claude Code's pass (claims made twice agree;
each `path:line` says what is claimed) and the fresh-context reviewer
with the fixed prompt. Every list-A entry was fixed before landing.

- **SPEC-017 Amendment 2026-10-11, the catalog edge, CLAUDE.md's gate**
  (`cf54945`, `a293c27`). List A 4. A1, the amendment admitted the
  callback's per-record `trace` line, which ADR-0009 and SPEC-005
  NFR-005-001 forbid: it is not restated, and it became debt #52. The
  other three were one-line fixes.
- **ADR-0012 Amendment 2026-10-11** (`be69547`). List A 1: §7's comment
  "the only class flowing in v0.1" (`:163`) was not dealt with; the
  amendment now classifies it.
- **SPEC-006, SPEC-015 and SPEC-016 Amendments 2026-10-11, roadmap §B1,
  this record** (`3b21b5f`), two passes. List A 5, then 3: this record
  still showed ADR-0012's amendment as unratified, its "(this commit)"
  markers, and the sweep of #54 missed sites (`SPEC-016:43`;
  `SPEC-015:16` and `:110`). SPEC-016 gained its amendment and the
  others were fixed.
- **ADR-0020 draft**, seven passes before the threat-model entry: list A
  9, 9, 7, 6, 5, 5 and 2, all resolved. Among them:
  - the parent at the edge broke `process.uid`, so it is split out (§5);
  - a SID in the `dedup_key` would outlive the 365 days, so it is left out;
  - ADR-0010's privilege detection: Kernel-Process opens first;
  - the join bound against batching: the slot design;
  - the read-model projects the fields;
  - Manuel's words are quoted literally (`dbb93b9` on `main`).

- **Roadmap §B3 and the SPEC-015 and SPEC-016 Amendments 2026-10-12.**
  Claude Code's pass: §B3 is the destination in roadmap §B3, Part (b) §8,
  both amendments and the catalog alike; the cited lines (`SPEC-015:32`,
  `:87`; `SPEC-016:42`, `:44`, `:45`, `:381`, `:435`) say what is claimed.
  The fresh-context reviewer (the fixed prompt), list A, 5 entries:
  - A1, the refusal message and `eval_ac_003` still name B2: stated in
    SPEC-015's amendment and recorded in #54.
  - A2, `rules/windows/README.md:5`: recorded in #54.
  - A3, the insertion shifted `SPEC-015:102`'s `:110`: corrected to `:118`.
  - A4, §C named the command line only: the user is added.
  - A5, ADR-0020 §Out of scope still sends the rules to §B2: corrected on
    its branch.

  List B, 8 entries:
  - B1, the reading of Manuel's answer: stated above and to him.
  - B2, §6's status: it closes with the ADR that answers it.
  - B3, "before its SPEC": now "before its SPEC is accepted".
  - B4, the user is a SID, not the account name of Sigma's `User`: stated
    in §B3.
  - B5, purpose limitation between B2 and B3: ADR-0020 §Consequences, on
    the branch.
  - B6, §B3's "Unblocks" and the references: added.
  - B7, the stale `read-model.ts` anchors in §B2 are corrected; the
    pointers to handoff-35 move at the close.
  - B8, the catalog's wording: clarified.
- **ADR-0020's branch after the threat-model entry** (`adr/s36-0020-cmdline-user`,
  NOT YET RATIFIED). Before it, seven passes on ADR-0020 and its co-located
  amendments gave list A 9, 9, 7, 6, 5, 5 and 2, all resolved. Three more,
  each the fixed prompt over the threat model and the six ADR files:
  - At `bf1b251`, list A 4. The threat model said a rule could key on
    three markers that detection did not read, so all three are now
    projected. The amendment claimed it changed nothing else while adding
    references, and said the data was "not among" §8's assets, though §8
    covers every event. Its new api entry clashed with §8's per-service
    users, which it now names. List B 12, dispositioned: kernel and
    complete memory dumps added; `raw_data` bounded; older Terminates read
    the marker's default; the resent copy's arrival; ADR-0012's
    half-resolved item; ADR-0009's wording; the threat-model reference
    wording. Left to SPEC-021: the truncation unit, the slot's lifetime,
    the exit-9 message, the column time-to-live's materialization. Left as
    is: the amendment dates (below), the stated limits of the evidence.
  - At `e7ba5f7`, list A 1: "a rule that keys on them matches nothing on an
    empty value" is false for a filter, which suppresses nothing (ADR-0012's
    amendment now says both). List B 16, dispositioned: the absent-field
    pattern of ADR-0011 §5 weighed (ADR-0020 A12); timestamp conversion
    named; "kept small", not "bounded", for the cache; the slot leaves at
    render; SPEC-016's section named; the api's shared ClickHouse user,
    which can write, stated; argument spoofing added; the markers usable
    once §B3 admits them; the rename named a breaking change and kept at
    0.1.0; the alternatives Manuel did not choose named so. The two left to
    SPEC-021 are SPEC-017 capture_ac_002 and the time-to-live's
    materialization.
  - At `fec2ee4`, list A 2. ADR-0006 §Compliance asks for a major version
    on a breaking change and was not amended: amended co-located, for 0.x
    with no outside consumer. ADR-0012's amendment called the `event_id`
    v4-or-v7 question open, though the code generates v7: reworded, and the
    stale schema description is debt #56. List B 15, dispositioned: §5's
    "no resolution flag" noted in ADR-0011's amendment; §8's query-log
    filtering named as not configured yet; the argument-spoofing citation
    corrected; "a row can still hold either empty"; `regex` is transitive;
    why the rundown does not close the startup gap; ADR-0003's future cold
    tier; the callback fills slots. Left to SPEC-021: the exit-9 message,
    LocalSystem, the races at the join bound, the time-to-live's
    materialization. Predating this change: the cache without its sweep if
    the hygiene thread fails.

  Manuel stopped the run at `fec2ee4` while it was working, and its report
  arrived after. The changes from `fec2ee4` to `b751bdc` have had no
  fresh-context pass. At landing, the co-located amendments carry the date
  of Manuel's ratification, as ADR-0011's Amendment 2026-10-04 carries
  SPEC-017's ratification date rather than its landing.

## Debts

New this session:

- **#52 — The process callback writes a `trace` line per record.**
  `agent/cg-agent/src/etw/session.rs` logs `dispatch callback fired` at
  `trace` for every record the Kernel-Process provider delivers. At the
  default level, `info`, it is not written. When `log.level` enables `trace`
  for the agent's ETW target, it is a line of output per event inside the
  dispatch callback, which ADR-0009 (§Consequences, §Compliance) and SPEC-005
  NFR-005-001 forbid. SPEC-017's Amendment 2026-10-11 does not restate it
  into §Operational §6. Destination: the next change under
  `agent/cg-agent/src/etw/` removes the line and corrects the module
  comments that say the callback does no I/O (`session.rs`, `dispatch.rs`,
  `ring.rs`).
- **#53 — The AC-006 line lacks `activity_id` and `event_id`.** SPEC-005
  AC-006 and its §Observability table require the `error` line of a process event
  dropped for an empty name to carry the `ProcessID`, the `activity_id`, the
  `event_id` and the reason; `agent/cg-agent/src/etw/ring.rs` writes `pid`
  and `reason` only, and `process_ac_006_name_log_and_drop.rs` asserts those
  two. Destination: the same change as #52.
- **#55 — The threat model describes the Phase-0 plan, not the system.**
  `docs/security/threat-model.md` (Accepted 2026-05-20, never amended) names
  assets and mitigations that the ADRs since changed: the agent's buffered
  events "encrypted at rest with a key derived from DPAPI" (the ring is in
  memory and ephemeral, ADR-0009 §3); an agent run as "a dedicated service
  account" (an elevated user in the MVP, ADR-0010 §1); `cg-ingest`
  publishing to NATS and protected by "Go strict typing" (TypeScript,
  ADR-0007, writing to ClickHouse with no NATS, ADR-0012 §2). It has no entry for the
  data about people the agent sends since roadmap §D: the logons of ADR-0019
  (account names, SIDs, source addresses, 365 days) and the connections of
  ADR-0018 (addresses and ports, by process). §8's "per-service database
  users with scoped privileges" are not the repository's state: the ingest
  service and the api share one ClickHouse user
  (`infra/dev/docker-compose.dev.yml:134`, `:207`); and its "query-log
  filtering of PII fields" is not configured. The threat model is Manuel's.
  Destination: an owner-review amendment after ADR-0020 lands; ADR-0020's
  branch adds only its own asset.
- **#56 — `alert.json` says the agent's `event_id`s are UUIDv4.** The
  description of `source_events` (`schemas/cges/v0.1/classes/alert.json:46`)
  reads "UUIDv4 today (Uuid::new_v4)"; the agent generates UUIDv7
  (`agent/cg-agent/src/etw/dispatch.rs:76`, `:224`;
  `agent/cg-agent/src/logon/mod.rs:168`; SPEC-017 §Data contracts). The
  pattern's re-narrowing stays open (SPEC-006 §Operational §5).
  Destination: B2's implementation branch, which changes the CGES schemas.
- **#57 — CLAUDE.md says a SPEC header has no `Last updated`.** *SPEC
  amendment workflow* describes a SPEC's header as `ID` / `Title` /
  `Status` / `Depends on` / `Authors`, with no `Last updated` field.
  SPEC-001 to SPEC-009 carry one (`docs/specs/SPEC-001-agent-heartbeat.md:9`
  and the next eight). The rule still holds for the later SPECs.
  Destination: a CLAUDE.md fact correction, Claude Code's (*Decision
  authority*), with the next CLAUDE.md change.
- **Note on #43** (handoff-35: events are not ordered across CPUs). Its
  premise is in doubt. ferrisetw 1.2's default `log_file_mode` includes
  `EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING`
  (`ferrisetw-1.2.0/src/native/etw_types.rs:243`), so a session may
  deliver in one buffer's order. To verify with SPEC-021's measurement.

Debts #1–#51 keep their live record in `docs/handoff-session-35.md` §Debts,
with this session's changes recorded above. Closed in this session:
debts #2, #14, #28, #44, #45, #48 and #49. New: #52 to #57.

## Decisions

Manuel's decisions this session are listed with his words under
*Progress* (2026-10-11: the ADR-0012 amendment, #49, the 1001 file, B2's
three decisions and 1′; 2026-10-12: the threat-model entry and §B3).

Claude Code's, **anticipated**:

- #44 closed by amending SPEC-017 §Operational §6 to what the code does,
  and not the code (`cf54945`). The alternative, changing the callback,
  was not needed: it fits ADR-0009 except for the `trace` line (#52).
- #45: the local gate lists each ts-ci job's steps (`a293c27`). The
  alternative, `task` targets, does not exist yet.
- #49 closed without amending ADR-0019 (delegated by Manuel). The
  alternative, an amendment, would have corrected a statement that is true.
- B2's sources measured with redacted scripts and a probe crate outside the
  repo (`C:\tmp`), before any contract. The alternative, tests in the repo,
  would have committed code for a question still open.
- The threat-model entry goes on ADR-0020's branch as its own commit
  (`bf1b251`), not on a branch of its own, so that one ratification covers
  the asset and the decision that creates it.
- The rules over the two fields go in roadmap §B3, with its owner-STOP
  §9 (`bdd1d74`); SPEC-015 and SPEC-016 are amended by scope, and the
  code's "B2" pointer goes to #54. The alternative, editing the code now,
  would need a branch and the elevated gate for a message.
- ADR-0020's design under its ratification: listed in its §Decision
  record (the join through a slot, the bounds, the markers, the 365 days
  of the SID, no user in the keys, the parent at the edge apart, no major
  version).

Claude Code's, **reactive**:

- The single kernel source (B2 decision 1) became 1′ after the third
  probe showed the kernel's event carries no image path (`cb82b39`).
- The field became `process.user.uid`, not `process.user_sid`: the newer
  classes use OCSF paths on the wire (review, A8).
- The parent at the edge was split out of ADR-0020 (review).
- Kernel-Process opens first, so that ADR-0010's privilege report stays
  its own (review).
- The read-model projects the fields, then all three markers, so that
  the threat model's mitigation holds (reviews).
- ADR-0006 amended for breaking changes while CGES is 0.x (review).
- Manuel's words re-quoted literally on `main` and on the branch
  (`dbb93b9`).

## Gate outputs and baselines

No code changed and no gate ran this session; the baselines are S35's
(`docs/handoff-session-35.md` §Test baselines): elevated, cargo
`--ignored` 7 passed and vitest `services/ingest` 60 files / 197 tests;
`services/api` 31 files / 62 tests. CI on every push to `main` was
`markdown-lint → success`, the other workflows not triggered by
documentation.

## What waits on Manuel

1. The fresh-context pass over `fec2ee4..b751bdc` (he stopped the run at
   `fec2ee4`): run it before landing, or say why not. Recommended: run it.
2. His "sí" to ADR-0020 and its co-located amendments: ADR-0006 (a breaking
   change keeps the version while CGES is 0.x with no outside consumer),
   ADR-0009, ADR-0011, ADR-0012, ADR-0019 and the threat model.
3. With SPEC-021: the closed list of secret patterns (ADR-0020 §2).
4. Roadmap Part (b) §9: which threats §B3's rules cover, before its SPEC is
   accepted.
5. #55: the threat model brought in line with the system.
6. Unchanged: Part (b) §1–§4, and §6 until ADR-0020 is Accepted.

## How Session 37 resumes

1. *Session protocol* §At the start. Expected: `main` = `origin/main` at
   this file's last commit; tree clean; no stash; one branch,
   `adr/s36-0020-cmdline-user` at `b751bdc`, no PR; catalogs ADR 19 / SPEC
   20; Known CI debt zero.
2. Ask Manuel the first two points of *What waits on Manuel*. For the
   pass, use the fixed SPEC/ADR prompt over `docs/security/threat-model.md`
   and `docs/adr/0006`, `0009`, `0011`, `0012`, `0019`, `0020` and
   `README.md` at the branch tip, and fix every list-A entry on the branch
   (marker kept) before landing. Offer him also to measure ADR-0020 Open
   questions 5 (step 4) before his "sí": one elevated probe run, and a
   positive result would remove the second session and the join from §1,
   which is better changed before ratification than amended after.
3. After his explicit "sí", land ADR-0020 (*Integration path* §3, §4):
   - rebase the branch onto `main` and push it with `--force-with-lease`;
   - squash it into one commit without the marker, with `git diff --cached
     --quiet <branch>` as the guard;
   - ADR-0020's Status becomes `Accepted`, with his words and the date in
     its §Decision record, and its catalog row `Accepted`;
   - `Last updated` in ADR-0020 and in each amended ADR gets the
     ratification date;
   - the headings of the co-located amendments that name ADR-0020 (and the
     threat model's) get the ratification date, together with every
     reference to them. ADR-0012's "Amendment 2026-10-11: classes 4001 and
     3002 …" and SPEC-017's Amendment 2026-10-11 are other amendments and
     keep their dates;
   - delete the branch, local and on origin;
   - CI on the pushed SHA.

   Then, on `main`: roadmap §B2 states the decided source and its status;
   Part (b) §6 is marked resolved.
4. SPEC-021, the contract of §B2's capture:
   - first, ADR-0020 Open questions 5: whether one session (a `UserTrace` in
     system-logger mode, with the kernel's process provider by GUID)
     delivers `CommandLine` and `UserSID`. Measure it with the probe crate,
     elevated, through Manuel; if it does, it is an amendment to ADR-0020
     before SPEC-021;
   - the closed list of secret patterns, for Manuel to ratify;
   - its amendments by scope (ADR-0020 §Consequences, Neutral);
   - #43's premise (note above) and the truncation unit.
5. Then the implementation, on a branch with a PR:
   - acceptance tests failing first;
   - CI on every push;
   - the elevated gate through Manuel;
   - the self-review;
   - landing by cherry-pick.

   It carries #52, #53, #54, #56 and #31.
6. Then roadmap §B3, after Part (b) §9.
