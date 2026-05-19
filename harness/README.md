# Harness

Scenario-based end-to-end harness for CyberGuard.

The harness is the single most important asset in this project after the code itself. Every detection rule, every pipeline transformation, every alert grouping logic has an associated scenario validated by the harness.

CI blocks merges if a new rule or detector has no scenario.

## Layout

| Subfolder | Purpose |
|---|---|
| [`cmd/cg-harness/`](cmd/cg-harness/) | Go runner that loads scenarios and validates outputs. |
| [`scenarios/`](scenarios/) | One subdirectory per scenario (`SCXXX-name/`). |

## Scenario structure

Each scenario directory contains:

| File | Purpose |
|---|---|
| `manifest.yml` | Metadata: id, title, MITRE technique(s), expected `detection_source`. |
| `events.jsonl` | Input events in CGES format. |
| `expected_alerts.yml` | Alerts the pipeline must emit. |
| `expected_incident.yml` | Grouped incident the pipeline must build. |
| `expected_mitre.yml` | MITRE mapping the pipeline must attach. |
| `expected_report.snap.json` | Forensic report snapshot. |

A scenario declares whether it expects detection by rule, by ML, or by both. The harness validates both tracks independently.

Populated by SPEC-XXX-harness.
