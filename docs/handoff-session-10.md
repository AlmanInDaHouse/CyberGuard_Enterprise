# Handoff — End of Session 10

Canonical state-of-the-world at the close of Session 10. This document is the
contract Session 11 resumes from, independent of memory compaction in Claude
Code or chat. Everything below is verified against `main`, not recalled.

- **Anchor commit:** `6b54f95` (`docs(spec): amend SPEC-003 (E2 wire shape, D1 narrowed, provenance cascade)`)
- **Branch:** `main`
- **Date:** 2026-05-23
- **CI verdict at anchor:** ALL GREEN
- **Known CI debt:** none
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## State of `main` at `6b54f95`

Session 10 settled the MVP architecture's plumbing: how events look on the
wire (SPEC-003 amendment 2026-05-23, E2 shape), how they're delivered
(ADR-0009), what dedup mechanism the server uses (ADR-0009 +
SPEC-005-pending), what privileges the agent needs (ADR-0010), what crate
the agent uses for ETW (ADR-0008). No implementation code shipped — every
commit is documentary; the implementation phase (the first concrete event
class, SPEC-005 process telemetry) is Phase 3, deferred to Session 11.

Seven atomic commits, all green on first push, no Known CI debt at any
point, no force-pushes, no amendments-of-amendments. The Phase 0 spike
validated empirically that ferrisetw 1.2.0 + a raw-Win32 side-channel
covers the agent's ETW consumption surface; the spike note at
`docs/spikes/2026-05-23-etw-process-events.md` carries the verification
record and the upstream-PR follow-up commitment.

Session 10 commit map (newest first):

```text
6b54f95  docs(spec): amend SPEC-003 (E2 wire shape, D1 narrowed, provenance cascade)
ec30bbd  docs(adr): accept ADR-0010 (agent privilege + installation posture for MVP)
8e296cd  docs(adr): accept ADR-0009 (events delivery + buffer) + amend ADR-0004
195a3c9  docs(engineering-notes): formalise three-role ADR Deciders convention
265b771  docs(adr): accept ADR-0008 -- ETW crate selection (ferrisetw 1.2.0 + side-channel)
bd3fce3  docs(engineering-notes): Session 10 -- follow-ups co-located with generators
dea00ff  docs(spikes): Phase 0 ETW Kernel-Process spike findings for SPEC-005
```

CI workflows for `6b54f95`:

```text
markdown-lint     → success
rust-ci           → skipped (path filter; no agent/ changes in this commit)
ts-ci             → skipped (path filter; no services/ingest/ changes)
schema-validation → skipped (path filter; no schemas/ changes)
```

Same skip pattern across the seven Session 10 commits — none touched
`agent/`, `services/ingest/`, or `schemas/`, so only `markdown-lint`
exercised on every push. The rust-ci and ts-ci suites stand at their
Session 9 GREEN baseline (last exercised at SHA `f407c05`); they will
re-fire on the first Phase 3.5 implementation commit and are expected
to remain green there because no architectural change in Session 10
contradicts the SPEC-001/002/003/004 contracts they validate.

## Phases completed in Session 10

- **Phase 0 — Spike** (`docs/spikes/2026-05-23-etw-process-events.md`).
  Three gates GREEN on a Windows 11 elevated PowerShell: REQ-A.1 (clean
  Drop releases the session), REQ-A.2 (force-kill leaves a reclaimable
  zombie; raw `ControlTraceW(STOP)` reclaims on next start), REQ-B
  (lost-events side-channel is falsifiable — 7649 lost in 25 s under
  1 KB × 2 buffers + 80 ms in-callback sleep + three 200-process
  bursts, monotonic). CI gate explicitly skipped by chat decision and
  carried forward as accepted risk in ADR-0010 §Decision part 3.
- **Phase 1.1 — ADR-0008** (ETW crate selection). ferrisetw 1.2.0 for
  session lifecycle + parsing; raw-Win32 side-channel for `events_lost`
  and `stop_zombie` (two ~30-line helpers) covering the published-1.2.0
  gaps. Three gaps documented honestly in the ADR (the third is the
  cosmetic `TraceError` non-`std::error::Error` impl).
- **Phase 1.2 — ADR-0009 + ADR-0004 amendment** (events delivery +
  buffer model). At-least-once delivery, server-side dedup keyed on
  `event_id` (UUIDv7), in-memory ring buffer in the agent (ephemeral,
  FIFO drop, `events_dropped_total` observable), ClickHouse
  ReplacingMergeTree on `event_id`. Persistent disk-backed buffer
  deferred to a future SPEC. Co-located ADR-0004 amendment 2026-05-23
  (triple scope: (a) buffer model deferred; (b) `sequence_number`
  persistence consequence retired, closing SPEC-003 §Drift D3 in
  ADR-0004's prose; (c) stale `ADR-0008 (contract generation tooling)`
  cross-reference corrected).
