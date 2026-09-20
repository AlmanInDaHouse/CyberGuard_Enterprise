# Handoff — End of Session 27

Full state-of-the-world at the S27 close. Written so a cold or compacted session
recovers the thread in one read.

Session 27 produced TWO things, and the order matters.

First, the project's first direction artifacts. The session began because it was
unclear which phase the project was in — there was no work-order document. The
README had said "Bootstrap phase" since scaffolding; S27 replaced that with a
real MVP scorecard (`975af2b`) and added a dependency-ordered technical roadmap
(`2410d2c`), linked from README and CLAUDE.md (`6dd7860`). The Go event firehose
(`services/pipeline/`) is explicitly OUT of the MVP plan.

Second, roadmap Phase A — the first step of that plan: the production detection
driver merged to main (`63019bc`), moving criteria 1 / 2 / 4 from test-validated
altitude to a standing stack, with the Class B documentary coherence the merge
required.

No new ADR/SPEC (catalogs stay ADR 17 / SPEC 14). Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `975af2b` | README: "Bootstrap phase" → real status + MVP scorecard. |
| `2410d2c` | `docs/product/roadmap.md` — the technical work-order. |
| `6dd7860` | link the roadmap from README + CLAUDE.md. |
| `e3123eb` | handoff-27 opened (debt inventory). |
| `bdf8347` / `37404d6` / `9ae4d48` / `57a99cc` | four rule-3b entries. |
| `58264c3` | debt #14. |
| `44f7432` | handoff-27 step-7 deferred scope. |
| `63019bc` | `feat(ingest)`: prod detection driver + Class B (the squash). |
| `fa7c025` | `docs`: step 7 — post-merge scorecard / roadmap refresh. |
| (this commit) | handoff-27 completed + README pointer → -27. |

Branch `feat/detection-prod-driver` squashed into `63019bc`; the "NOT YET
RATIFIED" WIP marker did not survive. `e761610`'s CLAUDE.md conflicted with main
(pre-`fa74b28` rules) and was resolved toward main in `b065aa5`; it contributes
no diff to the squash.

## Phase A — delivered

- Elevated `detect_ac_001` marquee GREEN dev-local 2026-09-20 (detect-ac-001 ✓
  40198 ms, spec-005 ✓, 60/60 in an elevated terminal with real ETW capture:
  `EtwSession::open`, 8 ring drains, `dropped_total` 0).
- Squash `63019bc`: the in-process TS scheduler (ADR-0012 Amendment 2026-06-07)
  gives `runDetectionCycle` its first production caller; started in `startIngest`
  after the listeners bind, stopped in `close()`; off-switch
  `INGEST_DETECT_INTERVAL_MS=0`.
- Non-elevated ingest suite 57/57 on main (52 base + 5 driver).
- Criterion 4 to REAL green: scorecard "Delivered (SPEC-014)".
- Merge-gate condition 1 satisfied per the refined rule: the marquee over
  `e761610` stayed valid at tip `6f9d5e3` (only doc / comment added on top;
  non-elevated suite 57/57 identical).

## The Class B coherence pass

Grep-decided set: 12 live altitude clauses classified, 10 edited, 2 deferred
(1 region).

- 12 classified = 8 documentary + 2 code + 2 README (deferred).
- Edited (commit i, in `63019bc`): 8 documentary — ADR-0017 :12/:64/:88,
  SPEC-014 :17/:38/:96, `docs/adr/README.md:89`, `docs/specs/README.md:50`.
- Edited (commit ii, in `63019bc`): 2 JSDoc — `services.ts:27`,
  `transport.ts:20`.
- Deferred to step 7 (`fa7c025`): root `README.md` :21 + :28 — one scorecard
  region, a status surface, not documentary coherence; owned by the scorecard
  refresh, not a Class B pass. Resolving :28 without :21 would self-contradict
  main until step 7.
- Excluded permanently: H-inline (SPEC-014:121, ADR-0017:96/:52, all
  `handoff-*`), ADR-0012:275 (amendment §Context, H-inline), CLAUDE.md:275 (the
  rule, not the system).

An earlier "9 documentary" count was an arithmetic slip during the session; the
list was always 8. The squash commit `63019bc`'s message carries the same slip
(says "9", names 8 sites).

## New rule 3b entries (from this session)

Four, all in CLAUDE.md's merge-gate section:

- Citation-preservation exception (polarity): preserve a stale `path:line`
  verbatim EXCEPT when inverting the assertion makes the citation vouch for the
  opposite — then fix it in place. Case: ADR-0017:64 (`(ADR-0012 §1, :28)` →
  Amendment `:271`).
- Statement-form preservation: a `MUST` stays a `MUST`, only its object updates.
  Case: SPEC-014:96.
- Scope of rule (a): "test-validated altitude" is marked resolved in PROSE, but
  REMOVED in a scorecard status cell. Case: README.md:21.
- Merge-gate condition 1 covers the CODE, not a SHA: a green marquee holds over
  a later tip if only doc / comment is added on top (diff has no files under
  `services/`, `agent/`, `dashboard/` + identical non-elevated count). Case: S27,
  `e761610` valid at `6f9d5e3`.

