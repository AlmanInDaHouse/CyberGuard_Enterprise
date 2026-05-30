# Scenarios

End-to-end scenarios driving the harness.

## Naming

`SCXXX-short-kebab-title/`, where `XXX` is a zero-padded sequential id starting at `001`.

## Catalog

| Scenario | Title | Tracks (rule / ml / hybrid) |
|---|---|---|
| [SC001](SC001-office-spawns-script-host/) | Office spawns a script host | rule |
| [SC010](SC010-benign-script-host/) | Benign script host (false positive) | rule |

Populated by the SPECs of the detectors each scenario validates. SC001 / SC010 land with SPEC-006 (Detection MVP).

## Scenario format

Each scenario is a directory `SCXXX-kebab-title/` containing a `scenario.json`. This is the **contract** the future Go `cg-harness` runner consumes — it must not have to reverse-engineer the format from examples:

```json
{
  "id": "SC001",
  "title": "human-readable title",
  "track": "rule | ml | hybrid",
  "expected_detection_source": "rule | ml | hybrid | null",
  "rule_id": "rule.<id>",
  "description": "what the scenario validates",
  "input": {
    "cges_events": [
      {
        "activity_id": 1,
        "process_name": "child.exe",
        "image_file_name": "C:\\path\\child.exe",
        "process_pid": 4099,
        "process_parent_pid": 4012
      }
    ]
  },
  "expected": {
    "alert": true,
    "alert_count": 1,
    "rule_id": "rule.<id>",
    "detection_source": "rule",
    "final_score": 0.9
  }
}
```

`input.cges_events` are rows in the realized `cges_events` column shape (SPEC-006 §Data contracts). A false-positive scenario sets `expected.alert: false` with `expected.alert_count: 0` and `expected_detection_source: null`. Per ADR-0005 §Harness obligation, every scenario declares its `expected_detection_source`.