- **Phase 1.3 — ADR-0010** (agent privilege + installation posture for
  MVP). Elevated user process; no Windows Service in MVP; CI assumption
  carried as accepted risk with two named fallback paths (SYSTEM
  trampoline via psexec, or move marquee AC out of CI). ADR-0008
  `(forthcoming)` qualifier dropped in the same commit since the
  forward-pointer is no longer forward.
- **Phase 2 — SPEC-003 amendment** (E2 wire shape + D1 narrowed +
  four-item provenance cascade). Outer signed envelope gains `events`
  and `batch_hash`; signed region grows to include `batch_hash`; body
  remains inline (D1 narrowed, not retired). Eleven in-place rewrites
  across the document. Empty-array `batch_hash` constant
  `4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945`
  verified end-to-end via three independent tools (Python `hashlib`,
  OpenSSL `dgst`, agent's actual Rust `serde_jcs 0.1 + sha2 0.10`
  stack) at amendment-drafting time.

## ADRs at session close

All `Accepted`. Full catalog and dependency graph in
[docs/adr/README.md](adr/README.md).

| ADR | Title | Status / Session 10 note |
|---|---|---|
| [0001](adr/0001-monorepo-layout.md) | Monorepo layout | unchanged |
| [0002](adr/0002-language-per-component.md) | Language per component | unchanged (still amended-in-part by 0007 for `services/ingest/`) |
| [0003](adr/0003-polyglot-storage.md) | Polyglot storage | unchanged |
| [0004](adr/0004-agent-server-protocol.md) | Agent-Server secure protocol | **amended-in-place 2026-05-23 (triple scope: parts (a)/(b)/(c))**; `Last updated: 2026-05-23`; status stays `Accepted` |
| [0005](adr/0005-detection-rules-and-ml-in-parallel.md) | Detection — rules and ML in parallel | unchanged |
| [0006](adr/0006-cges-ocsf-alignment.md) | CGES alignment with OCSF v1.3 | unchanged this session; **pending amendment in Phase 3.1** for first concrete event class (Process Activity), or promotion to ADR-0011 if D2 sub-decisions turn non-trivial |
| [0007](adr/0007-ingest-language-typescript-mvp.md) | Ingest language — TypeScript for MVP | unchanged |
| [0008](adr/0008-etw-crate-selection.md) | ETW crate selection for Windows event capture | **Accepted this session**; ferrisetw 1.2.0 + raw-Win32 side-channel for the two documented gaps |
| [0009](adr/0009-event-delivery-and-buffer.md) | Event delivery semantics and agent buffer model | **Accepted this session**; at-least-once + UUIDv7 dedup + in-memory ring + persistent buffer deferred |
| [0010](adr/0010-agent-privilege-model-mvp.md) | Agent privilege model and installation posture for the MVP | **Accepted this session**; elevated user process, no Service yet, CI assumption + 2 fallback paths |

ADR-0004 amendment 2026-05-23 details (because it interacts with multiple
other artifacts):

- **Part (a) — buffer model deferred.** §Heartbeat and degraded mode buffer
  bullets and §Consequences > Negative "encrypted local buffer" bullet
  superseded by ADR-0009. Persistent disk-backed buffering reserved for a
  future SPEC.
- **Part (b) — `sequence_number` persistence consequence retired.** Three
  in-place rewrites close the Session 7 SPEC-003 §Drift D3 declaration
  that never landed in ADR-0004's prose (per-process monotonicity; step 4
  informational only; anti-replay enumeration loses `sequence_number`).
- **Part (c) — stale ADR-0008 cross-reference corrected.** ADR-0008 was
  reserved at the time of ADR-0004's writing for "contract generation
  tooling"; the §Consequences > Neutral bullet that pointed at the
  reserved meaning is dropped; API-versioning policy stays a future
  concern (future ADR cites 0004, not the reverse).

## SPECs at session close

All `Accepted`. Catalog in [docs/specs/README.md](specs/README.md).

