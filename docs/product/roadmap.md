# CyberGuard — technical roadmap

Orders the remaining MVP work by technical dependency, and lists the
owner-STOP decisions that gate it. This is the work-order document; the
current delivery state is the MVP scorecard in the
[README](../../README.md) and the latest session handoff
([handoff-session-29.md](../handoff-session-29.md)).

Not a product roadmap — see the Blueprint §15 note below.

## Scope — the Go firehose is OUT of this plan

The high-throughput event firehose (Go, `services/pipeline/`) is not in
this roadmap. Nothing in Part (a) ports code to Go.
Its exit condition is unchanged: it lands only when the event-firehose /
NATS ADR materialises with throughput evidence (10–100k eps) and brings
the Go toolchain (`docs/adr/0007-ingest-language-typescript-mvp.md:36,60`).
The 2026-06-07 amendment left that gate intact
(`docs/adr/0012-normalize-before-correlate-pipeline.md:288`).

## Blueprint §15 is misaligned — superseded here for planning

The Blueprint §15 phase roadmap (`docs/product/blueprint.md:638-689`) no
longer describes delivery order: Forensics (its Phase 5) shipped before
SOAR (its Phase 4) and before detection/agent completion.
The per-session Phase 3.x–Phase 6 tracker (engineering-notes) is retired:
post-S19 work is tracked by MVP criterion, not phase number.
This dependency-ordered document supersedes both for planning.

## Part (a) — technical dependency order

### A — Detection prod-driver in production

- **Status: DONE** — merged as `63019bc` (S27, 2026-09-20); the
  non-elevated suite is at 57 as predicted.
- Does: merge `feat/detection-prod-driver` — the in-process TS scheduler
  that gives `runDetectionCycle` its first production caller
  (`services/ingest/src/detect/index.ts:31`).
- Unblocks: criteria 1 / 2 / 4 stop being test-validated and run in a
  standing stack.
- Blocked by: elevated `detect_ac_001` marquee green on the branch tip
  (S27 gate), then the Class B coherence edits.
- Needs: ADR-0012 Amendment 2026-06-07 (Accepted;
  `docs/adr/0012-normalize-before-correlate-pipeline.md:275,288`).
- Discharged: the "no production caller" assertions
  (`docs/specs/SPEC-014-incident-notification.md:17,96`).
- Verification invariant: the non-elevated ingest suite is at 52/52
  today (main, S27). The "57/57" in `docs/handoff-session-26.md:34` was
  inherited from the export and unverified; 52 + the 5 tests in
  `services/ingest/test/detect-driver.test.ts` = 57, so merging rama-b
  (this phase) must take the suite to 57. Any other number is something
  to investigate.

### A' — Land feat/ingest-container-packaging

- **Status: DONE** — landed as `48e3bd6` (bind) + `a67315c` (packaging) (S28,
  2026-09-21); the WIP's build-context fix (D1) plus two reactive corrections
  (D2, D3; see handoff-28); Class B residue closed in `e01c1f5`.
- Does: land the packaging branch (`feat/ingest-container-packaging`
  @03005f0) — already-written, pushed WIP that was falling out of the
  record.
- Work: apply the `services/ingest/Dockerfile:31` fix
  (`COPY --from=build /repo/rules` → `COPY rules`); reconcile the
  `docker-compose.dev.yml` + `.env.example` comments (branch @03005f0)
  that already describe the corrected line.
- Gate (own): first-hand `docker build` + verify the rules land at
  `/app/rules/windows` inside the image + exercise the fail-loud path
  positive AND negative.
- Blocked by: A (part of main-with-driver — needs the env vars + the
  driver contract).

### A'' — api container packaging

- **Status: DONE** — landed as `2cd0231` (bind) + `569f92d` (packaging) +
  `faac8b3` (ts-ci guard) (S29, 2026-09-23); `API_BIND_HOST` owner-STOP ratified
  (SPEC-008 Amendment 2026-09-23).
