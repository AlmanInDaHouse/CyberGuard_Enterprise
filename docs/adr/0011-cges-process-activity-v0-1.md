# ADR-0011: Per-class CGES jurisprudence — Process Activity v0.1 (ETW mapping, PPID race, process.uid recipe)

- Status: Accepted
- Date: 2026-05-23
- Last updated: 2026-05-23
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0006 established CGES as OCSF-aligned (v1.3) with `cg_*` extensions, deliberately abstracting per-class field details (§Out of scope first bullet). Concrete CGES event classes have been pre-scaffolded since Session 3 — `schemas/cges/v0.1/classes/1007_process_activity.json` and five siblings exist, each referenced from `event.json`'s `oneOf` — but the agent-side production of any concrete class has not yet started.

SPEC-005 (forthcoming) introduces the first wire-level event-emitting workload: process telemetry from Windows `Microsoft-Windows-Kernel-Process` ETW, ratified in Session 10 via D2 with four locked sub-decisions and three sub-decisions flagged as "may turn non-trivial." Session 11's Phase 3.1.1 audit of ADR-0006 surfaced that the framework absorbs class introductions by populating pre-existing schema slots, not by amending ADR-0006 — and that D2's per-class decisions, both locked and non-trivial, belong in a dedicated ADR rather than in ADR-0006 (which would violate ADR-0006 §Out-of-scope) or in SPEC-005 alone (which would conflate jurisprudence with production specification).

This ADR establishes a new pattern: **per-class jurisprudence ADRs**. Process Activity (OCSF 1007) is the first instance. The pattern records normative behaviour specific to one CGES class — what the agent emits and how, which OCSF-permissive shapes the agent narrows for production, and where the agent-side runtime contract is stricter than the JSON Schema floor — separate from CGES framework jurisprudence (ADR-0006) and separate from SPEC-005's production specification.

## Decision

### 1. Per-class pattern

This ADR establishes the per-class jurisprudence pattern for concrete CGES event classes. Process Activity (OCSF 1007) is the first instance, driven by SPEC-005's process telemetry scope. Future concrete CGES classes — whichever ones become load-bearing for future SPECs — each merit their own ADR, numbered when proposed. ADR-0006 (framework) remains abstract; per-class ADRs declare the normative behaviour specific to each class. There is no schedule or planned ordering; the pattern is class-on-demand.

When (for instance) a future SPEC introduces File System Activity (OCSF 1001) telemetry, a dedicated per-class ADR would record the analogous per-class normative for that class. Hypothetical framing only — this ADR does not pre-commit to which classes, in which order, or under which ADR numbers.

### 2. Process Activity v0.1 — no `cg_raw_ref` policy

For Process Activity events, the agent emits `raw_data` inline and never `cg_raw_ref`. ADR-0006's mutual-exclusion contract permits either; this ADR pins the per-class choice. Justification: ETW Kernel-Process Launch and Terminate payloads, even with full command line and image path, fit comfortably within the 64 KB inline cap (typical sub-1 KB; pathological case <16 KB even with maximal Windows path lengths and 32 KB command line). The MinIO offload mechanism's added complexity — an upstream PUT, a downstream resolve, hash verification — buys nothing for this class.

This is a v0.1 per-class policy. Future event classes with genuinely large payloads (full-file binary capture, large network-packet dumps) re-evaluate `cg_raw_ref` per their own per-class ADRs.

### 3. Process Activity v0.1 — single schema, `activity_id` discriminator

Launch (activity_id=1) and Terminate (activity_id=2) share the single schema file `schemas/cges/v0.1/classes/1007_process_activity.json`. The activity is discriminated by `activity_id`, never by a separate class file. Per-event field optionality is layered on top of the class shape via JSON Schema's standard `if/then/else` constructs if needed (e.g., `exit_code` only meaningful for activity_id=2).

The class file's existing `activity_id` enum permits OCSF's full set: `[0, 1, 2, 3, 4, 5, 99]` (Unknown, Launch, Terminate, Open, Inject, Set User ID, Other). **v0.1 agent emits only `1` and `2`.** The enum stays OCSF-permissive at the schema level; the agent emission scope is narrower. This dual-layer (permissive schema, narrow agent) mirrors the PPID race decision in §5: the JSON Schema is the syntactic floor; the agent contract is the stricter normative.

