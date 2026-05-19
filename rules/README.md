# Detection rules

Sigma-compatible detection rules consumed by the rule engine in [`services/pipeline/`](../services/pipeline/).

## Layout

| Subfolder | Purpose |
|---|---|
| [`windows/`](windows/) | Windows-targeted rules. |
| [`linux/`](linux/) | Linux-targeted rules. |
| [`network/`](network/) | Network-targeted rules. |
| [`tests/`](tests/) | Per-rule `.test.json` fixtures. |

## Convention

Every rule file has a sibling `.test.json` in [`tests/`](tests/) containing input events and expected matches. CI blocks merges if a new rule has no test.

Every rule sets `detection_source: "rule"`. Rules that exist only to confirm an ML signal set `detection_source: "hybrid"` and document the ML alert id they pair with.

Populated by SPEC-XXX-detection-rules.
