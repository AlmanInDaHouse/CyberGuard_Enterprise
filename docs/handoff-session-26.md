# Handoff — End of Session 26

Canonical state-of-the-world at the close of Session 26. Written so a **cold or
compacted** session recovers the thread in one read — nothing here assumes the
S26 conversation.

> ⚠️ **Session-25 gap — the chain broke and this handoff repairs it.**
> There is **no `docs/handoff-session-25.md`**. Session 25 (undocumented)
> committed the **ADR-0012 Amendment 2026-06-07** (`d95f8ea`, in-process TS
> scheduler) and the **Node-20 / setup-node CI migration** (`67c4a7c`,
> `64ca6f7`), and **staged but never committed** the detection-prod-driver and
> ingest-packaging WIP. The unbroken S9→S24 chain snapped at 25; that gap forced
> S26 to re-anchor from the repo and is the root cause of roughly half of what
> S26 fixed (unratified WIP at risk, documentary incoherence left on `main`).
> **The chain does not break again.**

Session 26 was a **safety + coherence recovery session**, not feature work: it
backed up unratified WIP, codified three process rules, ran a READ-ONLY triage of
the detection-prod-driver documentation, and applied the Class A coherence fix on
`main`. All landings green; Known CI debt stays **zero**.

## Anchor commits (all on `main`)

| SHA | What |
|---|---|
| `6a98549` | `docs(claude)`: process rules 3a (push-batching) + 3b (prod-driver merge gate). **Cherry-picked from `e761610`** on rama-b (branch correction — the rules were first committed on the branch, then moved to `main`). |
| `fa74b28` | commit (i): Class H-inline rule + Class B merge checklist added to rule 3b. |
| `8c3cbaa` | commit (ii): Class A prod-driver mechanism corrections across ADR-0017 / SPEC-014 / adr-README / specs-README + ADR-0012 catalog-row amendment summary. |

WIP backups (NOT on `main` — feature branches, not ratified):

| SHA | Branch | What |
|---|---|---|
| `30ac942` | `feat/detection-prod-driver` | Production detection driver (in-process TS scheduler) — 8 files, code-complete. 57/57 non-elevated tests **reported green by the S25 export — INHERITED, NOT re-verified in S26** (Gate 0 was read-only); re-run before ratifying. **NOT ratified**; pending the elevated `detect_ac_001` marquee. |
| `03005f0` | `feat/ingest-container-packaging` | Ingest container packaging — 4 files. **NOT ready to build** (Dockerfile:31 COPY rules fix identified, not applied). Parked. |

`feat/detection-prod-driver` carries **two** commits above the base:
`64ca6f7 → 30ac942 → e761610`. `e761610` (the CLAUDE.md rules) is
**content-identical to `main` via the `6a98549` cherry-pick**, so it adds no diff
and vanishes in the squash — **not pending work** (see §Pending gates).

- **Branch:** `main` at `8c3cbaa`.
- **Date:** 2026-09-14.
- **CI verdict:** every S26 push verified first-hand by full 40-char `head_sha`.
  `6a98549` / `fa74b28` / `8c3cbaa` → `markdown-lint` **success** (terminal,
  `total_count=1`, covered). The two feature-branch pushes (`30ac942`,
  `03005f0`) triggered **no** workflows (push to non-`main`, no PR) — reported
  **SIN COBERTURA**, not green (they were never merged).
- **Known CI debt:** ZERO rows.
- **Working tree:** clean at close (this handoff is the only uncommitted file).
- **Catalogs:** ADR 17 (`0001`–`0017`); SPEC 14 (`SPEC-001`–`SPEC-014`). No new
  ADR/SPEC in S26 (docs-coherence only).

## Gate 0 — re-anchor

**Motive:** S26's context arrived via an **export from another account derived
from memory** (not a repo handoff), carrying **12 uncommitted files unverified
for ~3 months**. Re-anchoring against `main` was mandatory before trusting any of
it. (The absence of `docs/handoff-session-25.md` was a **finding** during Gate 0,
**not** the motive.)