The Phase 0 spike ([docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md)) empirically validated that `Microsoft-Windows-Kernel-Process` exposes Launch and Terminate as the two foundational events; the other OCSF activity_ids (Open, Inject, Set User ID) require different ETW providers or instrumentation beyond Kernel-Process and are out of scope for v0.1.

Narrowing the schema enum to `[1, 2]` would tightly couple v0.1's emission scope to the schema. Any future agent that emits other activity_ids would require a schema update — even though the broader CGES contract (the class shape, the per-field semantics) is unchanged. The permissive-schema + narrow-agent dual-layer (chosen) decouples emission-scope evolution from schema-version evolution.

### 4. Process Activity v0.1 — ETW field mapping table

The agent maps ETW Kernel-Process Launch and Terminate event fields to CGES fields per D2's locked subset, as follows:

| CGES path | ETW field | Notes |
|---|---|---|
| `process.pid` | `ProcessID` | Decimal integer. |
| `process.name` | `basename(ImageFileName)` | Last path segment of the kernel device path; e.g., `\Device\HarddiskVolume2\Windows\System32\notepad.exe` → `notepad.exe`. |
| `process.file.path` | `ImageFileName` | Full kernel device path, translated to Win32 form (e.g., `\Device\HarddiskVolume2\Windows\System32\notepad.exe` → `C:\Windows\System32\notepad.exe`). The translation mechanism is SPEC-005's concern. |
| `process.parent_process.pid` | `ParentProcessID` | Decimal integer. See §5 for the PPID race resolution. |
| `process.cmd_line` | `CommandLine` | Raw string, no redaction in v0.1 (see §Out of scope for the PII deferral). |
| `process.user.uid` | `SubjectUserSid` | SID in string form, e.g., `S-1-5-21-1004336348-1177238915-682003330-1013`. |
| `process.created_time` | ETW event timestamp | For activity_id=1 (Launch): the event's own timestamp. For activity_id=2 (Terminate): the original Launch timestamp if available to the agent at Terminate time (the retention mechanism is SPEC-005's concern, analogous to the kernel-device-path translation in row 3); otherwise `null`. |
| `process.exit_code` | `ExitStatus` | Decimal integer. Only meaningful for activity_id=2 (Terminate); absent for activity_id=1. |
| `process.uid` | derived per §6 | Deterministic recipe, not a direct ETW mapping. |

Fields outside this subset that ETW Kernel-Process surfaces (e.g., `TokenInformation`, `PackageFullName`, `Flags`) are out of scope for v0.1. Adding them later is additive at the schema layer (OCSF objects already use `additionalProperties: true` per `schemas/cges/v0.1/README.md` §Conventions) and additive at the agent layer (new mapping rows in this table or in a future per-class ADR amendment).

### 5. PPID race resolution: schema relaxed, agent-side stricter normative

