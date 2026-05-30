# SPEC-006: Detection MVP — process-rule pipeline (read-model, one Sigma rule, scoring, alert emission)

- **ID:** SPEC-006
- **Title:** Detection MVP — process-rule pipeline
- **Status:** Accepted
- **Depends on:** ADR-0012 (the contract this SPEC derives from — not re-decided here), ADR-0002 (services/pipeline Go target, amended for the MVP slice), ADR-0003 (alerts → Postgres-only, amended), ADR-0005 (detection philosophy, scoring, detection_source, harness obligation), ADR-0006 (CGES/OCSF; Alert/score/mitre classes), ADR-0009 (ReplacingMergeTree at-least-once → FINAL reads), ADR-0011 (Process Activity 1007 it reads), SPEC-004 (the ingest service the slice is hosted in), SPEC-005 (the producer of the `cges_events` rows it consumes)
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-30
- **Last updated:** 2026-05-30

## Motivation

SPEC-006 is the first SPEC under which CyberGuard *detects* rather than merely captures and persists. It is the production realisation of ADR-0012 (normalize-before-correlate pipeline). It reads CGES Process Activity events (OCSF class_uid 1007) from ClickHouse `cges_events`, normalizes them into a rule-evaluable shape, evaluates one Sigma-style detection rule, scores the match, and emits an alert into the Postgres `alerts` table.

This SPEC re-decides nothing from ADR-0012. It references ADR-0012's jurisprudence (event source, scoring renormalization, dedup mechanism, alert storage, the Go-vs-TS seam, the two flagged contradictions) and derives the verifiable acceptance criteria, the concrete Sigma rule, and the operational specification.

The marquee (detect_ac_001 below) is the detection architecture's reason-to-exist: a real `cg-agent` captures a real parent→child process spawn via ETW, the events flow through ingest into ClickHouse, the detection slice reads them, the rule matches, and exactly one alert lands in Postgres — end-to-end. Per ADR-0012 §1, the detection slice is hosted transitorily in TypeScript at `services/ingest/src/detect/`; per ADR-0012 §6 the alert lands in Postgres only.

## Scope

### In scope

- A read-model that polls `cges_events` forward by a `time` watermark, collapsing at-least-once duplicates with `FINAL` per ADR-0012 §2 + §7.
- A normalize step that maps the realized `cges_events` column names to a rule-evaluable record (Sigma `process_creation` field shape), including resolution of the parent image via a self-join on `process_parent_pid` (the parent image is not a stored column — see §Operational).
- **One** Sigma detection rule for the MVP: *Office application spawns a script host* (MITRE T1059 / T1566), keyed exclusively on populated v0.1 fields (`process_name` / `image_file_name` for the child, the joined parent image, `activity_id`). See §Data contracts.
- A minimal Sigma-subset rule evaluator sufficient for that rule (selection on `Image` + `ParentImage` equality/lists). The general Sigma-to-Go engine is out of scope per ADR-0005 + ADR-0012 §1.
- A scorer implementing ADR-0012 §4 renormalization-over-present-sources: rule-only ⇒ `final_score = heuristic_score`.
- Alert emission into the Postgres `alerts` table (Kysely migration `0002_alerts`, shape per ADR-0012 §6), with declarative bucket-in-key dedup (`INSERT … ON CONFLICT (dedup_key) DO NOTHING`).
- Two harness scenarios per ADR-0005 §Harness obligation: SC001 (positive, `detection_source = rule`) and SC010 (false-positive, no alert).
- A test per AC under `services/ingest/test/`, named `detect_ac_NNN_*`; the marquee (detect_ac_001) uses the real agent + testcontainers per the harness-first invariant.

### Out of scope

Inherited verbatim from ADR-0012 §Out of scope, each with its named destination:

- **Event firehose / NATS.** Deferred to the firehose ADR; triggers the §1 seam extraction to Go.
- **Enrich stage.** Future SPEC of `services/pipeline/`.
- **Stateful multi-event correlation** beyond the single-batch + look-back parent-child join. Uses the §NFR correlation window; deferred.
- **ML / UEBA scoring.** `ml_score` / `ueba_score` are forward-compatible columns and renormalization terms only; no models.
- **Network / registry / file rules.** Their classes are schema-only and do not flow end-to-end.
- **Incidents + alert grouping.** MVP is alert-level only; the SC001 marquee asserts `alert_count = 1`, `rule_id`, `final_score ≥ 0.75` — the incident assertion is deferred.
- **Full detection bar** (10 rules / 10 scenarios) and the WebSocket dashboard. The MVP ships one rule + two scenarios.
- **The two flagged debts** (resolved operationally here, owned elsewhere): ADR-0011 §4 empty `command_line`/`subject_user_sid` (§Operational §4) and the `event_id` UUIDv4-vs-`alert.json`-v7 reconciliation (§Operational §5). Destinations: a future ETW-path session (ADR-0011) and ADR-0009/0011 (event_id version).

## Data contracts

### Read surface — `cges_events` columns the normalizer projects (verbatim)

Per ADR-0012 §3, the normalizer reads the realized ClickHouse column names, never the OCSF schema names:

```text
agent_id, org_id, event_id, class_uid, activity_id,
process_pid, process_uid, process_name, process_created_time, process_exit_code,
process_parent_pid, process_command_line, subject_user_sid, image_file_name, time
```

`process_command_line` and `subject_user_sid` are structurally empty in v0.1 (§Operational §4) and MUST NOT be used by any v0.1 rule. There is **no** `process_parent_name` column — the parent image is derived by self-join (§Operational §2).

### Normalized rule-evaluable record (Sigma `process_creation` shape)

The normalizer produces, per child Launch event (activity_id = 1):

| Sigma field | Source | Notes |
| --- | --- | --- |
| `Image` | `image_file_name` | full path; basename via `process_name` |
| `ParentImage` | joined parent row's `image_file_name` | resolved by self-join on `process_parent_pid` (§Operational §2); `null` if parent not captured |
| `ProcessId` | `process_pid` | |
| `ParentProcessId` | `process_parent_pid` | |
| `CommandLine` | `process_command_line` | **always `""` in v0.1** — rules MUST NOT use |
| `User` | `subject_user_sid` | **always `""` in v0.1** — rules MUST NOT use |

### Alert shape

Per ADR-0012 §6 (`alerts` table) and `schemas/cges/v0.1/classes/alert.json`. The emitted alert sets: `alert_id` (UUIDv7, slice-generated), `agent_id`, `class_uid = 10001`, `category_uid = 10`, `cg_kind = "alert"`, `activity_id = 1` (Created), `title`, `severity_id`, `cg_detection_source = "rule"`, `rule_id`, `source_events` (≥1 child `event_id`), `heuristic_score`, `final_score`, `cg_mitre`, `dedup_key`, `status = "new"`. `cg_score` is carried embedded per ADR-0012 §4.

### `dedup_key` format

Per ADR-0012 §5:

```text
dedup_key = "<agent_id>::<rule_id>::<process_name>::<bucket_5min>"
bucket_5min = floor(unix_seconds(time) / 300)
```

`process_name` is the child (script-host) process name. `subject` is omitted (v0.1 user telemetry empty, ADR-0012 §5).

### The MVP Sigma rule

One rule, `rules/windows/office_spawns_script_host.yml`, keyed only on populated fields:

```yaml
title: Office Application Spawns a Script Host
id: rule.office_spawns_script_host
status: stable
description: >
  A Microsoft Office application spawned a command or scripting interpreter
  (PowerShell, cmd, Windows Script Host, mshta). Classic macro/phishing
  execution chain (MITRE T1566 -> T1059). Keys on process image lineage only;
  command-line content is unavailable in CGES v0.1 (Kernel-Process provider
  emits no CommandLine — see SPEC-006 Operational).
logsource:
  product: windows
  category: process_creation
detection:
  selection:
    ParentImage|endswith:
      - '\winword.exe'
      - '\excel.exe'
      - '\powerpnt.exe'
      - '\outlook.exe'
      - '\msaccess.exe'
    Image|endswith:
      - '\powershell.exe'
      - '\pwsh.exe'
      - '\cmd.exe'
      - '\wscript.exe'
      - '\cscript.exe'
      - '\mshta.exe'
  condition: selection
level: high
tags:
  - attack.execution
  - attack.t1059.001
  - attack.initial_access
  - attack.t1566.001
cg:
  heuristic_score: 0.9        # confidence; fixture-04 precedent for this threat family
  severity_id: 4              # OCSF High (common/ocsf_severity.json); Sigma level:high -> 4
  cg_mitre:
    tactics: ["execution", "initial-access"]   # NAMES per cg_mitre.json, not TA-codes
    techniques: ["T1059.001", "T1566.001"]
```

