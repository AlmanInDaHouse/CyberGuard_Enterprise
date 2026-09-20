# Handoff — Session 27 (interim: debt inventory)

Opened mid-Session 27 to move the live debt inventory out of architect
session-memory and into the repo — the repo is the sole record. This is the
same correction Session 26 applied to debt #11 (moved from auto-memory into
[handoff-session-26.md](handoff-session-26.md) §Debts); #12 had since regressed
to living only in session-memory across two turns, and #13 is new. The full
Session 27 close will extend this file; for now it carries only the running
debt list.

## Debts

Continues the numbering from [handoff-session-26.md](handoff-session-26.md)
§Debts (entries 1–11).

- **#12 — Compose basename collision.** A root-level `docker-compose.dev.yml`
  stub and the real `infra/dev/docker-compose.dev.yml` share a basename —
  ambiguous for basename-resolving tooling. Resolved at criterion 7 (the
  deployment / compose layout); touching ADR-0001 is an owner-STOP. Details and
  sequencing in [roadmap.md](product/roadmap.md) Part (b) §4 and Part (a) §F.
  Carried in architect session-memory across two turns before being recorded
  here.
- **#13 — Anchor drift caused by the prod-driver branch.** The `BATCH_LIMIT`
  JSDoc and the `notify?` parameter added on `feat/detection-prod-driver` moved
  `runDetectionCycle` from `services/ingest/src/detect/index.ts:24` to `:36`,
  and the `alertId !== null` block from `:38-55` to `~:54`. About 11 path:line
  citations are now stale — `ADR-0017:12`, `ADR-0017:105`, `SPEC-014:6`,
  `SPEC-014:17`, `SPEC-014:27`, `SPEC-014:96`, `SPEC-014:117`, `SPEC-014:129`,
  `SPEC-007:148`, and `roadmap.md:36`. **Class A**: corrected in its own
  Class-A pass that sweeps by pattern, never inside a Class B commit —
  correcting some along the way would leave the pattern sweep incomplete.
- **#14 — MVP-criteria altitude mis-grouping.** `SPEC-014:17` equated criteria
  1–3 as the test-validated group. The real group is 1/2/4 (`README.md:28`);
  criterion 3 (OTP + RBAC) is delivered and runs in the auth request-path — it
  was never at that altitude. Present since SPEC-014 was written; caught in the
  Session 27 Class B pass on a clause-by-clause re-read. The `SPEC-014:17`
  instance disappears when the comparison is removed in that pass. A repo sweep
  (`criteria 1–3` / `1, 2 and 3`) found the same grouping at `ADR-0012:275`
  (Amendment §Context), which labels *"criteria 1, 2 and 3"* as *"detection,
  incident, notification"* — a parallel mis-numbering (notification is criterion
  4; "incident" is not a numbered criterion); that site is **Class H-inline**
  (immutable amendment record), so it is recorded here, never corrected. No live
  (non-H-inline) instance remains once `SPEC-014:17` is fixed.
