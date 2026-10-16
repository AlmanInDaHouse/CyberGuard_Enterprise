# examples-invalid

CGES events that `event.json` must refuse; CI fails if any validates (`schema-validation.yml`, `task validate-schemas`). Each file is a valid example with one rule broken: an empty `time` (debt #40); and, in the agent's classes (debt #39, SPEC-017 Amendment 2026-10-16), an ISO 8601 `time` in 1007, 4001 and 3002, a `time` with a leading 0, and one of 20 digits.