- Does: the api image builds and runs, and ts-ci builds ingest and api
  (#15, #16), so `task dev:up` works again.
- Work: apply the A' pattern to `services/api` + a `pnpm run build` step in
  the ts-ci ingest and api jobs; restore `README.md:54` once `task dev:up` is
  green.
- Gate (own): first-hand `task dev:up` with everything healthy + api reachable
  from the host + the ts-ci build steps green.
- Owner-STOP on the path: `API_BIND_HOST` (a new env var — the CLAUDE.md
  deployment-contract rule; precedent `INGEST_BIND_HOST`) and where the api
  config surface is documented (no SPEC covers it today).
- Blocked by: nothing (A' landed).

### B1 — Evaluator generalization

- Contract: SPEC-015 (Accepted 2026-09-23), amends SPEC-006 by scope.
- Does: relax the strict-reject validator
  (`services/ingest/src/detect/engine.ts:32-51`) to admit more Sigma
  fields and the `contains` operator; no read-model change (SPEC-015
  §Scope; widening belongs to B2 / D). No new capture, no agent
  decision.
- Blocked by: A. Cheap.
- Prerequisite of C AND D: network and login rules also need to leave the
  logsource pinned to `process_creation`
  (`services/ingest/src/detect/engine.ts:51`). This is the piece that
  must NOT travel inside the capture work.

### C — Criterion 1: the 10 rules

- Blocked by: B1 (not B2).
- With B1 done, the engine accepts `contains` and more Sigma fields over
  the already-normalized process fields
  (`services/ingest/src/detect/read-model.ts:89`: parentImage,
  imageFileName, pid, uid). That yields rules with real content —
  suspicious image paths, execution from Temp/AppData, multi-hop lineage
  — WITHOUT touching the agent.
- Only rules that inspect the COMMAND LINE stay out until B2 lands;
  everything expressible over process image / path / lineage is in scope
  here.
- Quality bar: ten rules, each with a WIRED test. Today `rules/tests/`
  holds a JSON fixture that no `.ts` loads; the real coverage is inline
  in `services/ingest/test/engine.test.ts:13`. That is fixed here.
- Loader is ready (`services/ingest/src/detect/engine.ts:88`).

### D — Criterion 2: new classes (4001 network + 3002 login)

- Does: agent capture + end-to-end for CGES 4001 (network) AND 3002
  (login), fused — they share the per-class projection and the widening
  of `class_uid: z.literal(1007)` (`services/ingest/src/schemas.ts:44`)
  to a union, so splitting them duplicates the plumbing.
- Blocked by: B1 (the evaluator generalization is already done).
- Needs: successor SPEC(s) to SPEC-005 + per-class ADRs.
- Discharges: `docs/adr/0011-cges-process-activity-v0-1.md:197` +
  `docs/adr/0012-normalize-before-correlate-pipeline.md:240` (4001 / 3002
  become load-bearing);
  `docs/specs/SPEC-005-agent-process-telemetry-windows-etw.md:15`
  (network + auth provider deferrals).
- Schemas ready: `schemas/cges/v0.1/classes/4001_network_activity.json`,
  `schemas/cges/v0.1/classes/3002_authentication.json`, both in
  `schemas/cges/v0.1/event.json:105,106`.

### B2 — CommandLine + subject_user_sid capture

- Does: give the agent a real capture source for `command_line` and
  `subject_user_sid`, then carry them end-to-end (agent → wire schema →
  `services/ingest/src/detect/read-model.ts:60`). An agent decision with
  its OWN ADR.
- Blocked by: B1 and D.
- Why a separate phase (S27 verification): the wired Kernel-Process
  provider (keyword 0x10, `agent/cg-agent/src/etw/session.rs:114`) does
  NOT expose CommandLine or a usable UserSID in ProcessStart. The agent
  already TRIES both (`session.rs:217-218`, each `unwrap_or_default()`)
  but the manifest carries neither property, so both resolve to EMPTY —
  absent, not present-but-empty (documented:
  `rules/windows/office_spawns_script_host.yml:8-9`;
  `read-model.ts:59-60`; SPEC-006 §Operational §4).
- Capture options (own ADR; NOT resolved here):
  - (1) NT Kernel Logger process events (Process_TypeGroup1) — carry
    CommandLine + ImageFileName; kernel-trace session.
  - (2) Security-Auditing 4688 + "include command line in process
    creation" audit policy — see Part (b) §6 (this option depends on the
    phase-F deployment contract).
  - (3) Out-of-band PEB read (NtQueryInformationProcess) — racy for
    short-lived processes; privilege concern.
- Sequencing: after D — owner decision, S27 (2026-09-19).

### E — SOAR (criterion 6)

- Does: one playbook executor on the incident-create seam. The seam
  exists: a create-only branch fires in `runDetectionCycle`
  (`services/ingest/src/detect/index.ts:69`) beside
  `notifyIncidentCreated`. SOAR attaches there as a second optional
  carrier, same fire-and-forget contract.
- Unblocks: criterion 6.
- Blocked by: A (incidents must actually be created in prod).
- Needs: a SOAR SPEC + ADR — unassigned today (`SPEC-XXX-soar`);
  `services/soar/` + `playbooks/` are README stubs.
- Precedent to copy: the email fire-and-forget
  (`docs/specs/SPEC-014-incident-notification.md`,
  `docs/adr/0017-incident-email-notification.md`; call site
  `services/ingest/src/detect/index.ts:69-76`).

### F — Installation / deployment (criterion 7)

- Does: a production compose, operator runbook, config/secrets surface,
  TLS/CA provisioning, published images, and dashboard in compose. Today
  only the dev stack exists (`infra/dev/docker-compose.dev.yml`, "not for
  production"); root `docker-compose.yml` is `services: {}`; `deploy/` is
  READMEs.
- Unblocks: criterion 7 (the "<30-min self-deploy" promise).
- Blocked by: owner-STOP deployment-contract decisions (Part b): compose
  location (#12), forensic trust anchoring — and it is here that B2
  option (2)'s audit-policy dependency resolves.
- Needs: an infra/deploy SPEC (`SPEC-XXX-infra-docker`).
- Discharges: `docs/adr/0010-agent-privilege-model-mvp.md:120` (packaging
  SPEC); the `deploy/` placeholders; debt #12.
- Last by dependency.

## Out of this plan (post-MVP deferrals — not phases)

- Event firehose (Go, `services/pipeline/`) — see the Scope section
  above. Exit: the event-firehose / NATS ADR with throughput evidence
  (`docs/adr/0007-ingest-language-typescript-mvp.md:36,60`); gate left
  intact by `docs/adr/0012-normalize-before-correlate-pipeline.md:288`.
- Forensic at-rest persistence (MinIO). Exit: a later
  forensic-persistence increment
  (`docs/specs/SPEC-012-forensic-evidence-hashchain.md:32` /
  `docs/specs/SPEC-013-forensic-report-render.md:35`).
- Persistent disk-backed agent buffer. Exit: a dedicated buffer SPEC or a
  superseding ADR (`docs/adr/0009-event-delivery-and-buffer.md:135`).

## Part (b) — owner-STOP decisions

1. ADR-0002 Go→TS reconciliation. The server side is entirely
   TypeScript, but ADR-0002 + `services/README.md` + `blueprint.md:393`
   still name a Go/Python constellation. (Debt #5 in handoff-26 §Debts;
   ADR-0002/0007 partially amended in S24.)
2. Criterion-7 deployment contract. Config surfaces the client operator
   sets; prod compose location (root vs `infra/`); secrets/passphrase
   provisioning.
3. Forensic trust anchoring. Out-of-band anchoring of the forensic
   Ed25519 public key
   (`docs/adr/0016-forensic-evidence-hash-chain.md:112` /
   `docs/specs/SPEC-012-forensic-evidence-hashchain.md:36`) — the
   highest-weight live decision.
4. Compose basename collision (#12). Root `docker-compose.dev.yml` stub
   vs the real `infra/dev/` one; resolved at criterion 7; touching
   ADR-0001 is owner-STOP.
5. RESOLVED (S27, 2026-09-19) — relative order of B2 vs D: D first.
   Owner decision.
6. B2 capture source vs the deployment contract. With B2 sequenced after
   D, option (2) (Security-Auditing 4688 + audit policy) depends on the
   phase-F deployment contract, which resolves later. Either B2 lands
   between D and F with option (2) ruled out — leaving (1) NT Kernel
   Logger and (3) PEB read — or B2 moves after F with all three live.
   Architect's recommendation: the former, via option (1); keeping audit
   policy out of the deployment contract is worth something on its own
   for a "<30-min self-deploy" product.

§1–§4 and §6 remain owner-STOP; §5 is resolved (above). Part (a) phase F
resolves §2 + §3 + §4 (and B2 option (2), per §6).
