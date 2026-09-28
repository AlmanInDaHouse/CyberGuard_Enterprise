# Handoff — End of Session 31

Full state-of-the-world at the S31 close. Written so a cold or compacted session
recovers the thread in one read.

Session 31 delivered roadmap **Phase C** — MVP criterion 1, ten detection rules —
as a new SPEC (**SPEC-016**, which amends SPEC-015 and SPEC-006 by scope) plus
its implementation: six commits cherry-picked to `main`, one per push. It also
added a phase to the roadmap: **G**, agent capture on the normal run path. No new
ADR. Catalogs: ADR 17 / SPEC 16. Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `228c5a3` | `docs(spec-016)`: SPEC-016 Accepted; catalog row and edge; roadmap §C, new §G, D and B2 dependencies; CLAUDE.md marquee validity extended to `rules/`. |
| `6f2ad9a` | `feat(ingest)`: rule loader contract (C1). |
| `33ba7f3` | `fix(ingest)`: resolve the parent per child (C4). |
| `c64172d` | `test(ingest)`: detect_ac_001 asserts on its own rule (C5). |
| `72a40db` | `feat(rules)`: nine process_creation rules (C2). |
| `d68655a` | `test(harness)`: scenario runner and SC011–SC019 (C3). |
| `ed391aa` | `chore(ingest)`: drop stale detection text (C6). |
| (this commit) | handoff-31 + README scorecard and roadmap refresh. |

Seven pushes, one per commit, CI green on each: `228c5a3` with **markdown-lint**;
the six code commits with **ts-ci**, and with **markdown-lint** wherever a
commit touched Markdown. No run needed a retry.

## SPEC-016

- **Owner delegation.** Manuel delegated all of C explicitly ("elige tú",
  2026-09-27): the ten rules, the definition of done, and the contract. The
  advisor decided; Manuel's ratification of SPEC-016 ratified the decisions
  (its §Ratification record).
- **Why a new SPEC that amends by scope** (precedents SPEC-015 → SPEC-006 and
  SPEC-011 → SPEC-010): C realises SPEC-006's "Full detection bar", hardens
  SPEC-015's rule document, and changes SPEC-006's parent resolution — planned
  extensions, not contradictions.
- **The ten decisions (brief):** (1) a new SPEC amending by scope; (2) the ten
  rules, six of which do not need the parent; (3) criterion 1 is done when each
  rule has a wired fixture and at least one CI scenario, scenario isolation
  holds, and the elevated marquee is green; (4) original rule content only;
  (5) the loader contract, with `cg.cg_detection_source` as the key; (6) parent
  resolution per child, with a termination check and a 24 h look-back
  constant, no environment variable; (7) incident grouping unchanged, reopened
  at roadmap §E; (8) multi-hop lineage out of the MVP; (9) the scenario runner
  is a vitest suite in `services/ingest`, and `ts-ci` runs on `rules/**` and
  `harness/**`; (10) doc-only gate first.
- **Transport.** SPEC-016 with its three companion doc edits, and later C6,
  reached Claude Code as patch files handed over by Manuel instead of text
  through the chat relay, and were verified by SHA-256 (relay rule 2).

## Phase C — delivered

- **C1 (`6f2ad9a`) — the loader contract** (SPEC-016 §Data contracts §1):
  `rule.<file name>` ids, unique; product `windows`; `level` ↔ OCSF
  `severity_id`; `cg_detection_source: rule`; strict `cg` and `cg_mitre`; ATT&CK
  tactic names and technique ids, none repeated; no `\\` in a value; a
  condition that needs at least one block to match. `loadRules` loads in
  file-name order and prefixes the file name to each rejection. The office rule
  gains `cg_detection_source: rule`. New `rules_ac_001`.
- **C4 (`33ba7f3`) — parent resolution per child** (§Operational §1): the most
  recent Launch of the parent pid on the same agent within
  `PARENT_LOOKBACK_SECONDS` (24 h) before the child, unless that process's own
  Terminate (same `process_uid`) precedes the child. Two queries per batch,
  resolved per child. `DetectConfig.correlationWindowSeconds` and
  `CORRELATION_WINDOW_SECONDS_DEFAULT` are removed: nothing read them once the
  join had its own look-back. New `rules_ac_004`.
- **C5 (`c64172d`) — detect_ac_001 asserts on its own rule** (§Operational §3):
  one `rule.office_spawns_script_host` alert, with `status`, the exact
  `dedup_key` and the child's `event_id` in `source_events`; it logs the other
  rules' alerts and the captured `image_file_name` of the probe and its child.
- **C2 (`72a40db`) — the nine rules and the ten fixtures**; new `rules_ac_002`.
- **C3 (`d68655a`) — SC011–SC019 and the scenario runner** (`rules_ac_003`);
  `ts-ci` triggers on `rules/**` and `harness/**`.
- **C6 (`ed391aa`) — stale text in files C touched:** the `read-model.ts` and
  `detect_ac_001` headers, and `harness/README.md` (it described a Go runner and
  a `manifest.yml` layout that do not exist).

**Order.** On the first review branch C2 and C3 came before C4, so C2's rule
descriptions claimed the per-child resolution one commit before it existed.
Claude Code reported it; the branch was rebuilt as `review/s31-c-rules-v2` in
the order above, each commit keeping its patch-id (`git patch-id --stable`) and
the final tree unchanged, with C6 on top.

**Gate.** Typecheck, lint and the non-elevated suite on every new tree. The
elevated marquee was run by Manuel on the reviewed tip `34799a9`, after
`cargo build --release -p cg-agent`: **41 files / 152 tests** green.