| SPEC | Title | Status / Session 10 note |
|---|---|---|
| [001](specs/SPEC-001-agent-heartbeat.md) | Agent heartbeat | unchanged |
| [002](specs/SPEC-002-agent-enrollment.md) | Agent enrollment | unchanged |
| [003](specs/SPEC-003-mtls-signed-envelope.md) | mTLS 1.3 and signed envelope | **three amendments now**: 2026-05-22 (a) separate enroll and heartbeat URLs; 2026-05-22 (b) relax `server.url` scheme constraint; **2026-05-23 triple scope (E2 wire shape + D1 narrowed + four-item provenance cascade)**; `Last updated: 2026-05-23` |
| [004](specs/SPEC-004-server-ingest-minimal.md) | Server ingest minimal | unchanged |

The 2026-05-23 amendment's empty-array `batch_hash` constant — the value
SPEC-005's test fixtures will hardcode for heartbeats with no events —
is `4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945`,
verified end-to-end via Python `hashlib`, OpenSSL `dgst`, and the
agent's actual `serde_jcs 0.1 + sha2 0.10` Rust stack at amendment
write time. Wire version `outer_envelope_version` stays `"0.1.0"`;
future bumps reserved for semantic incompatibility (signature algorithm
change, field deletion, type change of an existing field).

SPEC-005 (Agent process telemetry — Windows ETW Kernel-Process) is
forthcoming in Phase 3.3; it is the first consumer of ADR-0008, ADR-0009,
ADR-0010, and the SPEC-003 E2 shape.

## Engineering-notes Session 10 — nine durable conventions consolidated

Authoritative text in [docs/engineering-notes.md](engineering-notes.md)
under `## Session 10 (2026-05-23)`. Summarised here so Session 11
inherits them by reference:

1. **Follow-ups co-located with the document that generates them.** No
   central `docs/follow-ups.md` index; spike's follow-ups live at the
   spike, an ADR's at the ADR, an amendment's at the amendment.
2. **ADR Deciders field — three-role convention formalised.** Every ADR
   header uses `- Deciders: Manuel (project owner), Claude (architecture
   advisor), Claude Code (implementation)` verbatim.
3. **Cross-reference convention — provenance points to the cause, not
   the effect.** When an amendment closes a prior drift, rewrite-side
   bullets cite the drift's original declaration; the amendment is the
   closure, not the origin.
4. **Internal cross-check for multi-mention claims.** Any document where
   a load-bearing claim appears in more than one place needs an
   internal-consistency pass before submission, not only when amending.
5. **Audit cross-references on reserved-ADR repurposing.** When a
   reserved ADR number is ratified for a new topic, sweep the repo for
   cross-references written under the old reservation — lexical audit
   scoped to amendment terms cannot catch these.
6. **Dependency-graph edge convention — "amends in part" vs
   "supersedes".** Amends-in-part preserves the target's `Accepted`
   status; supersession is reserved for cases where the target's status
   flips to `Superseded`.
7. **Dep-graph edges capture constraint-bearing relationships only.**
   Non-constraining but topic-relevant relationships live as bullets in
   the ADR (typically §Consequences > Neutral), not as edges. Do not
   invent edges to document absences.
8. **Spike notes are point-in-time records, not living documents.**
   Their text is anchored to the date in the filename; semantic updates
   after that date are documentary revisionism, not hygiene.
9. **Bidirectional cross-reference audit on amendment.** When document
   X is amended, audit cross-references **to** X from other documents in
   the same session — they may need provenance updates. Procedural
   footnote: cross-references detected during drafting (not during the
   formal audit pass) should be surfaced explicitly in the draft for
   chat approval before being silently applied — the audit pass is the
   moment of accountable scope-setting; corrections found later need
   explicit retroactive scope-extension.

## D-decisions status