**Result — two halves:**

- **Structural GREEN.** Nothing lost: SHAs and files intact, `origin/main ==
  64ca6f7`, CI verified green first-hand. The **ADR-0012 Amendment 2026-06-07
  (`d95f8ea`) is already on `main`** — so mechanism claims naming the Go firehose
  as `runDetectionCycle`'s prod caller are false on `main` today, independent of
  any branch. Both feature branches were local-only (unpushed) with
  staged-but-uncommitted WIP; `gh` absent (REST fallback, token never logged);
  workflows trigger on `push:[main]` / PR only.
- **Documentary DRIFT.** A grep surfaced **11 documentation sites** still
  describing the prod caller as the deferred Go firehose — incoherent with the
  amendment already on `main`.

**Diagnosis:** the export was **precise where it carried verbatim text, imprecise
where it had paraphrased** — which is where the drift and the label collisions
(the "Decision A" / "Class A" clash below) originated.

## Decision — WIP backup (commit + push both WIP branches before the marquee)

The load-bearing decision of S26: **commit and push both WIP feature branches to
the remote *before* the elevated marquee, instead of waiting to ratify.** The
detection-driver + packaging work — **12 files** — had lived only in a single
disk's git **index** (staged, uncommitted, no remote copy) for **~3 months**; the
only thing that saved them was that nobody ran an errant `checkout`. `30ac942`
(detection driver) and `03005f0` (packaging) are the result of this decision.

**Rationale:** a commit **marked not-ratified on an unmerged branch is
INVENTORY, not ratification** — so it does not violate "only ratified work is
committed to `main`". Cost ~2 min, risk zero, and it removed a 3-month
single-point-of-failure. The WIP messages carry the "NOT YET RATIFIED" marker and
are squashed on merge (the marker must not survive in `main` history).

> Naming: this was called "Decision A" earlier in the session; **renamed to the
> WIP backup decision** because "A" collided with "Class A". **"Class A" is
> reserved for the documentary mechanism-claim track.**

## Process rules landed (S26)

- **Rule 3a — one commit per push when each needs independent CI.** Actions runs
  only the pushed head SHA, never intermediate commits; if batched, the report
  must state only the head was covered. (`6a98549`.)
- **Rule 3b — Detection prod-driver branch merge gate.**
  `feat/detection-prod-driver` merges to `main` only when **both**: (1) the
  elevated `detect_ac_001` marquee is GREEN, and (2) the Class B
  documentary-coherence edits are attached in their own commit, after the marquee
  and before the merge. Class A is a separate, non-marquee-gated track.
  (`6a98549`; extended in `fa74b28`.)
- **Rule 3b checklist (fa74b28)** — when applying the Class B edits at merge:
  review **embedded citations**, not just the assertion (known case
  `ADR-0017:64` cites ADR-0012 §1, stale post-amendment); classify **per-clause,
  not per-site** (known case `ADR-0017:88` carries a Class B "Inherited gap"
  clause inside a Class A bullet); and **redo the whole sentence** where a B
  assertion and an A clause **interleave in one parenthesis** (the only such
  site, `docs/specs/README.md:50`).
- **Class H-inline rule (fa74b28)** — historical records inside living documents
  are never edited.

## The Class A fix — scope (commit `8c3cbaa`)

Corrected **5 pure Class A** sites + **1 catalog line**, and **split-fixed 5
MIXTO** sites (A clause corrected, B clause left verbatim). Total edited: **5 A +
5 MIXTO + 1 catalog line = 11 sites** — the eleven the S26 triage ratified. (An
earlier count of "6 pure A" predated `ADR-0017:96` moving to Class H-inline.)

- **ADR-0017:** `:88`, `:104` (Class A); `:12`, `:64` (MIXTO split). `:88` was
  the twin of the "§Out of scope — production detection driver" bullet; its
  Class B `"Inherited gap; notification rides whatever drives the cycle."` clause
  was preserved (initially over-deleted, restored after ratification).
