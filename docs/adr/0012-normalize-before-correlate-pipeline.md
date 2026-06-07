# ADR-0012: Normalize-before-correlate pipeline — Detection MVP (event source, scoring, alert storage, dedup, the Go-vs-TS seam)

- Status: Accepted
- Date: 2026-05-30
- Last updated: 2026-06-07
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0005 (detection — rules and ML in parallel) locked the detection *philosophy*: rules and ML detect in parallel, both alert autonomously, the action policy discriminates by reversibility, and `final_score` composes the contributing signals. ADR-0005 §Compliance closes with an explicit forward reference:

> "Subsequent work that touches the pipeline (**a future ADR for the normalize-before-correlate pipeline concern, currently unowned**) must similarly honour the per-alert recording of `detection_source` and, for ML alerts, of the input feature snapshot and confidence score that justified them."

This ADR is that future ADR. It owns the *normalize-before-correlate* pipeline concern for the Detection MVP (Phase 5): the slice that reads CGES events out of storage, normalizes them into a rule-evaluable shape, evaluates Sigma-style rules, scores, and emits alerts. It is the first ADR to make detection concrete; ADR-0005 stays the philosophy.

Phase 4 (Session 16) closed with real `Microsoft-Windows-Kernel-Process` Process Activity (OCSF class_uid 1007) flowing end-to-end: agent → signed envelope → ingest → ClickHouse `cges_events`. The Detection MVP consumes that table. No other event class flows end-to-end yet (filesystem 1001, authentication 3002, network 4001 are schema-only), so the MVP detects on process telemetry only.

The architectural tension this ADR must resolve honestly: ADR-0002 assigns the correlation/pipeline engine to **Go** (`services/pipeline/`, "cg-pipeline"), and ADR-0005 §Out-of-scope states the Sigma-to-Go rule engine "is a SPEC of `services/pipeline/`." But `services/pipeline/` is scaffold-only (README placeholder, no code), the Go toolchain is not yet in the Approved local toolchain, and the event firehose that motivates the Go choice (ADR-0007) is deferred to its own future ADR on throughput evidence that does not yet exist. The MVP needs a detection slice *now*, against ClickHouse-direct reads, at MVP volume. This ADR declares a transitory seam — and, because ADR-0002 Rule 3 requires a superseding/amending ADR for any non-Go server-side artifact, it does so via an explicit co-located amendment to ADR-0002 (see §1 and the amendment block), not a bare prose note.

A read-only audit (Session 17, step 1) preceded this ADR and surfaced repo-grounded facts that shape the decisions below: (i) the realized CGES wire/DB column names diverge from the OCSF schema names across three layers; (ii) two ClickHouse columns (`process_command_line`, `subject_user_sid`) are structurally empty in v0.1 because the Kernel-Process provider declares no such fields; (iii) the ADR-0005 default weights with a literal absent-source-as-zero reading make the marquee threshold unreachable; (iv) alert/incident/score/mitre CGES schemas already exist and pin the alert shape; (v) the agent emits `event_id` as UUIDv4 while `alert.json` requires a UUIDv7 pattern on `source_events`. Each is addressed below; (v) is raised as an explicit reconciliation flag (§6).

## Decision

### 1. The Go-vs-TS seam — declared via a co-located amendment to ADR-0002

**Production target:** the normalize-correlate-score-alert pipeline belongs in **Go** at `services/pipeline/` per ADR-0002 §Decision and ADR-0005 §Out-of-scope.

**MVP slice (transitory):** for the Detection MVP, the normalize→rule→score→alert slice is implemented in the existing **TypeScript ingest service** (`services/ingest/`, behind a `src/detect/` module boundary), co-located with the ClickHouse and Postgres clients ingest already owns.

A detection slice in TypeScript is a non-Go server-side artifact, which ADR-0002 Rule 3 forecloses without a superseding/amending ADR (the same mechanism ADR-0007 used to move the `services/ingest/` row to TypeScript). Therefore this ADR **amends ADR-0002 for the MVP detection slice only** (see the co-located *Amendment to ADR-0002* block at the end). The amendment is deliberately narrow and transitory:

- It supersedes the `services/pipeline/` = Go assignment **only for the MVP detection slice hosted in `services/ingest/src/detect/`**, not for `services/pipeline/` as a whole.
- **Named exit condition:** the slice is ported into the Go `services/pipeline/` service when the event-firehose ADR (deferred per ADR-0007 §Consequences) lands and brings the Go toolchain. At that point the slice is removed from ingest and the ADR-0002 Go assignment is restored in full.
- The Go toolchain is **NOT** added to the Approved local toolchain by this ADR. No Go code is written. The module boundary keeps the rule-evaluation and scoring logic pure and transport-agnostic so the port is a lift, not a rewrite.