ETW always provides `ParentProcessID` at child Launch events. The parent process may already be terminated (PID reused, parent dead, race between child Launch and parent's lifetime end) when the child Launch fires. Under the current `schemas/cges/v0.1/objects/process.json` requiring both `pid` and `name`, the recursive `$ref` makes `parent_process.name` unsatisfiable when the parent is unresolvable.

**Schema change** (realised in Phase 3.2): `objects/process.json` `"required"` becomes `["pid"]`. The recursive `$ref` propagates the relaxation: `parent_process` accepts `pid`-only.

**Agent-side stricter normative** (binding on v0.1 agent code):

- The agent MUST always emit `name` for the **top-level `process` field**. Launch events always carry `ImageFileName`; failure to produce a top-level `name` indicates a captured event the agent cannot honestly represent — the event is logged-error and dropped before envelope construction. Stricter-than-schema enforcement at emit time.
- The agent MAY omit `name` for `parent_process` only when ETW resolution fails (e.g., `ParentProcessID` does not match any live process the agent can introspect at capture time). When omitted, only `parent_process.pid` is emitted; no sentinel value, no resolution flag.

This dual-layer design preserves OCSF alignment (OCSF v1.3 does not mandate `name` on Process; CGES previously over-required it as an implicit drift that Session 11's audit surfaced), avoids the operational hazards of sentinel values (ClickHouse cardinality inflation, dashboard contamination) and the framework asymmetry of an ad-hoc resolution-flag field, and pins the agent contract in this ADR rather than scattering it across the schema and SPEC-005.

CI fixtures (Phase 3.2 / 3.4) cover both branches: events with resolved parents validate fully; events with unresolved parents validate against the relaxed schema and additionally satisfy the agent's runtime stricter contract for the top-level field.

### 6. `process.uid` recipe

OCSF Process defines `uid` as a string identifier for the process instance. CGES events about the same logical process (Launch and Terminate of one PID, future Kernel-Network events tagged with the same process) must share `process.uid` for correlation.

**Format spec, v0.1 (binding, byte-level):**

```text
<agent_id_canonical>:<pid_decimal>:<created_time_unix_nanos_utc>
```

Where:

- `agent_id_canonical`: UUIDv7 in 8-4-4-4-12 hyphenated lowercase canonical form (e.g., `01934abc-def0-7000-89ab-000000000001`). Matches the `cg_agent.agent_id` wire format verbatim.
- `pid_decimal`: process ID as base-10 integer, no padding, no sign (e.g., `7144`). Range: 1 to 2^32-1 (Windows upper bound; Linux's PID_MAX is narrower).
- `created_time_unix_nanos_utc`: process creation time in nanoseconds since Unix epoch, UTC strict (NOT local time), as base-10 integer, no padding (e.g., `1716123612901000000`).

Total length: 36 (agent_id) + 1 (separator) + 1–10 (pid) + 1 (separator) + 19 (nanos) = 58–67 characters.

**Reproducibility requirement.** Two agents observing the same logical event MUST produce byte-identical `process.uid` strings. SPEC-005 test fixtures pin this expectation by comparing the agent's emitted value against a hand-computed expected string byte-for-byte. ETW timestamps must be converted to UTC nanoseconds via a documented mechanism (specified in SPEC-005, since the conversion happens agent-side); this ADR locks the OUTPUT format, not the conversion mechanism.

**Cross-event stability.** Launch and Terminate of one process instance produce the same `process.uid` because both events carry the same `(PID, creation_time)` tuple — the creation_time is invariant across an instance's lifetime, and the agent_id is invariant per agent. The recipe is fully deterministic from capture inputs; no agent-side mapping table, no state across restarts.

**Cross-agent disambiguation.** Two agents A and B observing same-PID-and-similar-time process activity on different hosts produce different `process.uid` strings via the `agent_id` prefix. Collisions are prevented by the UUIDv7 `agent_id`'s birthday-bound uniqueness across the agent fleet.

## Alternatives considered

### A1 — Bundle per-class decisions into ADR-0006 amendment

Pros: one document anchors all CGES jurisprudence; readers find everything in one place.

Cons: violates ADR-0006 §Out-of-scope first bullet, which explicitly defers per-class field details to the schema files. Per-class jurisprudence beyond the schema (PPID race, uid recipe, no-`cg_raw_ref` policy) would still need a home; expanding ADR-0006 to hold them would conflate framework-level decisions with class-specific ones. Scaling problem: future concrete classes accreting into ADR-0006 makes the document grow unboundedly and obscures the framework-versus-per-class distinction.

Rejected. Phase 3.1.1's audit (Session 11) surfaced exactly this scope-conflict before any text was drafted.

### A2 — Bundle per-class decisions into SPEC-005

Pros: the consumer of these decisions (the agent-side process telemetry workload) is one document; SPECs already carry ACs and harness scenarios.

Cons: SPECs specify production (the agent emits X under conditions Y); ADRs specify jurisprudence (the contract is Z because reasons R). Per-class decisions about `process.uid` format, PPID race resolution, and the no-`cg_raw_ref` policy are jurisprudence — they bind any future agent re-implementation, any future cross-language port, any future test fixture set, not only SPEC-005's specific deliverables. SPECs are also expected to evolve faster than ADRs (each SPEC is a feature; ADRs are stabler). Encoding jurisprudence in SPECs causes drift when the SPEC's production-side details change but the underlying contract should not.

Rejected. The pattern this ADR establishes (jurisprudence in ADRs; production in SPECs) is the same separation ADR-0009 + SPEC-005's forthcoming production-side rules use.

### A3 — Sentinel value `"<unresolved>"` for unresolvable parent name (PPID race)

Pros: no schema change; explicit signal queryable in ClickHouse via `WHERE parent_process.name = '<unresolved>'`.

Cons: ClickHouse aggregations over `parent_process.name` treat `<unresolved>` as a legitimate value, inflating cardinality stats and contaminating dashboards. "Top parent processes by child count" queries surface `<unresolved>` as a real entry unless every consumer remembers to filter — silent-bug-by-omission risk that compounds over time. Sentinel collides if any legitimate process is literally named `<unresolved>` (vanishingly unlikely but adversarial-possible). Not OCSF-idiomatic.

Rejected.

### A4 — Discriminator field `name_resolved: bool` (PPID race)

Pros: explicit, queryable, no sentinel collision.

Cons: introduces a field in `parent_process` that does not exist in `process` — breaks the recursive `$ref` symmetry. Does not scale: future resolution-status concerns (user SID resolution, image-path resolution) each become their own boolean; ad-hoc accretion. The cost of the OCSF-divergence (a CGES-specific extension on an OCSF object) is not justified when §5's schema-relax + agent-normative split works without the divergence.

Rejected.

### A5 — `pid:created_time_unix_nanos` only (process.uid; no agent_id prefix)

Pros: shorter string; deterministic; cross-event stable; debuggable.

Cons: not cross-agent disambiguated. Two agents A and B observing same-PID-and-similar-time process activity on different hosts could collide. Vanishingly unlikely on the wall-clock dimension (creation_time at nanosecond resolution makes accidental collision a 10^-18 event per second), but the CGES correlation contract is value-based, not statistical — a collision, however rare, breaks the correlation invariant silently and degrades trust in the schema. Adding `agent_id` is cheap (extra 37 bytes per uid) and forecloses the failure mode.

Rejected.

### A6 — `UUIDv5(namespace=agent_id, name="pid:created_time_unix_nanos")` (process.uid)

Pros: fixed length 36; opaque; cross-agent disambiguated via namespace; blends with `event_id` (UUIDv7) at storage layer.

Cons: requires a UUIDv5 implementation in the agent — cheap (sha1-based; ~30 lines of Rust on top of any sha1 crate, or one of the existing `uuid` crate's optional features). Debuggability hurts: `WHERE process.uid LIKE '...:7144:...'` queries no longer work; debugging requires decoding via the agent's exact UUIDv5 algorithm. The structured-string form's transparency (the recipe in §6) is more valuable for v0.1 forensic work than the storage uniformity gain from fixed-length UUIDs.

Rejected. May be revisited if a future per-class ADR has a stronger fixed-length requirement (e.g., a query path that benefits from sortable UUIDv7-like uids).

### A7 — UUIDv7 generated at process Launch, agent retains `(PID, boot_epoch) → uid` mapping (process.uid)

Pros: opaque, fixed length, blends with `event_id`.

Cons: NOT deterministic from capture inputs — the agent must remember `(PID, boot_epoch) → uid` so that Terminate can look up the Launch-time uid and emit a matching value. If the agent restarts between Launch and Terminate (a routine event on developer machines and not unheard-of on production endpoints), the mapping is lost; Terminate emits a fresh uid that does not match Launch's; cross-event stability is broken. Conflicts directly with ADR-0009 §Decision part 3 ("the agent has no on-disk event state in MVP").

Rejected. The stability failure under agent restart is decisive.

### A8 — Activity_id schema narrowing to `[1, 2]` for v0.1

Pros: schema-level enforcement of the v0.1 emission scope; CI rejects any future agent attempting to emit Open/Inject/Set User ID without first amending the schema; one source of truth.

Cons: tightly couples emission-scope evolution to schema-version evolution. Future ETW providers expanding the agent's reachable activity_id set would require a schema update — even though the broader CGES contract is unchanged. The permissive-schema + narrow-agent dual-layer (chosen in §3) decouples these axes; consistency matters across §3 and §5.

Rejected. The decoupling-versus-CI-enforcement trade-off favours the permissive-schema path for v0.x lifetime.

## Consequences

### Positive

- ADR-0006 stays clean as the framework ADR. Per-class jurisprudence accretes into per-class ADRs, not into ADR-0006. Future readers know where to look.
- The per-class pattern this ADR establishes is class-on-demand: no pre-committed schedule, no ADR-number reservations for future classes that may never be needed.
- Process Activity v0.1 has a complete jurisprudence record: D2's four locked sub-decisions are codified here; PPID race resolution and `process.uid` recipe are explicit; ETW mapping is a single table consumable by SPEC-005's harness fixtures and any future cross-language port.
- The schema relaxation in §5 closes a previously-implicit CGES drift (over-requiring `name` on Process beyond OCSF v1.3's actual contract). Honest schema work, not just race-condition resolution.
- The `process.uid` format spec in §6 is byte-level reproducible; SPEC-005 fixtures pin it directly without arbitrating implementation choices at test time.

### Negative

- One more document for readers to discover. Mitigated by: the ADR catalog ([docs/adr/README.md](README.md)) lists it; the dep-graph edges declare its relationships; future per-class ADRs cross-reference this one as the pattern's first instance.
- The activity_id permissive-schema + narrow-agent dual-layer (§3) requires CI to catch agent regressions: an agent bug emitting activity_id=3 would validate against the schema but violate the v0.1 contract. Agent-side runtime validation closes this; CI fixtures cover the assertion.
- The `process.uid` format-spec rigidity (§6) means any future format change is a CGES breaking change. Mitigated by: the format is deliberately the most generic structured-string form available (no UUID encoding to lock in, no opaque hashes); future tightening (Candidate A6 UUIDv5 wrapping) is layerable on top of the string format without breaking byte-for-byte reproducibility of the v0.1 form.

### Neutral

- The class schema file `schemas/cges/v0.1/classes/1007_process_activity.json` does not change under this ADR. The `activity_id` enum stays OCSF-permissive per §3's choice; the per-class structure stays as scaffolded in Session 3. Schema changes for this ADR (the `objects/process.json` `required` relaxation; the `created_time`, `exit_code`, and `uid` fields on `objects/process.json`) land in Phase 3.2.
- CommandLine PII handling stays under ADR-0006 §Out-of-scope's "Field-level encryption for PII. Deferred per Blueprint §17.11" umbrella; SPEC-005 records the v0.1 accepted-risk decision. This ADR mentions but does not re-decide PII handling.
- The kernel device path → Win32 path translation mechanism (§4 row 3) is agent-side; SPEC-005 specifies. This ADR locks the CGES-side output (Win32 form) but not the translation mechanism.

## Compliance

- The agent's process telemetry implementation MUST honour §4's ETW field mapping table for v0.1. Additions go in a per-class amendment to this ADR (or a future per-class ADR if the addition crosses class boundaries), not in agent code without ADR backing.
- The agent's runtime contract is stricter than the JSON Schema floor in two places: §5 (top-level `process.name` always present) and §3 (activity_id emission narrowed to `[1, 2]` despite schema permissiveness). Agent code MUST enforce both at emit time — log-and-drop on violation, before envelope construction.
- The `process.uid` format spec in §6 is binding byte-for-byte. SPEC-005 test fixtures pin it; any agent emission deviating from the format breaks the correlation contract.
- Future per-class CGES ADRs follow the pattern this ADR establishes. Each declares: scope (one OCSF class or one CGES-specific class), per-class policies (raw_data shape, discriminator strategy, ETW or other source mapping), and agent-side normatives stricter than the schema floor.
- Schema changes implementing this ADR (Phase 3.2): `objects/process.json` `required` → `["pid"]`; add `created_time`, `exit_code`, `uid` fields with type/format constraints per §4 and §6.

## Out of scope

- CommandLine PII handling. Captured as-is in v0.1 per ADR-0006 §Out-of-scope (Blueprint §17.11). Redaction patterns, opt-out config, and downstream sanitisation are deferred.
- Kernel device path → Win32 path translation mechanism. Agent-side concern; SPEC-005 specifies.
- Other ETW providers (network, file, registry, image-load). SPEC-005 is Kernel-Process only; future SPECs introduce other providers with their own per-class ADRs as the classes become load-bearing.
- Other Process Activity activity_ids (`0` Unknown, `3` Open, `4` Inject, `5` Set User ID, `99` Other). The schema permits them per §3; the v0.1 agent narrows to `[1, 2]` per the same section. Adding more requires either a per-class amendment to this ADR or a successor per-class ADR.
- Persistent disk-backed buffering interaction with `process.uid` reproducibility across agent restart. Persistent buffer is deferred per ADR-0009 §Decision part 4 + ADR-0004 amendment 2026-05-23 part (a); when it lands, the future SPEC must re-evaluate whether the in-memory determinism of §6 still holds, and may require amending §6 if the answer changes.

## Amendment 2026-05-23: §4 field format declarations (process.created_time integer-nanos UTC; process.exit_code signed int32)

**Status.** This amendment supersedes parts of §4 (rows 1 and 2 of the ETW field mapping table — the Notes columns for `process.created_time` and `process.exit_code`). ADR-0011 remains `Accepted`; the amendment convention is in-place per [docs/engineering-notes.md](../engineering-notes.md).

**Context.** Phase 3.2 (the schema work realising §5 + §Compliance line 190) surfaced that §4 rows 1 and 2 declare the ETW source-field mapping but lack the CGES-side type/format declaration that the JSON Schema constraints require as their authoritative contract. Two corrections, closed in one atomic move. **(a)** Row 1 (`process.created_time`) needs the OUTPUT format pinned (integer nanoseconds since Unix epoch, UTC strict) so the schema constraint and SPEC-005 test fixtures share a single source of truth with the §6 `process.uid` recipe. **(b)** Row 2 (`process.exit_code`) needs the integer WIDTH pinned (signed 32-bit per Windows `ExitStatus` / `LONG`) so the schema's `minimum` / `maximum` bounds derive from an explicit width declaration rather than being arbitrary.

**Amendment, part (a) — `process.created_time` format declaration.** The original row 1 Notes wording (`"For activity_id=1 (Launch): the event's own timestamp. For activity_id=2 (Terminate): the original Launch timestamp if available to the agent at Terminate time...; otherwise null."`) remains in place per the amendment convention; this amendment supersedes it where they differ. The binding format declaration for `process.created_time` is now:

> Emitted as integer nanoseconds since Unix epoch, UTC strict — the same format used inside `process.uid` per §6. The value MAY be `null` only for activity_id=2 (Terminate) events when the agent does not have the original Launch timestamp available (retention mechanism is SPEC-005's concern); for activity_id=1 (Launch) events the value is always the ETW event's own timestamp converted to UTC nanoseconds. The ETW-timestamp-to-UTC-nanos conversion mechanism is agent-side and specified in SPEC-005, consistent with §6's existing deferral on conversion mechanism.

This is a deliberate OCSF divergence. OCSF v1.3 Process defines `created_time` as a string (ISO 8601 / RFC 3339); CGES emits an integer for byte-level reproducibility of the §6 `process.uid` recipe — string formatting introduces timezone and sub-second-precision edge cases that the integer-nanos form forecloses by construction. The divergence is justified by the determinism contract in §6. This divergence is recorded here rather than in ADR-0006's framework-level enumeration, consistent with §Decision part 1 of this ADR (per-class jurisprudence lives in per-class ADRs; ADR-0006 §Deliberate divergence from OCSF establishes the pattern that per-class ADRs apply at their respective scopes).

**Amendment, part (b) — `process.exit_code` width declaration.** The original row 2 Notes wording (`"Decimal integer. Only meaningful for activity_id=2 (Terminate); absent for activity_id=1."`) remains in place per the amendment convention; this amendment supersedes the `"Decimal integer"` portion where it lacks width detail. The binding width declaration for `process.exit_code` is now:

> Emitted as a signed 32-bit integer (Windows `ExitStatus` is `LONG`, i.e. `int32_t` signed). Values range over `[-2147483648, 2147483647]`. NTSTATUS codes that appear in the `0x8xxxxxxx` – `0xFxxxxxxx` unsigned-hex Windows convention surface here as negative integers when interpreted as signed; display semantics (signed decimal vs. unsigned hex) are a consumer choice, not part of the CGES contract.

This is a constraint narrowing within OCSF's `integer` type, not a divergence. OCSF Process defines `exit_code` as `Integer` without explicit width; signed int32 is a subset of OCSF-valid values, so any CGES `exit_code` is also an OCSF-valid `exit_code`. The narrowing reflects the literal ETW source (`Microsoft-Windows-Kernel-Process` `ExitStatus` field — see §References) and catches agent regressions that would emit out-of-bounds values at schema-validation time. The conditional emission contract (`exit_code` MUST be present when activity_id=2 and MUST be absent when activity_id=1) is documented in SPEC-005 §Acceptance Criteria as a stricter-than-schema agent normative, consistent with the §3 + §5 permissive-schema + agent-stricter pattern.

**Backward compatibility.** Strictly tightening on the declaration side. The `process.created_time` format moves from implicit (`"ETW event timestamp"`) to explicit (integer nanos UTC); the `process.exit_code` width moves from implicit (`"Decimal integer"`) to explicit (signed int32). No previously-valid agent emission becomes invalid because no agent code emits these fields yet, no schema yet enforces them, and SPEC-005 is forthcoming. The amendment lands before the schema work in Phase 3.2.B so the deliverable chain (ADR jurisprudence → JSON Schema constraints → SPEC-005 test fixtures) starts from a single, consistent contract.

**Effect on other sections.**

- §6 (`process.uid` recipe) — unchanged. Line 86's `created_time_unix_nanos_utc` declaration is the consistency anchor part (a) cites; §6 was already authoritative on the format used inside `process.uid` and remains so. Part (a) makes the standalone `process.created_time` field match the same format that §6 already requires for its embedded component.
- §Compliance — unchanged. Line 190's reference to `"type/format constraints per §4 and §6"` now resolves to concrete declarations on both sides.
- §Out of scope — unchanged. CommandLine PII deferral, kernel-device-path translation, other activity_ids, persistent-buffer reproducibility — none touched by the format declarations.
- §References — unchanged. The existing OCSF Process object reference carries the citation part (a) uses for OCSF's string format; the ETW Microsoft-Windows-Kernel-Process schema reference carries the citation part (b) uses for `ExitStatus` width semantics.
- §Decision parts 1, 2, 3, 5 — unchanged. The amendment touches only §4 rows 1 and 2; the per-class pattern (§1), no-`cg_raw_ref` policy (§2), activity_id discriminator (§3), and PPID race resolution (§5) are unaffected.

Phase 3.2.B (the next commit in this session) realises the amended §4 in the JSON Schema constraints on `objects/process.json`.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout. Places `schemas/cges/` and `agent/`.
- [ADR-0002](0002-language-per-component.md) — Language per component. Rust for the agent that produces Process Activity events.
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage. ClickHouse partitioning and ordering unchanged; `process.uid` joins via the order key at query time, not at partition time.
- [ADR-0006](0006-cges-ocsf-alignment.md) — CGES alignment with OCSF v1.3. Framework ADR; this ADR is the first per-class jurisprudence ADR under its §Out-of-scope first bullet's deferral.
- [ADR-0008](0008-etw-crate-selection.md) — ETW crate selection. ferrisetw 1.2.0 is the consumer for the `Microsoft-Windows-Kernel-Process` provider whose Launch and Terminate events are mapped in §4.
- [ADR-0009](0009-event-delivery-and-buffer.md) — Event delivery and buffer model. `event_id` UUIDv7 (capture-time); in-memory ring (ephemeral). §6's "no agent-side mapping" requirement aligns with ADR-0009 §Decision part 3's "no on-disk event state."
- [ADR-0010](0010-agent-privilege-model-mvp.md) — Agent privilege model. Elevated user process; ETW Kernel-Process requires elevation per the Phase 0 spike's empirical findings.
- [SPEC-005](../specs/SPEC-005-agent-process-telemetry-windows-etw.md) — First consumer of this ADR. Specifies agent-side ETW capture, the kernel-device-path → Win32 conversion mechanism, the v0.1 emission scope, the AC tests pinning §6's byte-level format, and the CommandLine PII accepted-risk decision.
- [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md) — Phase 0 spike. Empirical validation that Launch and Terminate are the two foundational events from `Microsoft-Windows-Kernel-Process`; informs §3's v0.1 narrowing.
- [OCSF v1.3 Process Activity class (1007)](https://schema.ocsf.io/1.3.0/classes/process_activity) — class semantics inherited.
- [OCSF v1.3 Process object](https://schema.ocsf.io/1.3.0/objects/process) — object inheritance baseline. §5's schema relaxation aligns CGES with OCSF Process's actual required set.
- [ETW Microsoft-Windows-Kernel-Process schema](https://learn.microsoft.com/en-us/windows/win32/etw/microsoft-windows-kernel-process) — Microsoft's published schema for the events §4 maps.