- **SPEC-014:** `:6`, `:38`, `:133` (Class A); `:96` (MIXTO split). `:38` is the
  twin of `ADR-0017:88`; its `"Inherited gap; notify rides whatever drives the
  cycle."` Class B clause was likewise restored.
- **docs/adr/README.md:** `:31` (ADR-0012 catalog-row amendment summary) + `:89`
  (MIXTO split, B `"test-validated altitude until then"` preserved).
- **docs/specs/README.md:** `:50` (MIXTO split; interleaved B/A parenthesis —
  the one site needing whole-sentence rework at merge).

Pure Class B sites NOT touched (wait for the rama-b merge, rule 3b): `SPEC-014:17`.
Class H-inline sites NOT touched, ever: `ADR-0017:96` (landing checklist),
`ADR-0017:52` (§A3 alternative rationale), `SPEC-014:121` (ratification record).

## Debts / deferred — the running list (11)

10 inherited from [handoff-session-24.md](handoff-session-24.md) (the four
S23-carried items unbundled into line-anchored entries); **#11 is new**, moved
from architect auto-memory into the repo (the repo is the sole record).

1. **Node-20 / setup-node CI migration — RESOLVED in S25** (`67c4a7c` migrate off
   Node 20; `64ca6f7` disable setup-node@v5 pnpm auto-cache). Was handoff-24's
   URGENT #1 (deadline 2026-06-16). Kept for provenance; no longer live.
2. **markdownlint local-vs-CI version skew.** Local `markdownlint-cli2 v0.22.1`
   vs CI `markdownlint-cli2-action@v23`. No skew bit S26 (5 green pushes), but
   versions still differ. Destination: pin local to the CI action's version.
3. **Ingest harness fragility — hardcoded agent UUIDs collide across tests**
   (`notify-ac-001-on-create.test.ts:23` ↔
   `incident-ac-005-fk-production-faithful.test.ts:24`). Destination: an agent-id
   allocator or per-test schema for the ingest harness.
4. **Marquee `detect_ac_001` "0 events" environment trap.** Symptom-identical
   across stale-binary / not-elevated / dirty-watermark. Destination: assert
   event-count > 0 (or fail-fast on non-elevated ETW open). Never affects CI.