The `cg:` block is a CyberGuard extension to the Sigma document carrying the scoring/severity/MITRE the alert needs; the `title`/`logsource`/`detection`/`condition` are standard Sigma. The MVP evaluator supports only the `|endswith` modifier over `Image`/`ParentImage` string lists with a single `selection` and `condition: selection` — sufficient for this rule; richer Sigma is deferred to the Go engine (ADR-0012 §1).

**Scoring / severity basis (schema-verified, not assumed).** `severity_id = 4` is OCSF *High* per `common/ocsf_severity.json` (`0=Unknown, 1=Informational, 2=Low, 3=Medium, 4=High, 5=Critical, 6=Fatal`) — the principled mapping for the rule's Sigma `level: high`. The worked fixture `examples/04_alert_rule_source.json`, for the same threat family (`rule.psh_encoded_from_office`), carries `severity_id = 5` (Critical) because it detects the *encoded-command* variant (the `-EncodedCommand` obfuscation, MITRE T1027 defense-evasion) — a more specific, higher-impact signal than this lineage-only rule, which is command-line-blind in v0.1 (§Operational §4). `heuristic_score = 0.9` matches that fixture's value: detection *confidence* (Office legitimately spawning a script host is rare → low false-positive → high confidence) is high regardless of the lower *severity*; confidence and severity are orthogonal axes. `final_score = heuristic_score = 0.9` (rule-only renormalization per ADR-0012 §4) clears the SC001 marquee's ≥ 0.75 threshold. `tactics` use MITRE tactic **names** (`execution`, `initial-access`) per `common/cg_mitre.json`, not `TA-` codes.

## Operational

### 1. Read-model (the concrete query)

Per ADR-0012 §7, the slice polls forward by a `time` watermark, `FINAL`-collapsing duplicates:

```sql
SELECT event_id, agent_id, activity_id,
       process_pid, process_uid, process_name, image_file_name,
       process_parent_pid, process_created_time, process_exit_code, time
FROM   cges_events FINAL
WHERE  org_id    = {org}
  AND  class_uid = 1007
  AND  time > {watermark}
ORDER BY time ASC
LIMIT  {batch}
```

The watermark persists across polls (Postgres `detect_watermark(org_id, last_time)` row) and advances to the max `time` of each processed batch. Only `activity_id = 1` (Launch) events drive rule evaluation in the MVP (the parent-child chain is established at Launch); Terminate (activity_id = 2) is read but not rule-relevant for this rule.

### 2. Parent-image resolution by self-join (load-bearing; the new constraint)

`cges_events` stores `process_parent_pid` (a PID) but **no parent image column**. To evaluate `ParentImage`, the normalizer self-joins the child's `process_parent_pid` to a parent row's `process_pid` on the same `agent_id`, within the batch plus a look-back window (NFR-006-003), and reads the parent's `image_file_name` / `process_name`:

```text
ParentImage(child) = process_name of the row R where
  R.agent_id = child.agent_id
  AND R.process_pid = child.process_parent_pid
  AND R.activity_id = 1
  AND R.time <= child.time
  (most recent such R within the look-back window)
```

