# Rule tests

One `<name>.test.json` fixture per rule in [`../windows/`](../windows/) (SPEC-016 §Data contracts §3), evaluated by `rules_ac_002` (`services/ingest/test/rules-ac-002-fixtures.test.ts`):

```json
{
  "rule": "rule.<name>",
  "description": "what the cases pin",
  "cases": [
    {
      "name": "short case name",
      "event": { "Image": "...", "ParentImage": "... or null" },
      "expected_match": true
    }
  ]
}
```

Each case is evaluated as the Launch (`activity_id = 1`) of a process with those two fields; `ParentImage: null` is a parent that was not resolved. Each fixture has:

- `rule` equal to the `id` of `../windows/<name>.yml`;
- at least two positive cases, at least one of them with every path in the device form (`\Device\HarddiskVolume3\…`);
- at least two negative cases, including a near miss: the right name in the wrong place, or the right child under the wrong parent;
- for a rule whose `condition` reads `ParentImage`, a negative case with `ParentImage: null`.

`rules_ac_002` fails when a rule has no fixture, a fixture has no rule, a fixture misses a minimum, or a case does not evaluate as expected. The near miss is checked in review, not by the test.
