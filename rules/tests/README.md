# Rule tests

Per-rule `.test.json` fixtures. Each file mirrors the name and path of the rule it validates and contains:

- `input`: an array of CGES events.
- `expected_matches`: the rule ids and event ids expected to match.
- `expected_no_match`: events that must not match (negative cases).

CI blocks merges if a new rule has no test.

Populated alongside [`rules/`](../).