**v0.1 PRODUCTION FALSE-NEGATIVE — load-bearing honesty, stated plainly, not buried.** The parent image resolves only if the parent's Launch was captured — i.e. the parent started **after** the agent's ETW session opened. v0.1 does **not** enumerate pre-existing processes (per the SPEC-005 Phase-0 spike: "Launch events are only captured for processes that start after the agent's session opens; pre-existing processes are not enumerated"). **In production this produces false negatives in the most common real case: Microsoft Office is already running when the agent starts, so its Launch is never observed, the parent-pid self-join finds no parent row, `ParentImage` is `null`, and the rule does not fire.** The detect_ac_001 marquee deliberately controls spawn order (the `winword.exe` stand-in is launched *after* the agent's session opens) so the parent IS captured — therefore **a green detect_ac_001 does NOT mean production coverage of the already-running-Office case.** This is the same class of honest, recorded caveat as ADR-0010's "`runneradmin` ETW privilege untested in CI": the test passing and the field being covered are different claims. **Destination:** an initial-process-enumeration mechanism (ETW rundown via the `ProcessRundown` task the Kernel-Process provider exposes, or a `CreateToolhelp32Snapshot` sweep at agent startup) in a future capture-SPEC increment; until it lands, the gap is accepted and documented. Mirrored in §Risks and §Ratification record.

**PID-reuse caveat:** the join picks the most-recent parent Launch with `time ≤ child.time` inside the window; Windows PID reuse across the window could in principle match a stale parent. Accepted as v0.1 best-effort; the window bounds the exposure.

### 3. Scoring (renormalized, per ADR-0012 §4)

`final_score = Σ(w_i·s_i present) / Σ(w_i present)`. In the MVP only the rule fires, so present = {rule}, `final_score = (0.6 · heuristic_score) / 0.6 = heuristic_score`. The rule's `heuristic_score = 0.9` ⇒ `final_score = 0.9 ≥ 0.75`. The configured ADR-0005 weights (`0.6 / 0.25 / 0.15`) are untouched; `ueba_score` / `ml_score` are absent (NULL), not zero.

### 4. Empty columns — which rules are possible (resolves ADR-0011 §4 flag operationally)

`process_command_line` and `subject_user_sid` are structurally empty in v0.1: the `Microsoft-Windows-Kernel-Process` provider declares no `CommandLine` and no user-SID field (ADR-0012 §3, manifest-verified). Therefore v0.1 rules MUST key only on: `process_name`, `image_file_name`, `ParentImage` (joined), `process_parent_pid`, `process_pid`, `process_uid`, `activity_id`, `process_exit_code`. The blueprint's original SC001 ("powershell `-enc` from Office", keyed on command line) is **re-planted** here as the same threat observed via process lineage (Office `ParentImage` → script-host `Image`), which the populated fields support. The ADR-0011 §4 ETW-mapping correction (it maps `CommandLine`/`SubjectUserSid` the provider does not emit) is owned by ADR-0011; destination is a future ETW-path session.

### 5. `event_id` UUID version — relaxed `source_events` pattern (temporary MVP unblock)

Per ADR-0012 §6: the agent emits `event_id` as UUIDv4, but `alert.json` `source_events` requires a UUIDv7 pattern, so a real-data alert would fail validation. SPEC-006 relaxes the `alert.json` `source_events` item pattern to accept **any** UUID version (`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab0-9a-f][0-9a-f]{3}-[0-9a-f]{12}$`) as a **temporary MVP unblock — not a decision on v4-vs-v7**, which is ADR-0009/ADR-0011 domain. When those ADRs settle the version, the pattern is re-narrowed. The schema change lands in the SPEC-006 implementation (Phase 5), not retroactively in ADR-0012.

### 6. Dedup + status preservation

Alert insertion is `INSERT … ON CONFLICT (dedup_key) DO NOTHING`. Because `dedup_key` embeds the 5-minute bucket (ADR-0012 §5), a re-fire of the same `(agent_id, rule, process_name, bucket)` is a no-op — it does **not** overwrite an alert whose `status` a human has already moved off `new`. The bucket-boundary artifact (two firings either side of a 5-minute edge produce two alerts) is the documented v0.1 limitation per ADR-0012 §5.

## Non-functional requirements

NFR identifiers scoped to this SPEC (`NFR-006-NNN`).

- **NFR-006-001 (poll cadence).** The read-model polls `cges_events` every `5000` ms (default; per-org configurable). Bounds detection latency against ClickHouse query load at MVP volume.
- **NFR-006-002 (batch size).** Each poll processes at most `1000` Launch events (`LIMIT`), bounding per-poll work. Watermark advances by processed-max `time`.
- **NFR-006-003 (parent-join look-back = correlation window).** The self-join (§Operational §2) searches parent Launches within the correlation window of `300` s (ADR-0012 §8) before the child's `time`, per-org configurable. A parent older than the window is treated as not captured (rule does not fire).
- **NFR-006-004 (dedup window).** The `dedup_key` 5-minute bucket (`300` s) per ADR-0012 §5; aligned with NFR-006-003 so dedup and parent-join share one tunable.
- **NFR-006-005 (SC001 marquee budget).** detect_ac_001 MUST complete within `60.0` s wall-clock from probe-spawn to alert-readable-in-Postgres (SPEC-005's 45 s capture budget + one detection poll cycle of headroom). The harness logs `detect_marquee_elapsed_seconds` at `info` on every run.

## Acceptance criteria

Each AC maps 1:1 to a test under `services/ingest/test/`, named `detect_ac_NNN_*` (TypeScript/vitest, since the slice is hosted in `services/ingest/src/detect/` per ADR-0012 §1). The **gate** of each AC is stated explicitly — this defines the Phase-4 (harness-first RED) Known CI debt structure.

| AC | Gate | Why |
| --- | --- | --- |
| detect_ac_001 | **developer-local (Windows + Docker, elevated)** | real ETW capture; like SPEC-005 AC-001; `skipIf(platform!=='win32')` |
| detect_ac_002 | **CI-able (Linux, ts-ci, testcontainers)** | synthetic events inserted; no ETW |
| detect_ac_003 | **CI-able** | synthetic |
| detect_ac_004 | **CI-able** | synthetic |
| detect_ac_005 | **CI-able** | synthetic |
| detect_ac_006 | **CI-able (pure unit)** | scorer function, no DB |

- **detect_ac_001 (SC001 marquee — polyglot end-to-end).** Given the real `cg-agent` running elevated against real `services/ingest/` + real ClickHouse + real Postgres (testcontainers), when a probe parent process whose image basename is `winword.exe` (a renamed benign console stand-in copied to a temp path) is launched **after** the agent's ETW session opens and it spawns `powershell.exe` as a child, then: both Launch events are captured and persisted to `cges_events`; the detection slice reads them, the normalizer resolves `ParentImage = winword.exe` via the parent-pid self-join, the rule matches, and **exactly one** row appears in the Postgres `alerts` table satisfying: `rule_id = "rule.office_spawns_script_host"`, `cg_detection_source = "rule"`, `final_score = 0.9` (≥ 0.75), `source_events` contains the child's `event_id`, `dedup_key` well-formed per §Data contracts, `status = "new"`. The harness logs `detect_marquee_elapsed_seconds` (≤ 60.0 s per NFR-006-005). **Gate: developer-local Windows** (`skipIf(platform!=='win32')`), validated like the SPEC-005 marquee; not run in CI (no ETW on Linux runners; no container runtime on hosted Windows runners per ADR-0010 §Decision part 3).

- **detect_ac_002 (SC010 — false-positive does not fire).** Given synthetic `cges_events` rows inserted directly into ClickHouse (no agent, no ETW) representing a script host whose parent is **not** an Office application (`ParentImage = explorer.exe`, `Image = powershell.exe` — a routine admin action), when the detection slice runs, then **zero** alerts are written to the `alerts` table. **Gate: CI-able.**

- **detect_ac_003 (dedup — N matches → 1 alert).** Given `N = 5` synthetic matching parent→child event sets for the same `(agent_id, rule, process_name, 5-min bucket)` inserted into ClickHouse, when the detection slice runs, then exactly **one** alert row exists for that `dedup_key` (the `ON CONFLICT DO NOTHING` collapses the rest). This also positively exercises the rule-match + alert-emission path in CI. **Gate: CI-able.**

- **detect_ac_004 (status preservation — re-fire does not reset triage).** Given a matching event set that produces one alert (`status = "new"`), when an operator updates that alert's `status` to `"acknowledged"` and then an identical matching event set (same `dedup_key` bucket) is processed again, then the alert's `status` remains `"acknowledged"` (the `DO NOTHING` upsert does not overwrite). **Gate: CI-able.**

- **detect_ac_005 (read-model watermark — no re-processing).** Given batch A inserted and processed (watermark advances to `max(time_A)`), when batch B (`time_B > time_A`) is inserted and processed, then batch A's events are not re-evaluated: the watermark is strictly greater than `max(time_A)`, and no second alert is produced from batch A's events. **Gate: CI-able.**

- **detect_ac_006 (renormalized score — rule-only equals heuristic).** Given the scorer invoked with only a rule signal present (`heuristic_score = 0.9`, `ueba_score`/`ml_score` absent), then `final_score = 0.9` exactly (denominator = `w_rule = 0.6`, i.e. `0.6·0.9 / 0.6`), **not** `0.54` (the literal-absent-as-zero result). A second case (`heuristic_score = 0.5 ⇒ final_score = 0.5`) pins the identity. **Gate: CI-able (pure unit, no DB).**

**Coverage note.** The positive rule-match logic is CI-guarded by detect_ac_003/004 (synthetic matching events → alert); detect_ac_001 adds the real-ETW end-to-end chain on top, developer-local. The negative (FP) is detect_ac_002. So CI guards rule logic + dedup + status + watermark + scoring; the developer-local marquee guards the ETW capture chain.

## Test scenarios

Per ADR-0005 §Harness obligation, each scenario in `harness/scenarios/` declares its expected `detection_source`:

- **SC001 — Office spawns script host (positive).** Input: parent `winword.exe` Launch, child `powershell.exe` Launch with `parent_pid = winword.pid`. Expected: 1 alert, `detection_source = "rule"`, `rule_id = "rule.office_spawns_script_host"`, `final_score = 0.9`. Realized by detect_ac_001 (dev-local, real ETW) and detect_ac_003 (CI, synthetic).
- **SC010 — benign script host (false-positive).** Input: parent `explorer.exe` Launch, child `powershell.exe` Launch. Expected: 0 alerts (no `detection_source`). Realized by detect_ac_002.

## Risks

| Risk | Mitigation |
| --- | --- |
| Parent Launch not captured (pre-session / outside look-back) → rule does not fire (false negative) | Documented v0.1 limitation (§Operational §2); marquee controls spawn order; rundown/longer look-back deferred |
| PID reuse within the look-back window matches a stale parent | Join picks most-recent parent Launch with `time ≤ child.time` inside the window; bounded exposure; v0.1 best-effort |
| `FINAL` read cost at scale | Acceptable at MVP volume; `GROUP BY event_id` + `argMax` alternative noted; revisit at scale per ADR-0012 §7 |
| Bucket-boundary dedup artifact (double alert across a 5-min edge) | Accepted, documented (ADR-0012 §5); sliding-window deferred |
| `source_events` UUIDv4 vs `alert.json` v7 pattern | Pattern relaxed as temporary MVP unblock (§Operational §5); real decision is ADR-0009/0011 |
| Empty `command_line`/`subject_user_sid` mislead future rule authors | §Operational §4 + §Data contracts mark them unusable in v0.1; rules key on lineage |

## Open questions

1. **Second rule?** The MVP ships **one** rule (recommended — keeps the marquee tight and the evaluator minimal). A second (e.g., *script host spawns a child from a user-writable temp path*, keyed on `Image` path prefix) is a candidate but is deferred unless ratified now. **Recommendation: one rule for the MVP.**
2. **`severity_id` source.** SC001 uses `severity_id = 4` (OCSF High) from the rule's `cg:` block. Confirmed sensible; revisit per-rule when the bar grows.
3. **`alert.json` `severity_id` as a named property.** Currently carried via `additionalProperties: true`; promoting it to a named property is a schema question, flagged in ADR-0012 §Consequences (neutral). Deferred.

## Ratification record

Load-bearing decisions surfaced and ratified in chat (Session 17). Recommended-default-and-rationale pattern per SPEC-003/005.

1. **One rule for the MVP (ratified).** The MVP proves the full detection CHAIN, not breadth; one well-chosen rule exercises it end-to-end. *Office spawns a script host* is real macro-hunting signal, not a placeholder. Re-planting it from command-line content (the blueprint's original `powershell -enc from Office`) to process **lineage** (`ParentImage` Office → `Image` script-host) is the correct adaptation to v0.1's empty `command_line` (§Operational §4), not a downgrade. A second rule is an §Open question, deferred.

2. **`severity_id = 4` + `heuristic_score = 0.9` (schema/fixture-verified, not from memory).** `severity_id = 4` = OCSF *High* read from `common/ocsf_severity.json`, the principled mapping for Sigma `level: high`. Fixture `04_alert_rule_source.json` (same threat family) uses `severity_id = 5` (Critical) for the *encoded-command* variant (more specific, T1027 obfuscation); the lineage-only v0.1 rule is command-line-blind, so *High* not *Critical* is correct. `heuristic_score = 0.9` matches fixture 04's value for this threat family — confidence (low-FP lineage signal) is high independent of the lower severity (orthogonal axes). `cg_mitre.tactics` use names per `cg_mitre.json`.

3. **Parent-resolution production false-negative — explicit, not buried (condition of ratification).** v0.1 does not enumerate pre-existing processes; the parent-child rule fires only when the parent's Launch was captured. In production, an already-running Office app (the common case) yields a false negative. A green detect_ac_001 (which controls spawn order) does NOT imply production coverage. Same honesty class as "`runneradmin` untested in CI." Destination: an initial-process-enumeration increment in a future capture SPEC. Recorded in §Operational §2 + §Risks.

4. **Gate split (minimal CI debt).** detect_ac_001 is developer-local (Windows + Docker + elevated; `skipIf(platform!=='win32')`), validated like the SPEC-005 marquee; detect_ac_002–006 are CI-able on Linux ts-ci (synthetic events / pure unit). The Phase-4 harness-first RED therefore turns **only ts-ci** red (detect_ac_002–006 before impl), with the Known CI debt co-located in that SHA; detect_ac_001 is never CI debt.

5. **Two operational debts carried, not re-decided.** Empty `process_command_line` / `subject_user_sid` (ADR-0011 §4 domain — rules key on lineage); `event_id` UUIDv4 vs `alert.json` v7 pattern (relaxed to version-agnostic as a temporary MVP unblock per §Operational §5; the v4-vs-v7 decision is ADR-0009/0011 domain). Neither is decided by this SPEC.

## References

- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — the contract this SPEC realises (event source, normalize, scoring renorm, dedup, alert storage, the Go-vs-TS seam, the two flagged contradictions). SPEC-006 re-decides none of it.
- [ADR-0002](../adr/0002-language-per-component.md) — Go `services/pipeline/` target; amended (ADR-0012 §1) for the MVP slice hosted in TS ingest.
- [ADR-0003](../adr/0003-polyglot-storage.md) — alerts → Postgres-only for the MVP (amended by ADR-0012 §6).
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — detection philosophy; `detection_source`; scoring composition (clarified by ADR-0012 §4); §Harness obligation.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md) — CGES/OCSF; Alert (10001) / `cg_score` / `cg_mitre` classes.
- [ADR-0009](../adr/0009-event-delivery-and-buffer.md) — at-least-once + `ReplacingMergeTree` → `FINAL` reads; `event_id` (UUIDv7 per the ADR; UUIDv4 in code — §Operational §5).
- [ADR-0011](../adr/0011-cges-process-activity-v0-1.md) — Process Activity 1007 it reads; §4 ETW-mapping contradiction flagged (empty `command_line`/`subject_user_sid`).
- [SPEC-004](SPEC-004-server-ingest-minimal.md) — the ingest service hosting the detection slice; the testcontainers harness pattern the CI-able ACs reuse.
- [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — producer of the `cges_events` rows; AC-001 marquee pattern detect_ac_001 mirrors; the "no pre-existing process enumeration" fact that bounds parent resolution.
- `schemas/cges/v0.1/classes/alert.json`, `common/cg_score.json`, `common/cg_mitre.json` — alert / score / mitre shapes.
- `services/ingest/src/db/migrate.ts` (`cges_events` columns), `services/ingest/src/db/migrations/0001_initial.ts` (Kysely pattern the `0002_alerts` migration extends).
- [Sigma specification](https://github.com/SigmaHQ/sigma-specification) — rule format; the MVP supports a minimal subset (`|endswith` over `Image`/`ParentImage`).
- [MITRE ATT&CK](https://attack.mitre.org/) — T1059 (Command and Scripting Interpreter), T1566 (Phishing); the SC001 threat.
