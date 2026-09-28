# Scenarios

End-to-end scenarios driving the harness.

## Naming

`SCXXX-short-kebab-title/`, where `XXX` is a zero-padded sequential id starting at `001`.

## Catalog

| Scenario | Title | Tracks (rule / ml / hybrid) |
|---|---|---|
| [SC001](SC001-office-spawns-script-host/) | Office spawns a script host | rule |
| [SC010](SC010-benign-script-host/) | Benign script host (false positive) | rule |
| [SC011](SC011-office-spawns-lolbin/) | Office spawns a LOLBin | rule |
| [SC012](SC012-script-host-spawns-powershell/) | Script host spawns PowerShell | rule |
| [SC013](SC013-staged-payload-execution/) | Staged payload executed from a document | rule |
| [SC014](SC014-deceptive-executable-name/) | Executable with a deceptive name | rule |
| [SC015](SC015-system-binary-masquerading/) | System binary name outside the system folders | rule |
| [SC016](SC016-exec-from-suspicious-folder/) | Execution from a suspicious folder | rule |
| [SC017](SC017-exec-from-startup-folder/) | Executable launched from a Startup folder | rule |
| [SC018](SC018-psexec-like-service/) | PsExec-like remote service executed | rule |
| [SC019](SC019-credential-theft-tool/) | Credential theft tool executed | rule |

Populated by the SPECs of the detectors each scenario validates. SC001 / SC010 land with SPEC-006 (Detection MVP); SC011–SC019 land with SPEC-016 (rule set v1). SC002–SC009 stay reserved for the blueprint §14 scenarios. Every scenario runs in CI through `rules_ac_003` (`services/ingest/test/rules-ac-003-scenarios.test.ts`), against the whole of `rules/windows/` (SPEC-016 §Data contracts §4).

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
