# Detection rules

Sigma-compatible detection rules consumed by the detection slice in [`services/ingest/src/detect/`](../services/ingest/src/detect/).

## Layout

| Subfolder | Purpose |
|---|---|
| [`windows/`](windows/) | Windows-targeted rules. |
| [`linux/`](linux/) | Linux-targeted rules. |
| [`network/`](network/) | Network-targeted rules. |
| [`tests/`](tests/) | Per-rule `.test.json` fixtures. |

## Convention

Every rule file has a sibling `.test.json` in [`tests/`](tests/) containing input events and expected matches. CI blocks merges if a new rule has no test.

Every rule sets `cg_detection_source: rule` inside its `cg:` block (ADR-0012 §Compliance; ADR-0005 names the same concept `detection_source`). The loader rejects a rule without it, and any other value: `ml` and `hybrid` need a model pairing that no rule can carry. The full loader contract (id, level and severity, the `cg:` keys, the ATT&CK vocabulary) is SPEC-016 §Data contracts §1.

Populated by the detection SPECs. SPEC-006 (Detection MVP) lands the first rule, [`windows/office_spawns_script_host.yml`](windows/office_spawns_script_host.yml), with its sibling fixture [`tests/office_spawns_script_host.test.json`](tests/office_spawns_script_host.test.json).
