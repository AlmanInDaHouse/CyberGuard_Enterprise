# Engineering notes

Session-level engineering lessons. Not ADRs (no architectural decision), not SPECs (no feature). Working memory: patterns that accelerate similar decisions later. If a pattern recurs, promote it to an ADR.

## Session 5 (2026-05-20)

- **Test API exposure trade-off.** If a test requires exposing something dubious to the public API (e.g. internal logger primitives for AC-009 testability), restructure the test rather than the API. Bloating public surface for test convenience accumulates faster than expected.
- **chrono dependency trade-off.** `chrono` is heavy but the alternative (manual ISO 8601 with days-from-epoch) was 30+ lines and edge-case-prone. For RFC 3339 timestamps in the agent, `chrono` is the right call.