**First observed path form.** The marquee logged the captured images of the
probe and its child:

```text
\Device\HarddiskVolume3\Users\manul\AppData\Local\Temp\cg-detect-probe-Lyk6HK\winword.exe
\Device\HarddiskVolume3\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
```

The agent emits the NT device form, untranslated (debt #22). SPEC-016 §Context
fact 3, inferred at `8a0ed9f`, is now observed; every rule matches both forms.
No other rule fired during the 40 s system-wide capture.

## Test baselines

Non-elevated (the full suite minus `spec-005-marquee` and
`detect-ac-001-marquee`): **39 files / 150 tests**. Elevated: **41 / 152**. On
Windows without elevation the two marquees run and fail; their `skipIf` only
skips non-Windows platforms.

## Notes for the record

- **`rules_ac_006` nuance.** SPEC-016 allowed only `rawRule` and detect_ac_001
  to change among the existing tests; `test/helpers/detect.ts` changed too,
  mechanically (the removed field), with no assertion touched. Accepted.
- **Sigma tags.** The nine new rules use hyphenated ATT&CK tags
  (`attack.defense-evasion`); the office rule keeps `attack.initial_access`.
  The loader does not read tags.
- **Fixtures.** The near-miss requirement of SPEC-016 §Data contracts §3 is
  checked in review, not by `rules_ac_002`.

## Owner-STOP decisions pending (waiting on Manuel)

Unchanged from handoff-30, minus the choice of C's rules (resolved by delegation
into SPEC-016): ADR-0002 Go→TS reconciliation; the criterion-7 deployment
contract; forensic trust anchoring; B2 capture source; the compose basename
collision #12; amending SPEC-004 with `INGEST_DETECT_*` (#17); and the optional
items of handoff-29 (publishing the local tag; the Decision-authority process
note). New for G:

- **An unelevated agent on the normal run path.** Once capture moves there, an
  agent without elevation exits with code 9 (SPEC-005 AC-002) instead of
  sending heartbeats — a deployment-contract change (roadmap §G).

## Debts

- **#1–#11:** unchanged — see [handoff-session-26.md](handoff-session-26.md)
  §Debts.
- **#12–#14:** unchanged — see [handoff-session-27.md](handoff-session-27.md)
  §Debts. #13 gains stale anchors: `services/ingest/src/detect/types.ts` cites
  `RuleMatch.severityId` at `:87`, and ADR-0017 and SPEC-014 cite
  `incidents.ts:84` for the `GREATEST` severity update — the same pattern as
  their other `incidents.ts` / `index.ts` line anchors.
- **#17–#19:** unchanged — see [handoff-session-28.md](handoff-session-28.md)
  §Debts. **#20:** see [handoff-session-29.md](handoff-session-29.md).
  **#21:** see [handoff-session-30.md](handoff-session-30.md); it also covers
  the other `detect_ac_*` headers that still describe the harness-first RED.
- **#22 — No device-path → Win32 translation.** SPEC-005 §Operational §3 and
  ADR-0011 §4 row 3 require the Win32 form; the agent copies ETW's `ImageName`
  unchanged, while `agent/cg-agent/src/etw/types.rs` says the translation
  happens at emit time. Observed in S31 (above). Destination: roadmap §G.
- **#23 — Capture only in test mode.** Only `run_test_mode`, reached with
  `CG_AGENT_TEST_MODE=1`, opens the ETW session; its loop drops a drained batch
  on a transient send failure (against ADR-0009 §1), sends nothing while no
  events are drained, and skips the going-offline handshake. Destination:
  roadmap §G.
- **#24 — No "per-org configurable" window exists.** ADR-0012 §8 and SPEC-006
  NFR-006-003 describe per-org configurability; the dedup bucket and the parent
  look-back are constants. Destination: roadmap §F.
- **#25 — Clients per call in the detection cycle.** `upsertAlert`,
  `upsertIncident` and the watermark helpers open a `pg.Pool` per call, and the
  read-model a ClickHouse client per batch, although the driver amendment
  (ADR-0012 Amendment 2026-06-07, §Decision) says the driver shares the service's
  clients. The cost grows with the number of matches.
- **#26 — Incident MITRE and escalation.** An incident's `cg_mitre` keeps the
  first alert's techniques, so the PDF shows them partially when later alerts
  share the tactic set but not the techniques; a severity raise sends no email.
  Destination: roadmap §E.
- **#27 — `ac-001-marquee` flake.** The SPEC-004 marquee, which runs in CI too,
  reads heartbeats `ORDER BY arrived_at` and expects the first to carry
  `sequence_number` 1; it failed once locally in S31 and passed on every rerun,
  in isolation and in CI.
- **#28 — `.github/workflows/README.md` is stale.** It lists only
  `markdown-lint` as active and plans `harness.yml` and `rules-test.yml`, whose
  role `ts-ci` now covers through `rules_ac_002` and `rules_ac_003`.

Known CI debt: ZERO rows.

## How Session 32 resumes

1. Read this handoff, the prior handoffs and CLAUDE.md; confirm the `main` tip.
2. Next by dependency: **G** (agent capture on the normal run path, with the
   device-path → Win32 translation). First step: Claude Code audits both run
   paths read-only; the advisor drafts G's SPEC (a successor to, or an
   amendment of, SPEC-005). The unelevated-agent behaviour is an owner-STOP.
3. G's gate is the SPEC-005 marquee pointed at the normal run path, plus the
   SPEC-006 marquee (changes under `agent/` invalidate a green run).
4. After G: **D** (network 4001 + login 3002), then **B2** (which also carries
   the parent stamped by the agent), **E** and **F**.
