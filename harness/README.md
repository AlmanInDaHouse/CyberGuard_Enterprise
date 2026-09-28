# Harness

Scenario-based end-to-end harness for CyberGuard.

The harness is the single most important asset in this project after the code itself. Every detection rule has at least one scenario that the harness validates (ADR-0005 §Harness obligation).

## Layout

| Subfolder | Purpose |
|---|---|
| [`scenarios/`](scenarios/) | One subdirectory per scenario (`SCXXX-name/`), each holding a `scenario.json`. |
| [`cmd/cg-harness/`](cmd/cg-harness/) | Placeholder for the Go runner of ADR-0002 Rule 3. Deferred: ADR-0012 §Compliance keeps the Go toolchain out until the event-firehose ADR. |

## How scenarios run today

The scenario format is the contract in [`scenarios/README.md`](scenarios/README.md). Every scenario runs in CI through `rules_ac_003` (`services/ingest/test/rules-ac-003-scenarios.test.ts`, SPEC-016 §Data contracts §4): its events are inserted into ClickHouse, one detection cycle runs against the whole of `rules/windows/`, and the alerts must match the scenario's expectation exactly, so a positive scenario also proves that no other rule fires. `rules_ac_003` fails when a rule has no positive scenario, and `ts-ci` runs on changes to `rules/**` and `harness/**`.

A scenario declares whether it expects detection by rule, by ML, or by both (`track`, `expected_detection_source`). Only the rule track exists today.

The blueprint's richer layout (`manifest.yml`, `events.jsonl`, `expected_*.yml`, a report snapshot) is not used; incident, MITRE and report expectations would be future extensions of `scenario.json`.
