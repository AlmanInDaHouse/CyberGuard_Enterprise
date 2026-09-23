# SPEC-015: Detection evaluator — generalized Sigma subset (process_creation)

- **ID:** SPEC-015
- **Title:** Detection evaluator — generalized Sigma subset (process_creation)
- **Status:** Accepted
- **Depends on:**
  - SPEC-006 — amends §In scope `:26` and the evaluator note in §Data contracts `:134` **by scope**; the MVP rule and the `detect_ac_*` ACs are unchanged.
  - ADR-0012 §1 — the transitory TypeScript MVP detection slice this evaluator lives in; the Go exit condition is untouched.
  - ADR-0005 — the harness obligation; the general Sigma-to-Go engine stays a `services/pipeline/` SPEC.
  - SPEC-005 — the populated process fields the evaluator keys on.
  - `docs/product/roadmap.md` §B1 — the phase this SPEC is the contract for.
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

SPEC-006 shipped an evaluator sufficient for exactly one rule: only `|endswith` over `Image` and `ParentImage`, both keys required, a single `selection`, and `logsource.category` pinned to `process_creation` (`services/ingest/src/detect/engine.ts:32-51` at `1fbb035`). MVP criterion 1 needs ten rules over the already-normalized process fields (roadmap §C), and criterion 2 (roadmap §D) needs the logsource unpinned so non-process classes can dispatch their own evaluator. This SPEC (roadmap phase B1) generalizes the evaluator with **no new capture** and **no read-model change** — it widens what the loader accepts and what the evaluator can express; nothing downstream of the evaluator changes.

## Scope

### In scope

- **Fields (for `process_creation`):** `Image` and `ParentImage` — the populated, detection-relevant fields (SPEC-006 §Data contracts; ADR-0012 `:61`). A `selection` may use one or both.
- **Modifiers:** none (exact), `endswith`, `startswith`, `contains` — all case-insensitive. A value list is OR; the fields of a selection are AND.
- **Detection:** one or more named blocks, each a map of field → string or list of strings. Block names match `^[A-Za-z_][A-Za-z0-9_]*$` and are not reserved words (`and`, `or`, `not`, `of`, `all`, `them`). A `condition` over those names uses `and` / `or` / `not` / parentheses. Every block MUST be referenced by the condition, and the condition MUST reference only defined blocks.
- **Logsource dispatch:** `logsource.category` picks an evaluator from a table. Only `process_creation` is implemented here; roadmap §D adds the others.
- **Fail-closed:** anything outside this subset is rejected at load with `UnsupportedRuleError` naming the offending construct.

### Out of scope

Each is rejected at load; its destination is in brackets.

- `CommandLine` and `User` [B2 — structurally empty in v0.1; the rejection message says so]. Any other field [future SPECs].
- Modifiers other than the four above.
- Wildcards `*` and `?` in values — Sigma treats them as patterns, so literal matching would silently never fire.
- List-of-maps blocks, keyword-only blocks, non-string values, and empty values or lists — an empty string matches every value under `contains` / `startswith` / `endswith`.
- `1 of` / `all of` / `them`, aggregations, `timeframe`.
- Other `logsource` categories [D].
- Widening the read-model or the normalized record [B2 / D].
- Multi-hop lineage [C decides it per rule; it would need a read-model change].
- The Sigma-to-Go engine [ADR-0005].

## Data contracts

- **Field map.** `Image` ← `image_file_name` (`imageFileName`); `ParentImage` ← the joined parent's `image_file_name` (`parentImage`; `null` when the parent was not captured).
- **Semantics.** Exact = full-string equality; every comparison is case-insensitive. A `null` field makes every item on that field false. A `selection` is the AND of its fields; a field is the OR of its values; a `condition` is a boolean expression over block results.
- **Consequence (stated explicitly).** A filter keyed on `ParentImage` never suppresses when the parent is unknown. This is deliberate: a security evaluator prefers a false positive to a false negative.
- **Rule document.** Standard Sigma plus the `cg:` block, unchanged from SPEC-006. The office rule (`rules/windows/office_spawns_script_host.yml`) is valid unchanged and evaluates identically.

## Operational

- Rejection happens in `loadRules` (`engine.ts`), so the driver's fail-loud startup applies unchanged: a single out-of-subset rule stops ingest from starting, exactly as today.
- The `activity_id = 1` guard in `evaluateRule` stays for `process_creation`.
- No change to the read-model, scorer, alerts, incidents, or notify.

## Acceptance criteria

Each maps 1:1 to a test under `services/ingest/test/`, named `eval_ac_NNN_*`.

- **eval_ac_001 (modifiers).** Every modifier matches and misses, case-insensitively, over `Image` and `ParentImage`; a selection with a single field is valid.
- **eval_ac_002 (condition).** `and` / `or` / `not` / parentheses, including `selection and not filter`.
- **eval_ac_003 (fail-closed).** Each out-of-scope construct — `CommandLine`, `User`, another field, another modifier, a wildcard, an empty value, `1 of`, an aggregation, an undefined or unreferenced block, another logsource — is rejected at load, and the error names it.
- **eval_ac_004 (unknown parent).** A `ParentImage` item is false, and `selection and not filter_parent` fires when the parent is unknown.
- **eval_ac_005 (regression).** The office rule behaves the same, and SPEC-006 `detect_ac_001..006` are unchanged and green (`detect_ac_001` is the elevated marquee, run by Manuel).

## Test scenarios

B1 adds no rule. ADR-0005's §Harness obligation applies to the rules that roadmap §C adds over this evaluator.

## Risks

| Risk | Mitigation |
| --- | --- |
| A bug in the condition parser yields false negatives | Exhaustive tests, and rejection on any parse error |
| The unknown-parent semantics can raise false positives | Deliberate (§Data contracts): a security evaluator prefers a false positive to a false negative |
| Some community Sigma rules will be rejected | Roadmap §C adapts them to the subset |

## Open questions

1. Multi-hop lineage — deferred to roadmap §C (it would need a read-model change).
2. Exposing `ProcessId` / `ParentProcessId` — excluded (no detection value).

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern, per SPEC-005..014):

1. **A new SPEC amending SPEC-006 by scope, not in place** — precedent: SPEC-010 → SPEC-009.
2. **Fields `Image` + `ParentImage` only** — `CommandLine` / `User` rejected until B2.
3. **Four modifiers, case-insensitive** — wildcards rejected.
4. **`and` / `or` / `not` / parentheses** — no `1 of` / `all of` / aggregations.
5. **Unknown parent ⇒ `ParentImage` items false** — prefer a false positive to a false negative.
6. **Logsource dispatch, only `process_creation` implemented** — D adds the others.
7. **No read-model change in B1** — the marquee path is untouched.
8. **Fail-closed at load, fail-loud at boot** — unchanged from SPEC-006.
9. **Doc-only this gate** — the code is the next gate (review branch, relay rule 5) and includes the elevated marquee.

## References

- [SPEC-006](SPEC-006-detection-mvp.md) — the evaluator this amends by scope (§In scope `:26`, §Data contracts `:134`); its MVP rule and `detect_ac_*` ACs are unchanged.
- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — §1 the transitory TypeScript detect slice; the Go exit condition is untouched.
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — the §Harness obligation; the Sigma-to-Go engine stays a `services/pipeline/` SPEC.
- [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — the populated process fields the evaluator keys on.
- [roadmap](../product/roadmap.md) — §B1 (this SPEC's phase), §C (the ten rules), §D (new classes / logsource unpinning).
- `services/ingest/src/detect/engine.ts`, `services/ingest/src/detect/read-model.ts` — the evaluator and read-model this SPEC governs.