## Step 7 — post-merge scorecard / roadmap refresh

`fa7c025`: criterion-4 row → "Delivered (SPEC-014)."; the 1/2/4 caveat →
"standing stack, driven by the in-process detection driver"; roadmap Phase A →
Status DONE, "Discharges" → "Discharged". Scorecard rows 1/2/6 unchanged
(coverage, not altitude). `index.ts:31` in Phase A left verbatim (debt #13). The
README handoff pointer moved 26 → 27 in the handoff-completion commit (this one),
closing the interim-pointer condition.

## Roadmap A–F + sequencing

- A — prod-driver in prod. DONE (this session).
- A' — land `feat/ingest-container-packaging` (@`03005f0`). UNBLOCKED by A; own
  gate = `docker build` + rules land at `/app/rules/windows` + fail-loud path.
  No Manuel dependency.
- B1 — evaluator generalization (`engine.ts:32-51`: admit `contains` + more
  Sigma fields). UNBLOCKED by A; cheap; prerequisite of C AND D. No Manuel
  dependency.
- C — criterion 1, the 10 rules. Blocked by B1.
- D — criterion 2, classes 4001 network + 3002 login. Blocked by B1. Needs
  successor SPEC(s) + per-class ADRs.
- B2 — CommandLine + `subject_user_sid` capture. Blocked by B1 AND D. Its own
  ADR. Sequenced AFTER D (owner decision, 2026-09-19).
- E — SOAR (criterion 6). Unblocked by A; needs a SOAR SPEC+ADR.
- F — install / deploy (criterion 7). Last; owner-STOP deployment contract.
  Discharges debt #12.

## Owner-STOP decisions pending (waiting on Manuel)

- ADR-0002 Go→TS reconciliation. Recommendation: amend — TS as the MVP's current
  server-side assignment, Go as a named future.
- Deployment contract (phase F): config surfaces the operator sets; prod compose
  location; secrets / passphrase provisioning.
- Forensic trust-anchoring: out-of-band anchoring of the forensic Ed25519 public
  key (`ADR-0016:112` / `SPEC-012:36`) — the highest-weight live decision.
- B2 capture source (roadmap Part (b) §6). Recommendation: option 1, NT Kernel
  Logger (keeps audit policy out of the deployment contract).
- Compose basename collision (#12) — resolved within phase F.

## Debts

- #1–#11: unchanged — see [handoff-session-26.md](handoff-session-26.md) §Debts
  (Class H, immutable, so the pointer cannot go stale). New this session: #12,
  #13, #14.
- **#12 — Compose basename collision.** A root-level `docker-compose.dev.yml`
  stub and the real `infra/dev/docker-compose.dev.yml` share a basename —
  ambiguous for basename-resolving tooling. Resolved at criterion 7 (the
  deployment / compose layout); touching ADR-0001 is an owner-STOP. Details and
  sequencing in [roadmap.md](product/roadmap.md) Part (b) §4 and Part (a) §F.
- **#13 — Anchor drift caused by the prod-driver branch.** The `BATCH_LIMIT`
  JSDoc and the `notify?` parameter moved `runDetectionCycle` from
  `services/ingest/src/detect/index.ts:24` to `:36`, and the `alertId !== null`
  block from `:38-55` to `~:54`. About 11 `path:line` citations are now stale —
  `ADR-0017:12`, `ADR-0017:105`, `SPEC-014:6`, `SPEC-014:17`, `SPEC-014:27`,
  `SPEC-014:96`, `SPEC-014:117`, `SPEC-014:129`, `SPEC-007:148`, and
  `roadmap.md:36`. **Class A**: corrected in its own Class-A pass that sweeps by
  pattern, never inside a Class B commit — correcting some along the way would
  leave the sweep incomplete.
- **#14 — MVP-criteria altitude mis-grouping.** `SPEC-014:17` equated criteria
  1–3 as the test-validated group. The real group is 1/2/4 (`README.md:28`);
  criterion 3 (OTP + RBAC) runs in the auth request-path and was never at that
  altitude. Present since SPEC-014 was written; caught in the S27 Class B pass on
  a clause-by-clause re-read. The `SPEC-014:17` instance was removed in that pass
  (`63019bc`). A sweep found the same grouping at `ADR-0012:275` (Amendment
  §Context) — Class H-inline, so it is recorded here, never corrected. No live
  (non-H-inline) instance remains.

Known CI debt: ZERO rows.

## How Session 28 resumes

1. Read this handoff + prior handoffs (26 back to 9) + CLAUDE.md.
2. Confirm the main tip, tree clean, Known CI debt zero, catalogs ADR 17 /
   SPEC 14.
3. Next work with NO Manuel dependency: A' (packaging) and B1 (evaluator) — both
   unblocked by A, both cheap. C, D, B2, E and F each carry an owner-STOP on the
   path — see the Owner-STOP section above before starting them.
4. Standing gate for any `detect/` change: the elevated `detect_ac_001` marquee
   green (rebuild binary + elevated terminal).
5. Debt #13 is a good low-risk first Class-A citation sweep.