The seven Session-10-opening D-decisions (D1–D7 as ratified by Manuel
in chat for SPEC-005's scope), tracked against the artifacts that
realized them.

| D | Topic | Status | Realized by |
|---|---|---|---|
| D1 | ETW crate (ferrisetw + side-channel) | **DONE** | ADR-0008 + Phase 0 spike |
| D2 | CGES Process Activity mapping + 4 sub-decisions (event_id capture-time, no cg_raw_ref, single schema with activity_id discriminator, ETW field subset) | **PENDING** | Opens Phase 3.1 (ADR-0006 amendment or ADR-0011) |
| D3a | At-least-once delivery + UUIDv7 `event_id` dedup | **DONE** | ADR-0009 §Decision part 1 + part 2 |
| D3b | In-memory ring buffer; persistent disk buffer deferred | **DONE** | ADR-0009 §Decision part 3 + part 4 + ADR-0004 amendment part (a) |
| D4 | Agent privilege + installation posture for MVP | **DONE** | ADR-0010 |
| D5 | Envelope shape E2 (events[] + batch_hash) + SPEC-003 amendment audit | **DONE** | SPEC-003 amendment 2026-05-23 part (a) (E2 shape) + part (b) (D1 narrowed) |
| D6 | ClickHouse `cges_events` single table with `class_uid` discriminator + partition/order tuple correction | **PARTIALLY DONE** | Decision ratified; exact DDL pending in SPEC-005 (the ReplacingMergeTree engine choice from D3a is locked by ADR-0009) |
| D7 | Marquee timeout 45 s + elapsed-time logging in CI | **PENDING** | Opens Phase 3.3 (SPEC-005 AC-001) |

## Phase 3 opening protocol — Session 11

The remaining work to ship SPEC-005's first concrete event class
(Process Activity from `Microsoft-Windows-Kernel-Process`). Five
sub-phases, sequenced; the chat-review gates between each are
mandatory per the standing ask-first contract for new/amended
ADRs and SPECs.

- **Phase 3.1 — ADR-0006 amendment for first CGES event class
  (Process Activity).** Lexical audit of ADR-0006 first (Session 9
  lesson at maximum load — third application this session would be
  fourth in series), then chat reconciliation, then amendment text.
  D2's four sub-decisions get drafted here; if PPID modeling under
  parent-process-race-condition, `process.uid` recipe from PID +
  creation timestamp + LUID, or command-line PII policy turn
  non-trivial, **promote to a dedicated ADR-0011** rather than
  expand ADR-0006 unboundedly. The promotion decision is a chat
  gate inside Phase 3.1.
- **Phase 3.2 — CGES schema JSON** at
  `schemas/cges/v0.1/process_activity.json`. JSON Schema draft
  2020-12. Hand-crafted sample event for validation. Chat-review
  before the harness depends on the shape.
- **Phase 3.3 — SPEC-005 full draft** (not the outline from
  Session 10's grouped briefing). All nine ACs concretised. AC-001
  marquee with the 45 s threshold + elapsed-time logging per D7's
  addition. Dispatch-callback NFR cites the spike's empirical
  evidence (7649 lost under 80 ms callback + 1 KB × 2 buffers). All
  Out-of-scope items explicit (other ETW providers, other event
  types, persistent buffer, packaging, cross-platform, PII
  redaction). Status `Proposed` until chat ratifies.
- **Phase 3.4 — Harness RED.** All nine ACs as failing tests
  **before** any implementation lands. Per the invariant: the
  marquee never downgrades to mocks; the harness uses real `cg-agent`,
  real `services/ingest/`, and real Postgres/ClickHouse/Redis via
  testcontainers, with the documented escalation order if
  testcontainers can't reach the Docker socket.
- **Phase 3.5 — Implementation to GREEN.** Real ETW capture via
  ferrisetw (ADR-0008), real envelope E2 with `batch_hash` over
  `events` (SPEC-003 amendment 2026-05-23 part (a)), real ingest
  persistence in `cges_events` (ADR-0009 + D6). Marquee end-to-end.
  This is the first commit in Session 11 that exercises both `rust-ci`
  (agent ETW + envelope code) and `ts-ci` (server `events` insert
  path); both expected to come up green.

## Known follow-ups

Co-located with their generating document per Convention 1; surfaced
here as a single index for Session 11's planning.

- **Upstream PR to ferrisetw** exposing `events_lost()` and
  `stop_if_exist` (or its equivalent) — combined PR, same crate, same
  maintainer review flow. Trigger: after SPEC-005 is `Accepted`.
  Recorded in
  [docs/spikes/2026-05-23-etw-process-events.md](spikes/2026-05-23-etw-process-events.md)
  §Follow-ups.
- **Persistent disk-backed encrypted buffer subsystem** — at-rest
  encryption (DPAPI on Windows in MVP; Linux/macOS platform keyrings
  separately deferred per ADR-0002 Rule 2), file format and rotation,
  crash recovery, replay-on-reconnect dedup interaction, renewed
  cross-restart `sequence_number` persistence. Deferred per ADR-0009
  §Decision part 4 + ADR-0004 amendment 2026-05-23 part (a); reserved
  for a future SPEC.
- **Windows Service / packaging SPEC** — MSI/NSIS/WiX installer,
  service registration (`sc create` or `windows-service-rs`), service
  lifecycle handlers, log rotation / Event Log integration,
  auto-update. Deferred per ADR-0010 §Decision part 2; supersedes
  ADR-0010's installation-posture portion when it lands.
- **CI privilege validation** (`windows-latest`'s `runneradmin` can
  open Kernel-Process) — deferred per ADR-0010 §Decision part 3; the
  SPEC-005 AC-001 marquee's first CI run is the free first-use
  validation. If the assumption is contradicted, fallback path 1
  (SYSTEM trampoline) or fallback path 2 (move marquee out of CI)
  fires; choice deferred until that point.
- **API-versioning ADR** — provisional, only if a future need
  surfaces. ADR-0004's §Consequences > Neutral bullet on `/v1/` no
  longer pre-attributes this work to any specific ADR (the stale
  ADR-0008 reference was dropped under ADR-0004 amendment part (c)).
  If a future ADR addresses API-versioning, it cross-references
  ADR-0004; ADR-0004 stays the reverse-pointer-free side.

## Invariants carried into Session 11

Binding for every session. Authoritative text in
[CLAUDE.md](../CLAUDE.md) + Session-10 additions in
[docs/engineering-notes.md](engineering-notes.md) §Session 10.

- **Spec-Driven Development with harness-first mandatory.** No
  implementation code before its SPEC/ADR is `Accepted`; in Phase 3.5,
  the harness lands RED in Phase 3.4 before any agent/server code
  changes for SPEC-005.
- **Spanish in chat between Manuel and architect-Claude; English in
  all repo artifacts (ADRs, SPECs, schemas, code, commit messages,
  engineering-notes, handoffs, spike notes).** Architect-to-Claude-Code
  prompts go in English per the Session 10 multilingual-LLM-performance
  research note.
- **Single build runner: Taskfile.yml.** No Makefile, no per-component
  scripts that duplicate task targets.
- **Marquee never downgrades to mocks or in-process fakes.** Escalation
  order if testcontainers cannot reach the Docker socket:
  testcontainers → image pin → GHCR mirror → backends started via
  `task dev:up` by test setup.
- **Post-push CI poll mandatory.** Poll every workflow for the pushed
  SHA to a terminal state; RED blocks closure unless Known CI debt is
  declared in the same SHA that turned the workflow red. `gh` primary,
  REST API fallback via the git credential helper (this session used
  the REST API throughout — `gh` not installed).
- **Decision reporting — anticipated vs reactive.** Reactive
  architectural decisions get paragraph-level explanation. Expect
  Phase 3.1 and Phase 3.5 to surface more reactive than Session 10's
  documentary work did.
- **SPEC and ADR amendments in-place** with `## Amendment <date>:`
  heading; status stays `Accepted`; `Last updated` bumped. Amendments
  of `Accepted` ADRs and SPECs are ask-first.
- **Session 9 lesson applies at maximum load:** lexical audit before
  any amendment text, on every touched key, in every document being
  amended. Phase 3.1's ADR-0006 amendment is the next test of this
  discipline.
- **All nine Session 10 conventions apply** as listed above (and as
  recorded in `engineering-notes.md`).

## Approved local toolchain

Unchanged from Session 9. Authoritative table in
[CLAUDE.md](../CLAUDE.md) §Approved local toolchain. Anticipated next
addition: none required by Phase 3 (SPEC-005 stays Windows + Rust +
TypeScript, all already installed). A Go toolchain still anticipated
when the event-firehose ingest begins, which is **not** Phase 3 — that
is the unnumbered future SPEC for the high-throughput firehose ingest,
deferred per ADR-0007 §Consequences.

## How Session 11 resumes

1. Read this document and [CLAUDE.md](../CLAUDE.md) first; they are the
   binding contract. The nine Session 10 conventions in
   [engineering-notes.md](engineering-notes.md) §Session 10 carry
   forward and must be honoured from the first commit.
2. Confirm `main` is at `6b54f95` and all CI is green before starting.
   `git status` should show working tree clean (modulo
   `.claude/` which is pre-existing local config, untracked).
3. Open Phase 3.1 (ADR-0006 amendment for first CGES event class).
   First action: lexical audit of ADR-0006 against the D2 sub-decision
   keys (process.pid, process.name, process.file.path,
   process.parent_process.pid, process.cmd_line, process.user.uid,
   process.created_time, process.exit_code, plus the `cg_*` extensions
   from ADR-0006 §Decision). Audit-first protocol per Session 9 lesson,
   then chat reconciliation, then amendment text.
4. Claude Code's auto-memory directory carries narrative; this document
   and [engineering-notes.md](engineering-notes.md) carry the facts; the
   repo at `main` is the ultimate source of truth. Where any two of
   these disagree, re-verify against `main`. The auto-memory's exact
   filesystem path depends on the host; consult Claude Code's
   documentation if needed.
