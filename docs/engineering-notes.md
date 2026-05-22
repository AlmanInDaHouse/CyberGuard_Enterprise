# Engineering notes

Session-level engineering lessons. Not ADRs (no architectural decision), not SPECs (no feature). Working memory: patterns that accelerate similar decisions later. If a pattern recurs, promote it to an ADR.

## Session 5 (2026-05-20)

- **Test API exposure trade-off.** If a test requires exposing something dubious to the public API (e.g. internal logger primitives for AC-009 testability), restructure the test rather than the API. Bloating public surface for test convenience accumulates faster than expected.
- **chrono dependency trade-off.** `chrono` is heavy but the alternative (manual ISO 8601 with days-from-epoch) was 30+ lines and edge-case-prone. For RFC 3339 timestamps in the agent, `chrono` is the right call.

## Session 10 (2026-05-23)

- **Follow-ups are co-located with the document that generates them.** A spike's follow-ups live at the bottom of the spike note; an ADR's follow-ups live at the bottom of the ADR; an amendment's follow-ups live with the amendment. No central `docs/follow-ups.md` index is created proactively — that would be premature abstraction. If dispersion later makes auditing hard, an index gets created **then**, populated from the existing in-place sections. The rationale is that a follow-up is most useful when read next to the evidence that justified it; separating them loses signal.
- **ADR Deciders field — three-role convention formalised.** Every ADR header uses `- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)` verbatim. ADRs 0001–0008 already use this form by consistent copy; this note promotes it from accidental pattern to explicit precedent so future ADRs adopt it by reference, not by copy.
