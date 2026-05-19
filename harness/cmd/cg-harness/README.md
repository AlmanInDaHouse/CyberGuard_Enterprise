# cg-harness

Scenario runner for the CyberGuard harness.

Populated by SPEC-XXX-harness. Until then this folder is a placeholder.

Expected responsibilities:

- Discover scenarios under [`harness/scenarios/`](../../scenarios/).
- Replay `events.jsonl` against the pipeline (locally or against a test container).
- Diff observed alerts / incidents / MITRE mappings / report snapshots against expected files.
- Report pass / fail per scenario with structured output for CI.