This is a second Accepted-ADR amendment in this ADR (the first is ADR-0003 §6). Both land atomically when this ADR flips to Accepted.

### 2. Event source — ClickHouse direct, not NATS

The MVP detection slice reads events **directly from ClickHouse `cges_events`**, not from a NATS subject. ADR-0003 names `events.normalized.{org}` and `alerts.{org}` as the eventual NATS JetStream subjects, and ADR-0002 cites NATS client maturity as a Go driver — but NATS/JetStream is gated on the event-firehose ADR (deferred per ADR-0007 §Consequences on throughput evidence that does not exist). At MVP volume, a polling read against ClickHouse is sufficient and avoids standing up a message bus prematurely.

Because `cges_events` is `ReplacingMergeTree(arrived_at)` (ADR-0009: at-least-once delivery, server-side dedup on `event_id`), reads that must not see duplicates **MUST** use `FINAL` (or an explicit `GROUP BY event_id` with `argMax(_, arrived_at)`). The MVP read-model (§7) uses `FINAL`.

### 3. Normalize — read the wire/DB names, never the OCSF names

The normalize step reads the **realized ClickHouse column names verbatim** from `cges_events` (`services/ingest/src/db/migrate.ts`), never the OCSF dotted names from the JSON Schema. These are three distinct, non-matching naming layers (audit finding i). Note the agent JSON wire nests the per-process fields under a `process` object inside `CgesProcessActivity` (`agent/cg-agent/src/cges/emit.rs`); the ClickHouse table flattens them with a `process_` prefix; the OCSF schema uses different names again:

| Concept | ClickHouse column — normalize READS this | Agent JSON wire (nested under `process.`) | OCSF schema (`process.json`) — TRAP |
| --- | --- | --- | --- |
| command line | `process_command_line` | `process.command_line` | `cmd_line` |
| user SID | `subject_user_sid` | `process.subject_user_sid` | `user` (→ `user.uid`) |
| image path | `image_file_name` | `process.image_file_name` | `file` (→ `file.path`) |
| parent pid | `process_parent_pid` | `process.parent_pid` | `parent_process` (→ `.pid`) |
| pid / uid / name | `process_pid` / `process_uid` / `process_name` | `process.pid` / `.uid` / `.name` | `pid` / `uid` / `name` |
| exit / created | `process_exit_code` / `process_created_time` | `process.exit_code` / `.created_time` | `exit_code` / `created_time` |

(Event-level fields `event_id`, `class_uid`, `activity_id`, `time` are top-level on the wire, not under `process`.) Full `cges_events` column set the normalizer projects from: `agent_id, org_id, event_id, class_uid, activity_id, process_pid, process_uid, process_name, process_created_time, process_exit_code, process_parent_pid, process_command_line, subject_user_sid, image_file_name, time`.

**Column-population reality in v0.1 (audit finding ii — load-bearing for rule design):** `process_command_line` and `subject_user_sid` are **structurally always empty (`""`)**. The `Microsoft-Windows-Kernel-Process` provider manifest declares no `CommandLine` and no user-SID field on any event or version (verified via `Get-WinEvent -ListProvider`); the agent's `try_parse("CommandLine")` / `try_parse("UserSID")` therefore always fall through to `unwrap_or_default()`. This is a provider limitation, not a bug.

**Consequence:** v0.1 detection rules MUST key only on populated fields — `process_name`, `image_file_name`, `process_parent_pid` (parent-child linkage via a join on `(agent_id, parent_pid) → (agent_id, pid)`), `process_pid`, `process_uid`, `activity_id`, `process_exit_code`. A rule keyed on command-line content cannot fire on v0.1 data and must not be shipped as v0.1-validated.