5. **Go→TS BROAD documentation sweep.** `blueprint.md:393` ("Go + headless
   Chromium") and `services/README.md:8-12` still name a Go constellation the project
   does not build. Destination: blueprint + `services/README.md` reconciliation.
6. **DDL-mirror of `cges_events` in the api test harness** —
   `events-schema.ts:34` ↔ `migrate.ts:134`. The api harness re-declares the
   ClickHouse DDL instead of sharing ingest's. Destination: one shared DDL source.
7. **`canonicalize` not extracted to a shared package** — `canonical.ts:11` ↔
   `jcs.ts:7`. Duplicated JCS canonicalization. Destination: a shared package.
8. **Throwaway-DB migration-backfill pattern not extracted** to a shared helper —
   `incident-severity-ac-004:57`. Destination: extract the helper.
9. **Double incident-resolution in `buildReport`** — `report.ts:137` / `:141`;
   on-demand-scale-irrelevant. Destination: dedupe the resolution.
10. **SPEC-010 404 → 503 refinement** (`services/api/src/read/routes.ts:70`). A
    transient ClickHouse failure returns `404`, conflating evidence-inaccessible
    with evidence-absent
    ([SPEC-010 §Open questions](specs/SPEC-010-forensic-event-drill.md) item 3).
11. **ADR amendment placement — catalog-row vs §Dependencies precedent
    divergence (NEW, 2026-09-14).** ADR-0009 (`docs/adr/README.md:57`) and
    ADR-0013 (`:76`) record their amendments only in §Dependencies, not the
    catalog row. S26 consciously placed the ADR-0012 Amendment 2026-06-07 summary
    in the **catalog row** (`:31`) — CLAUDE.md §"SPEC amendment workflow" only
    requires the summary be "in the catalog", so both comply; cosmetic. Retrofit
    deferred.

> **Numbering note:** this 11-entry numbering is **reconstructed in S26** —
> handoff-24 did not number its debts and bundled the four S23-carried items
> (now #6–#9, each with the line anchors Gate 0 verified). #1 (Node-20) is
> **resolved in S25** (`67c4a7c` / `64ca6f7`), kept for provenance; **#11 is
> new** (moved from architect auto-memory into this repo record).

## Pending gates

- **The `feat/detection-prod-driver` merge gate is OPEN.** The WIP (`30ac942`)
  is code-complete but the **elevated `detect_ac_001` marquee has NOT been run**
  this session. Per rule 3b it merges only when the marquee is GREEN **and** the
  Class B edits are attached. Precondition (Convention #10 / SPEC-006
  dev-local): `cargo build --release -p cg-agent`, confirm the `.exe` is newer
  than every `agent/` edit, run from an **elevated** terminal. On merge, **both
  rama-b commits** (`30ac942` **and** `e761610`) are **squashed**: the `30ac942`
  "NOT YET RATIFIED" text must not survive in `main` history, and `e761610` is
  content-identical to `main` (via the `6a98549` cherry-pick) so it contributes
  **no diff** and vanishes in the squash — **not pending work**.
- **The detection prod-driver still has no *running* production caller on `main`.**
  The scheduler code lives on the unmerged branch; the pipeline (detection →
  incident → notify) remains validated-but-not-running until rama-b merges.

## MVP scorecard (Blueprint §18) — after S26

| # | Criterion | State |
|---|---|---|
| 3 | OTP login + RBAC 3 roles | **Delivered** (SPEC-008). |
| 4 | Gmail/SMTP notification | **Delivered** — test-validated altitude. |
| 5 | Incident PDF export | **Delivered** (SPEC-013). |
| 1 | 10 detection rules | **Partial 1/10** (SPEC-006). |
| 2 | Windows agent: processes / network / logins | **Partial 1/3** — processes only (SPEC-005). |
| 6 | 1 SOAR playbook | **Pending** — unblocked by the prod-driver seam. |
| 7 | Installation docs < 30 min | **Pending** — owner-STOP deployment contract. |

Criteria **1 / 2 / 4** remain at **test-validated altitude** — the pipeline
(detection → incident → notify) runs only under the harness until rama-b (the
prod-driver) merges.

## Open questions (carried, live)

- **Out-of-band trust anchoring of `forensic_pubkey`** (owner STOP, deployment
  contract) — unchanged, the highest-weight live decision.
- **SPEC-014 SMTP/recipient values** — var names fixed, values operator-set
  (owner STOP).

## Invariants / how Session 27 resumes

1. Read this handoff + prior handoffs (24 back to 9) + [CLAUDE.md](../CLAUDE.md).
   **There is no `-25`; do not look for it.**
2. Confirm `main` at `8c3cbaa`, tree clean, Known CI debt zero, catalogs 17/14.
3. The **repo is the source of truth**; nothing is ratified from memory —
   re-verify every anchor against `main`.
4. **Relay transport protocol.** The chat↔CC channel corrupts long lines (`.md`
   paragraphs of 800+ chars with no wrap): it drops segments mid-line and glues
   words at the seam. It also hits the `-` and context lines CC reads from disk —
   **the on-disk work was never affected.** What worked: **word-diff** instead of
   unified diff (collapses unchanged prose), **one file per message**, and a
   **numbered `[n/m]` short-line partition** when a hunk still will not pass. A
   **duplicate send** gives redundancy — except when both copies break at the
   same point, where the split (not the duplicate) is the fix.
5. Standing gate for any `detect/` change or the rama-b merge: the **elevated
   `detect_ac_001` marquee green** (rebuild binary + elevated terminal).
6. Session method: architect-advisor (scope + gate review) ↔ Claude Code
   (executor, READ-ONLY except approved diffs) ↔ Manuel (relay + diff gate +
   local/elevated tests).