> **Cross-ADR flag (out of this ADR's scope, raised for the gate, deferred per §Out of scope):** ADR-0011 §4 maps `process.cmd_line ← CommandLine` and `process.user.uid ← SubjectUserSid` as if those ETW fields exist. The manifest shows they do not. This is the same class of false-ETW-premise that ADR-0011 §6 already corrected for the Terminate timestamp (Amendment 2026-05-28). It warrants a co-located ADR-0011 §4 amendment, which is ADR-0011's domain, not this ADR's. Note also a spelling discrepancy to reconcile in that amendment: ADR-0011 §4 names the ETW field `SubjectUserSid`, the agent parses `UserSID` (`session.rs`), and the manifest has neither.

### 4. Scoring — renormalized weighted-sum (clarifies ADR-0005, configured weights unchanged)

ADR-0005's formula `final_score = w_rule·rule_score + w_ml·ml_score + w_ueba·ueba_score` does not specify the behaviour when a source is absent. A literal absent-as-zero reading caps `final_score` at `w_rule = 0.6` when only the rule fires — below the SC001 marquee threshold this ADR fixes for SPEC-006 (**`final_score ≥ 0.75`**; an SC001 acceptance threshold this ADR introduces for SPEC-006 to realise, not a pre-existing repo value), and inconsistent with the worked fixture `schemas/cges/v0.1/examples/04_alert_rule_source.json` (`heuristic_score: 0.9` → `final_score: 0.9`, not `0.54`).

This ADR pins the absent-source semantics: **the configured weights renormalize over the sources actually present.**

```text
final_score = Σ(w_i · s_i over present sources i) / Σ(w_i over present sources i)
```

- The denominator is the **sum of the active weights**, not a count of sources. It is always > 0: `alert.json` requires `cg_detection_source ∈ {rule, ml, hybrid}`, so at least one scoring source is always present — the empty-present-set (division-by-zero) case cannot occur for a schema-valid alert.
- Rule-only (the v0.1 case): present = {rule}; `final_score = (0.6 · heuristic_score) / 0.6 = heuristic_score`.
- Rule + ML (future, ueba absent): `final_score = (0.6·heuristic + 0.15·ml) / 0.75`.
- All three present: denominator = 1.0; the formula reduces exactly to ADR-0005's literal form.

This is a **clarification of ADR-0005's under-specified absent-source case, not an amendment of its weights**: the *configured* weights `w_rule=0.6, w_ueba=0.25, w_ml=0.15` (and their sum-to-1.0 invariant) are untouched; only the *effective applied* weighting is renormalized when sources are absent, and the all-present case is identical to ADR-0005. The CGES score field is `heuristic_score` (`common/cg_score.json`), so ADR-0005's `rule_score` term maps to `heuristic_score`. `cg_score` is not a property of `alert.json`; it is carried embedded on the alert via `additionalProperties: true` (§6 pins where).

**Fixture consistency.** Fixture 04 (rule-only) already embodies this: `final_score = heuristic_score = 0.9`. Fixture `05_alert_ml_source.json` (ml-only) carries `final_score: 0.71`, which follows neither the literal nor the renormalized formula (ml-only renormalizes to `final_score = ml_score = 0.88`); its `0.71` is a stale illustrative value, corrected to `0.88` when SPEC-006 lands — a non-normative example update, the same posture as the §5 dedup_key fixture, contradicting no Accepted contract.

### 5. Dedup key — declarative bucket-in-key, subject omitted in v0.1

Alerts are deduplicated via a **declarative `dedup_key` with the 5-minute window embedded**, enabling an idempotent `INSERT … ON CONFLICT (dedup_key) DO NOTHING` without separate window-query logic (and without its read-then-write race):

```text
dedup_key = "<agent_id>::<rule_id>::<process_name>::<bucket_5min>"
where bucket_5min = floor(unix_seconds(time) / 300)
```

- `agent_id` replaces the fixture's hostname (`FIN-PC-014`): v0.1 captures no hostname, and `agent_id` (UUIDv7) is the stable per-endpoint key actually available.
- `rule_id` is the **full** rule identifier (e.g. `rule.psh_encoded_from_office`), not a bare suffix — the component is unambiguous and SPEC-006's updated fixture uses the full form.
- `process_name` is the subject discriminator the fixture uses (its `winword.exe`). It is the bare `process_name` column, not the full `image_file_name` path.
- `bucket_5min` embeds the window; two firings in the same 5-minute bucket collapse to one alert; the next bucket re-alerts.

**Why `subject_user_sid` is NOT in the v0.1 key (audit-grounded, not a preference):** distinguishing two same-name processes by user would be semantically desirable, but `subject_user_sid` is structurally empty in v0.1 (§3). Including it would add a constant `""` component discriminating nothing. v0.1 telemetry carries no user identity. **Forward path:** when user-SID capture lands (e.g. Security-Auditing event 4688, or a token-introspection mechanism in a future per-class ADR), `subject` is added as a `dedup_key` component and this v0.1 omission is revisited.

**Known v0.1 artifact (documented, accepted):** two firings straddling a bucket boundary (e.g. `11:04:59` / `11:05:01`) fall in different buckets and are NOT deduplicated. Accepted for the MVP; the destination is a sliding-window dedup in a later increment if boundary duplicates prove noisy. Recorded in SPEC-006 §Operational.

The on-disk example `examples/04_alert_rule_source.json` (`dedup_key: "FIN-PC-014::psh_encoded_from_office::winword.exe"`) is an **example, not normative schema** (`alert.json` constrains `dedup_key` only as a non-empty string). It is updated to the chosen pattern when SPEC-006 lands; cheap, and contradicts no Accepted contract (unlike the ADR-0003 retention row in §6).

### 6. Alert storage — Postgres only for the MVP (amends ADR-0003 §Retention)

Alerts persist to **Postgres only** for the MVP. Alerts are mutable triage state (status transitions, assignment, timestamps); an append-only ClickHouse sink would be debt. This **amends ADR-0003 §Retention** (co-located amendment block at the end). The `alerts` table lands as a new Kysely migration (`services/ingest/src/db/migrations/0002_alerts.ts`), extending the existing chain (`0001_initial.ts`: `ca`, `agents`, `enrollment_tokens`) under the same `pg_try_advisory_lock`.

**`alerts` table shape (the decision; SPEC-006 realises it):**

```sql
CREATE TABLE IF NOT EXISTS alerts (
  alert_id            uuid PRIMARY KEY,                       -- UUIDv7, generated by the detection slice
  org_id              text        NOT NULL DEFAULT 'default',
  agent_id            uuid        NOT NULL REFERENCES agents (agent_id),
  category_uid        smallint    NOT NULL DEFAULT 10,
  class_uid           integer     NOT NULL DEFAULT 10001,     -- CGES Alert
  activity_id         smallint    NOT NULL DEFAULT 1,         -- 1 = Created
  cg_kind             text        NOT NULL DEFAULT 'alert',
  title               text        NOT NULL,
  description         text,
  severity_id         smallint    NOT NULL,                   -- OCSF severity 1..6
  cg_detection_source text        NOT NULL
                        CHECK (cg_detection_source IN ('rule', 'ml', 'hybrid')),
  rule_id             text,                                   -- required when source in (rule, hybrid)
  model_id            text,                                   -- required when source in (ml, hybrid)
  source_events       uuid[]      NOT NULL,                   -- >= 1 cges_events.event_id (UUIDv4, agent-generated; see flag)
  heuristic_score     numeric(4,3),                           -- cg_score components, 0..1
  ueba_score          numeric(4,3),
  ml_score            numeric(4,3),
  final_score         numeric(4,3) NOT NULL,                  -- renormalized per §4
  cg_mitre            jsonb,                                  -- { tactics: [], techniques: [] }
  dedup_key           text        NOT NULL,
  status              text        NOT NULL DEFAULT 'new'
                        CHECK (status IN ('new', 'acknowledged', 'resolved', 'false_positive')),
  created_at          timestamptz NOT NULL DEFAULT now(),
  updated_at          timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT alerts_dedup_key_unique  UNIQUE (dedup_key),
  CONSTRAINT alerts_rule_id_present   CHECK (cg_detection_source NOT IN ('rule','hybrid') OR rule_id  IS NOT NULL),
  CONSTRAINT alerts_model_id_present  CHECK (cg_detection_source NOT IN ('ml','hybrid')   OR model_id IS NOT NULL),
  CONSTRAINT alerts_source_events_nonempty CHECK (cardinality(source_events) >= 1),
  CONSTRAINT alerts_final_score_range CHECK (final_score BETWEEN 0 AND 1),
  CONSTRAINT alerts_heuristic_range   CHECK (heuristic_score IS NULL OR heuristic_score BETWEEN 0 AND 1),
  CONSTRAINT alerts_ueba_range        CHECK (ueba_score IS NULL OR ueba_score BETWEEN 0 AND 1),
  CONSTRAINT alerts_ml_range          CHECK (ml_score IS NULL OR ml_score BETWEEN 0 AND 1),
  CONSTRAINT alerts_score_present     CHECK (num_nonnulls(heuristic_score, ueba_score, ml_score) >= 1)
);
```

The columns mirror `alert.json`'s required set (`alert_id, category_uid, class_uid, cg_kind, activity_id, title, cg_detection_source, source_events, dedup_key`) plus its conditional `rule_id`/`model_id`, the embedded `cg_score` components (§4), `cg_mitre` (`cg_mitre.json`: `tactics[]` + `techniques[]`), the OCSF `severity_id`, and the **mutable triage columns** (`status`, `updated_at`) that justify Postgres over ClickHouse. The `CHECK` constraints encode `alert.json`'s `allOf` conditionals, `cg_score.json`'s `0..1` range and its `anyOf` (≥1 score present), and the `source_events` cardinality. Dedup is `INSERT … ON CONFLICT (dedup_key) DO NOTHING` — **DO NOTHING, not DO UPDATE**: a re-fire within the bucket must not reset a `status` a human has already moved off `new`.

> **Reconciliation flag for the gate (audit finding v) — `source_events` UUID version.** The agent generates `event_id` with `Uuid::new_v4()` (`agent/cg-agent/src/etw/types.rs`, `session.rs`); ingest passes it through verbatim. But `alert.json`'s `source_events` items require the UUIDv7 pattern `^...-7xxx-[89ab]xxx-...`. A real-data alert's `source_events` (v4) would therefore **fail `alert.json` validation**. **This ADR takes no position on whether `event_id` should be v4 or v7** — that reconciliation is ADR-0009 / ADR-0011 domain (those ADRs already describe `event_id` as UUIDv7, and v7 may well be intentional: it is time-ordered, relevant to the temporal ordering of `event_id`). To avoid blocking the MVP, **SPEC-006 relaxes `alert.json`'s `source_events` pattern to accept any UUID version — a temporary MVP unblock, not the answer**; when ADR-0009 / ADR-0011 settle v4-vs-v7, the pattern is re-narrowed accordingly. Relaxing *defers* the real decision, it does not make it. The Postgres `uuid[]` column is version-agnostic, so storage is unaffected either way.

### 7. Read-model — the concrete ClickHouse query

The transitory slice polls `cges_events` forward by a `time` watermark, collapsing at-least-once duplicates with `FINAL`:

```sql
SELECT event_id, agent_id, activity_id,
       process_pid, process_uid, process_name, image_file_name,
       process_parent_pid, process_created_time, process_exit_code, time
FROM   cges_events FINAL
WHERE  org_id    = {org}
  AND  class_uid = 1007                 -- Process Activity only (the only class flowing in v0.1)
  AND  time > {watermark}               -- last processed event time, exclusive
ORDER BY time ASC
LIMIT  {batch}
```

The watermark advances to the max `time` of each processed batch. `FINAL` is acceptable at MVP volume; its cost and the alternative (`GROUP BY event_id` + `argMax(*, arrived_at)`) are noted in SPEC-006 §Operational. Parent-child rules join the batch (and a short look-back) on `(agent_id, parent_pid) → (agent_id, pid)`.

### 8. Correlation window — default 300 s, per-org configurable (new ADR-0012 surface)

This ADR sets the correlation window default at **300 seconds (5 minutes)** and makes it **per-org configurable**, extending the per-org configuration surface ADR-0005 already establishes for weights and `trust_sources` to a new dimension. (ADR-0005 §Consequences states only that the window value is "set in the SPEC of cg-pipeline and is revisable"; making it a per-org surface is this ADR's decision, not a pre-existing ADR-0005 one.) In the MVP the window governs exactly one thing: the `dedup_key` bucket (§5). **Stateful multi-event correlation across the window (sequences, joins beyond single-batch parent-child) and the `hybrid` rule+ML join are deferred** (§Out of scope) — the window is defined now so the dedup bucket and the future hybrid join share one tunable, but only its dedup role is implemented in v0.1.

## Alternatives considered

### A1 — NATS/JetStream as the MVP event source now

Pros: matches ADR-0003's eventual `events.normalized.{org}` topology; no polling. Cons: stands up a message bus for zero MVP benefit; the firehose justifying NATS is deferred on absent throughput evidence (ADR-0007). Rejected — premature infrastructure.

### A2 — Alerts in ClickHouse (honour ADR-0003 §Retention as written)

Pros: no ADR-0003 amendment. Cons: alerts are mutable triage state; ClickHouse is append-only / merge-on-read; mutable `status` on `ReplacingMergeTree` means version churn and `FINAL` on every triage mutation — debt. Rejected; ADR-0003 §Retention amended for the MVP (§6) with a named path back if alert analytics demands a warm ClickHouse copy.

### A3 — Stand up the Go `services/pipeline/` service now

Pros: builds the production-target architecture directly. Cons: adds the Go toolchain to local + CI before the firehose ADR justifies it; `services/pipeline/` is greenfield; doubles the language surface for a slice that fits in the service already owning the ClickHouse + Postgres clients. Rejected — the transitory TS seam (§1, via the ADR-0002 amendment) defers Go to its named trigger.

### A4 — Literal absent-as-zero scoring (`final_score = 0.6·heuristic` rule-only)

Pros: ADR-0005 formula at face value. Cons: caps `final_score` at 0.6, making the SC001 threshold ≥ 0.75 unreachable for a rule-only MVP, and contradicts fixture 04 (`final_score = 0.9`). Rejected — renormalization (§4) is what the fixtures embody.

### A5 — Bucketless `dedup_key` + windowed suppression query

Pros: matches the fixture's `host::rule::process_name` exactly; no bucket-boundary artifact. Cons: the 5-minute window becomes a read-then-write with a race window and more code than a declarative unique constraint. Rejected for the MVP in favour of the declarative bucket-in-key (§5), with the boundary artifact documented and a named path to a sliding window. (Mechanism choice, ratified explicitly; the fixture is non-normative and is updated.)

### A6 — Include `subject_user_sid` in `dedup_key`

Pros: same-name processes under different users would separate. Cons: `subject_user_sid` is structurally empty in v0.1 (§3); the component would be a constant `""`. Rejected for v0.1 on the audit fact, with a named forward path.

## Consequences

### Positive

- The MVP detects on real process telemetry without standing up NATS or the Go pipeline service; the seam (§1) defers both to their named triggers.
- Scoring (§4) makes the SC001 threshold reachable and is consistent with fixture 04, while leaving ADR-0005's configured weights untouched and forward-compatible with ML/UEBA.
- Alert storage (§6) puts mutable triage state where it belongs and honours the pre-existing `alert.json` / `cg_score.json` / `cg_mitre.json` schemas — with the one exception flagged for reconciliation (`source_events` UUID version).
- The schema-vs-wire decision (§3) closes the trap before any rule is written; the empty-column reality is recorded so no rule ships against fields that cannot fire.
- The dedup mechanism (§5) is a one-line idempotent upsert with no race.

### Negative

- The transitory seam (§1) is debt by construction; ported to Go on the firehose ADR. Mitigated by the module boundary and the named exit condition — honest, tracked debt.
- The bucket-boundary dedup artifact (§5) can double-alert across a 5-minute edge. Accepted for the MVP, documented, with a named path to a sliding window.
- `FINAL` reads (§7) cost more than plain scans; acceptable at MVP volume, noted for revisit at scale.
- This ADR amends **two** Accepted ADRs (ADR-0003 §Retention, ADR-0002 §services/pipeline row for the MVP slice) and surfaces contradictions in two others (ADR-0011 §4 ETW mapping; the `event_id` UUIDv4-vs-`alert.json`-v7 gap). All are handled openly (co-located amendments; explicit flags) rather than silently.

### Neutral

- The correlation window (§8) is defined but only its dedup role is implemented; hybrid and stateful-correlation roles are deferred.
- `alert.json` does not currently list `severity_id` (it relies on `additionalProperties: true`); whether to add it as a named property is a SPEC-006 schema question, not an ADR decision.

## Compliance

- Any v0.1 detection rule MUST declare `cg_detection_source: "rule"`, key only on populated `cges_events` columns (§3), and have a paired scenario in `harness/scenarios/` per ADR-0005 §Harness obligation.
- The scorer MUST implement renormalization-over-present-sources (§4); a rule-only alert's `final_score` equals its `heuristic_score`.
- Alerts MUST persist to Postgres per §6 and satisfy the `alerts` table constraints.
- Reads from `cges_events` MUST collapse at-least-once duplicates (`FINAL` or `GROUP BY` + `argMax`) per §2.
- The detection slice MUST live behind a `services/ingest/src/detect/` module boundary (§1) so the Go port is a lift; the Go toolchain MUST NOT be added until the firehose ADR.
- SPEC-006 carries the production specification (rules, AC tests `detect_ac_NNN_*`, the SC001/SC010 marquee scenarios, the `0002_alerts` migration, the read-model implementation, the `source_events`-pattern reconciliation) consistent with this ADR's jurisprudence.

## Out of scope

Each deferred item names its destination:

- **Event firehose / NATS JetStream ingest.** Deferred to the unnumbered future ADR per ADR-0007 §Consequences; that ADR brings the Go toolchain and triggers the §1 seam extraction.
- **Enrich stage** (GeoIP, asset/identity enrichment, threat-intel join). A future SPEC of `services/pipeline/`.
- **Stateful multi-event correlation** across the window (sequences, multi-step joins beyond single-batch parent-child). Uses the §8 window; deferred.
- **ML / UEBA scoring.** Forward-compatible columns (§6) and renormalization terms (§4); the models are out of scope per ADR-0005 §Out of scope.
- **Network / registry / file detection rules.** Their CGES classes (4001 / — / 1001) are schema-only and do not flow end-to-end; deferred to the per-class ADRs/SPECs that make them load-bearing.
- **Incidents + alert grouping** (`incident.json` class 10002, status lifecycle, `alert_ids[]`). The MVP emits alert-level only; the `incidents` table and grouping are a future SPEC. The SC001 marquee asserts at alert level (`alert_count = 1`, `rule_id`, `final_score ≥ 0.75`); the incident assertion is deferred.
- **Full detection bar** (10 rules / 10 scenarios) and the WebSocket dashboard. The MVP ships 1–2 Sigma rules + scenarios; the full bar and live dashboard are later phases.
- **ADR-0011 §4 ETW-mapping correction** and the **`event_id` UUIDv4-vs-v7 ADR-vs-code discrepancy** (§3, §6 flags). ADR-0011 / ADR-0009 domain; raised for the gate, not actioned here. Destination: a future session that touches the ETW capture path.

## Landing checklist (atomic on flip to Accepted)

When this ADR is ratified Proposed→Accepted, the same commit:

1. Flips the status header to `Accepted`.
2. Adds the *Amendment to ADR-0003* block (below) to `docs/adr/0003-polyglot-storage.md`.
3. Adds the *Amendment to ADR-0002* block (below) to `docs/adr/0002-language-per-component.md`.
4. Adds the catalog row to `docs/adr/README.md`.
5. Adds the dependency edges to `docs/adr/README.md` §Dependencies (0012 → 0002, 0003, 0005, 0006, 0007, 0009, 0011).

## Amendment to ADR-0003 (co-located, lands atomically when this ADR is Accepted)

The following block is added to `docs/adr/0003-polyglot-storage.md` (before its §References) in the same commit that flips this ADR to `Accepted`:

> **Amendment 2026-05-30 (ADR-0012): Alerts retention narrowed to Postgres-only for the MVP.** §Retention's Alerts row (`Postgres + ClickHouse | Postgres + ClickHouse`) is superseded for the MVP by ADR-0012 §6: alerts persist to Postgres only. Rationale: alerts are mutable triage state; an append-only ClickHouse sink would be debt. ADR-0003 remains `Accepted`; this amendment supersedes the Alerts retention row where they differ. Destination for the ClickHouse sink: reconsidered if/when historical analytics over alerts becomes a requirement. Incidents/Cases retention (Postgres) is unchanged.

## Amendment to ADR-0002 (co-located, lands atomically when this ADR is Accepted)

The following block is added to `docs/adr/0002-language-per-component.md` (before its §References) in the same commit that flips this ADR to `Accepted`:

> **Amendment 2026-05-30 (ADR-0012): MVP detection slice hosted in TypeScript ingest (transitory).** ADR-0002's table assigns `services/pipeline/` to Go, and Rule 3 requires a superseding/amending ADR for any non-Go server-side artifact. ADR-0012 §1 amends this for the MVP detection slice only: the normalize→rule→score→alert slice is implemented in TypeScript inside `services/ingest/src/detect/`, not in Go `services/pipeline/`. Transitory and narrow — `services/pipeline/`'s Go assignment is otherwise unchanged. Named exit: when the event-firehose ADR lands with the Go toolchain, the slice is ported into `services/pipeline/` and removed from ingest. Mirrors ADR-0007's amendment of the `services/ingest/` row.

## Amendment 2026-05-31 (ADR-0013): the correlation window is two distinct tunables, not one

§8 defined a single 300 s correlation window and stated the dedup bucket and "the future hybrid join share one tunable"; §Out of scope stated stateful multi-event correlation "Uses the §8 window." [ADR-0013](0013-incident-correlation-windowing.md) §2 supersedes that single-tunable framing where they differ: the **dedup bucket** keeps its 300 s value and role (collapsing identical re-fires — unchanged), while **incident/stateful correlation** uses its **own, wider window** (value set in SPEC-007, per-org configurable). ADR-0012 remains `Accepted`; this amendment is additive and backward-compatible — the 300 s dedup tunable and every dedup test are untouched. Rationale: a 5-minute bucket is calibrated to collapse identical re-fires, not to span a multi-step attack chain of distinct alerts; conflating the two widths would fragment one incident into many.

## Amendment 2026-06-07: production detection driver (in-process scheduler)

### Context

Decision §2 (line 40) establishes that "a polling read against ClickHouse is sufficient" for the MVP detection event source, but does not specify which component performs the polling. runDetectionCycle (services/ingest/src/detect/index.ts) has had no production caller — it is exercised only by the detect/notify/incident acceptance tests. As a result MVP criteria 1, 2 and 4 (detection, incident, notification) are built and test-validated but do not execute in a running stack. This amendment closes that gap.

### Decision

A production detection driver is added to the ingest service: an in-process scheduler that invokes the existing transitional TypeScript detection slice on a recurring basis.

- Placement: started inside startIngest after the HTTP listeners bind (services/ingest/src/server.ts), stopped in the returned close() handler. It shares the already-constructed services clients (ClickHouse, Postgres, Redis) and the existing notify config; no new service and no new long-lived process.
- Loop: for each org present in agents.org_id, the driver calls runDetectionCycle(detectConfig, notify) and drains forward — re-invoking while a cycle returns a full batch (eventsEvaluated === BATCH_LIMIT), bounded by a per-tick iteration cap — then yields. The body of runDetectionCycle is unchanged; the durable per-org watermark (detect_watermark) remains the sole cursor.
- Scheduling: a self-rescheduling timer (next tick scheduled only after the current pass resolves) makes the driver single-flight in-process by construction; a cycle that runs longer than the interval cannot overlap.
- Interval and rules directory are environment variables with safe defaults (ports pattern), not operator-mandatory settings.

### Relationship to the §1 named exit

This amendment does NOT trigger the §1 seam extraction (line 33) or the §Out-of-scope event-firehose deferral (line 236). The driver runs the existing transitional TS slice in production; it ports nothing to Go. The named exit remains gated, unchanged, on the future event-firehose / NATS JetStream ADR that brings the Go toolchain. When that ADR lands, both the slice and this driver move to Go services/pipeline/ together, per §1.

### Scope

MVP is single-instance. Multi-instance / HA execution (serializing cycles across replicas) is deferred; when needed it is satisfied by the existing pg_try_advisory_lock pattern (migrate.ts) wrapped per-org per-tick, with no redesign. Multi-tenancy remains out of MVP scope; the per-org loop iterates whatever orgs exist (one, 'default', in v0.1).

## References

- [ADR-0002](0002-language-per-component.md) — Language per component. Assigns the pipeline engine to Go; §1 amends it for the MVP detection slice (transitory).
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage. §Retention Alerts row amended to Postgres-only for the MVP (§6).
- [ADR-0005](0005-detection-rules-and-ml-in-parallel.md) — Detection rules and ML in parallel. This ADR is the "future ADR for the normalize-before-correlate pipeline concern" its §Compliance anticipates; §4 clarifies its scoring composition (configured weights unchanged).
- [ADR-0006](0006-cges-ocsf-alignment.md) — CGES alignment with OCSF. The Alert/score/mitre classes this ADR emits live under its framework.
- [ADR-0007](0007-ingest-language-typescript-mvp.md) — Ingest language TypeScript for the MVP. §1 extends its transitory logic to the detection slice with the same firehose-deferred exit.
- [ADR-0009](0009-event-delivery-and-buffer.md) — Event delivery and buffer. `ReplacingMergeTree(arrived_at)` + at-least-once is why §2/§7 reads use `FINAL`. (Also: §References declares `event_id` UUIDv7; the agent code emits UUIDv4 — the §6 reconciliation flag's discrepancy.)
- [ADR-0011](0011-cges-process-activity-v0-1.md) — Process Activity v0.1. The class the MVP detects on; §3 flags a contradiction in its §4 ETW mapping.
- `schemas/cges/v0.1/classes/alert.json`, `schemas/cges/v0.1/common/cg_score.json`, `schemas/cges/v0.1/common/cg_mitre.json` — the alert/score/mitre shapes §6 mirrors.
- `schemas/cges/v0.1/examples/04_alert_rule_source.json`, `schemas/cges/v0.1/examples/05_alert_ml_source.json` — worked alert fixtures (non-normative; updated by SPEC-006 per §4/§5).
- `services/ingest/src/db/migrate.ts` (ClickHouse `cges_events` DDL), `services/ingest/src/db/migrations/0001_initial.ts` (Kysely Postgres migration pattern §6 extends).
- Microsoft-Windows-Kernel-Process provider manifest (`Get-WinEvent -ListProvider`) — source of the §3 column-population finding.
