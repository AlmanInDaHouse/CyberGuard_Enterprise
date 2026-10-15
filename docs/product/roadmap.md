# CyberGuard — technical roadmap

Orders the remaining MVP work by technical dependency, and lists the
owner-STOP decisions that gate it. This is the work-order document; the
current delivery state is the MVP scorecard in the
[README](../../README.md) and the latest session handoff
([handoff-session-39.md](../handoff-session-39.md)).

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

- **Status: DONE** — landed as `f006b9e` (condition parser) + `1f9aa4a`
  (evaluator) + `18fc2d3` (reject explicit `|exact`) (S30, 2026-09-23);
  elevated marquee green on the reviewed tip (37 files / 107 tests).
- Contract: SPEC-015 (Accepted 2026-09-23), amends SPEC-006 by scope.
- Does: relax the strict-reject validator
  (`services/ingest/src/detect/engine.ts:32-51` at `1fbb035`) to admit more Sigma
  fields and the `contains` operator; no read-model change (SPEC-015
  §Scope; widening belongs to B2, and for other classes to the detection
  SPEC that makes each load-bearing, SPEC-015 Amendment 2026-10-11). No new capture, no agent
  decision.
- Blocked by: A. Cheap.
- As planned, a prerequisite of C and D: network and login rules also need
  to leave the logsource pinned to `process_creation`
  (`services/ingest/src/detect/engine.ts:51` at `1fbb035`). This is the piece that
  must NOT travel inside the capture work. D, when it landed, delivered
  capture, delivery and storage, and no rule: network and login rules are
  deferred to the detection SPEC that makes each class load-bearing
  (ADR-0012 Amendment 2026-10-11).

### C — Criterion 1: the 10 rules

- **Status: DONE** — landed as `6f2ad9a` (loader contract) + `33ba7f3`
  (parent per child) + `c64172d` (marquee) + `72a40db` (rules) + `d68655a`
  (scenarios) + `ed391aa` (stale text) (S31, 2026-09-28); elevated marquee
  green on the reviewed tip (41 files / 152 tests).
- Contract: SPEC-016 (Accepted), amends SPEC-015 and SPEC-006 by scope.
- Blocked by: B1 (not B2).
- With B1 done (SPEC-015), the engine accepts exact / `endswith` /
  `startswith` / `contains` over `Image` and `ParentImage`, with named
  blocks and `and` / `or` / `not` conditions. That yields rules with real
  content — suspicious image paths, execution from Temp/AppData, parent
  lineage — WITHOUT touching the agent. Multi-hop lineage stays out of
  the MVP (SPEC-016); instead, the parent is resolved per child, with its
  own 24 h look-back.
- Only rules that inspect the COMMAND LINE or the USER stay out: B2
  carries the fields and B3 adds the rules over them; everything
  expressible over process image / path / lineage is in scope here.
- Quality bar: ten rules, each with a WIRED test. At `8a0ed9f`
  `rules/tests/` held a JSON fixture that no `.ts` loaded (and whose format
  had drifted from `rules/tests/README.md`); fixed here: `rules_ac_002`
  evaluates every fixture and `rules_ac_003` every scenario.
- Loader: `loadRules` (`services/ingest/src/detect/engine.ts`), hardened
  here (SPEC-016 §Data contracts §1).
- On a real agent the rules see events since G landed (§G): capture runs
  on the agent's normal run path.

### G — Agent capture on the normal run path

- **Status: DONE** — landed as `9250f68` … `24e0d04`, fourteen commits,
  one per push (S32, 2026-10-09); elevated gate green on the reviewed tip
  `4933dfb` (four real-ETW tests; 42 files / 154 tests). Details:
  `docs/handoff-session-32.md`.
- Contract: SPEC-017 (Accepted), amends SPEC-005, SPEC-003 and SPEC-001
  by scope, and ADR-0004 and ADR-0011 in place.
- Does: give the agent's normal run path (`run_secure`) the SPEC-005
  capture that only the test-mode path has today, with a session start
  that reports its failure (exit code 9 or 1), ADR-0009 §1 at-least-once
  delivery (a batch that fails transiently is resent with the same
  `event_id`s), a heartbeat on every interval without a POST, the
  going-offline handshake, and the SPEC-005 §Operational §3 device-path →
  Win32 translation. The test-mode path is removed.
- Why (observed at `8a0ed9f`): `agent/cg-agent/src/main.rs` reaches
  `run_test_mode`, the only path that opens the ETW session, only when
  `CG_AGENT_TEST_MODE=1`; `run_secure` sends heartbeats only
  (`agent/cg-agent/src/lib.rs`). The test-mode loop drops a drained batch
  on a transient send failure, sends nothing while no events are
  drained, and skips the going-offline handshake. No translation exists:
  the agent copies `ImageName` unchanged — the S31 marquee recorded
  `\Device\HarddiskVolume3\…` paths. Origin: the S15 fix moved mTLS
  into `run_test_mode` instead of ETW into `run_secure`
  (`docs/handoff-session-15.md:67`).
- Unblocks: process capture, and so detection, on a real deployment
  (today only with the test switch); D, whose network and login capture
  builds on this path.
- Blocked by: nothing technical. Sequencing: after C, before D — advisor
  decision under the owner's delegation, S31 (2026-09-27).
- Gate (own): the Rust ETW tests and both marquees, elevated, on the
  normal run path (SPEC-017 §Acceptance criteria).
- Owner decision, S32 (2026-10-04): an unelevated agent exits with code 9
  (SPEC-005 AC-002), instead of degrading to heartbeats only (Part (b)
  §7).

### H — Late events in detection

- **Status: DONE** — landed as `9ea6a07` and `f2d36ae`, one per push
  (S33, 2026-10-10); elevated gate green on the reviewed tip `b7bdbe2`
  (four real-ETW tests; 49 files / 163 tests). Details:
  `docs/handoff-session-33.md`.
- Contract: SPEC-018 (Accepted), amends SPEC-006 by scope, and ADR-0012
  in place.
- Does: make the detection read-model advance by arrival instead of by
  event time, so an event that reaches the server after the watermark has
  passed its `time` is still evaluated. The cursor is the pair
  `(arrived_at, event_id)`, read behind a 5 s settle margin and without
  `FINAL`; an index on `arrived_at` keeps the read from scanning the
  whole table.
- Why (observed at `e19f782`): `readNewEvents`
  (`services/ingest/src/detect/read-model.ts`) selects `time > watermark`
  per org (ADR-0012 §7). With two agents in one org, the events of the
  one that delivers later are never evaluated. `cges_events` already
  stores a server-assigned `arrived_at`. Read at `7a9160a`, it is wider:
  every agent is written under one org, so this is any two agents; and,
  deduced from the code, an agent whose clock runs ahead hides the
  others' events, and a single agent loses its own when its clock steps
  back.
- Unblocks: correct detection with more than one agent per org; D, whose
  classes share `cges_events` and its arrival order (SPEC-019 stores
  them; the read-model keeps reading class 1007).
- Blocked by: nothing technical. Sequencing: after G, before D — advisor
  decision under the owner's delegation, S32 (2026-10-04). With one agent
  and G's in-order retries, G does not make it worse.
- Gate (own): the `late_ac_*` tests in CI (SPEC-018 §Acceptance
  criteria), and the elevated gate with both marquees.
- Owner decisions, S33 (2026-10-09), delegated to the advisor: the
  vehicle; no lateness horizon; the cursor starts at the beginning after
  the upgrade (SPEC-018 §Ratification record).

### D — Criterion 2: new classes (4001 network + 3002 login)

- **Status: DONE** (S35). Both halves landed one commit per push, and
  the elevated gate was green on the tree each one landed.
  - **Network:** SPEC-019 (Accepted, amended 2026-10-10), landed as
    `8fd71eb` to `8fac5c3` on 2026-10-10. The gate on `599424d` ran six
    real-ETW tests, event 31 included; 54 files / 175 tests.
  - **Login:** ADR-0019 and SPEC-020 (Accepted, both amended 2026-10-11),
    landed as `89db206` to `45e7fb7` on 2026-10-11. The gate on `0d03697`
    ran seven elevated tests; 60 files / 197 tests.

  Details: `docs/handoff-session-35.md`.
- Does: agent capture, delivery and storage, end to end, for CGES 4001
  (network) and 3002 (login), with a marquee for each. No detection rule
  and no product surface in this phase.
- Split in two, network first:
  - Network — TCP connections opened, outbound and inbound, over IPv4
    and IPv6, from ETW Kernel-Network in the agent's existing session.
    Contract: ADR-0018 and SPEC-019 (Accepted). SPEC-019 also carries
    what both classes share: the agent's event types, the widening of
    `class_uid: z.literal(1007)`
    (`services/ingest/src/schemas.ts:44`) to a union, and the widening
    of `cges_events`.
  - Login — logons, successful and failed. Contract: ADR-0019 and
    SPEC-020 (Accepted, S35); landed. The source is the Windows Security
    log, read through the Event Log API: a second capture source that is
    not ETW.
- Blocked by: nothing; G, H and B1 are done.
- Gate (own): the `net_ac_*` tests (SPEC-019 §Acceptance criteria) and
  the `auth_ac_*` tests (SPEC-020 §Acceptance criteria), in CI and in the
  elevated gate.
- Discharges, network half:
  `docs/adr/0011-cges-process-activity-v0-1.md:197` and
  `docs/specs/SPEC-005-agent-process-telemetry-windows-etw.md:15` (the
  network provider's deferral);
  `docs/adr/0012-normalize-before-correlate-pipeline.md:240` (4001 stops
  being schema-only; its rules stay deferred).
- Discharges, login half:
  `docs/adr/0012-normalize-before-correlate-pipeline.md:16` (3002 is
  schema-only). No document defers an authentication provider.
- Schemas ready: `schemas/cges/v0.1/classes/4001_network_activity.json`,
  `schemas/cges/v0.1/classes/3002_authentication.json`, both in
  `schemas/cges/v0.1/event.json:105,106`.
- After D, not a phase yet: DNS, the first network follow-up. It needs
  another provider and a CGES class that does not exist (ADR-0018
  §Alternatives considered A3).
- Owner decisions, S34 (2026-10-10), delegated to the advisor: the scope
  of the phase; what "basic network" covers; the vehicle (SPEC-019
  §Ratification record).
- Owner decisions, S35 (2026-10-10): which logon data the agent collects
  (ADR-0019 §3–§5); the four points the draft left open, delegated to
  Claude Code (ADR-0019 §Decision record); ADR-0019 ratified. On
  2026-10-11: the gate's successful logon is an S4U logon of the current
  user (ADR-0019 Amendment 2026-10-11).

### B2 — CommandLine + subject_user_sid capture

- **Status: PARKED** (S38, 2026-10-14). Source measured and decided by
  Manuel (2026-10-11) and recorded in ADR-0020, Accepted on 2026-10-13
  with its six co-located amendments: the kernel's process events, in a
  second session of the agent's, joined to Kernel-Process's Launch by PID
  and timestamp; secrets replaced on the agent; the user on the wire as
  `process.user.uid`; Windows 10 and Windows Server 2019 covered
  (`docs/handoff-session-37.md`). Its SPEC, SPEC-021, cannot be written by
  either party. The platform's safety classifier stopped Claude Code's
  drafting twice in S37, with the instruction not to produce that content
  again (handoff-37 §SPEC-021). Manuel no longer holds what it was to say
  ("no me acuerdo de que va ahi asique es imposible volver a hacerlo",
  2026-10-14). B2 is not one of the seven MVP criteria. Manuel delegated
  the pending decisions on 2026-10-14, and Claude Code parked B2 and §B3.
  **Reopen when** a SPEC-021 text reaches the repository from outside
  Claude Code, for instance another model given ADR-0020 and handoff-37
  §SPEC-021. Claude Code then lands it verbatim, reviews it against the
  repository and implements it.
- Does: give the agent a real capture source for `command_line` and
  `subject_user_sid` (on the wire `process.command_line` and
  `process.user.uid`, ADR-0020), then carry them end-to-end (agent → wire
  schema → `services/ingest/src/detect/read-model.ts:114-115`). An agent decision with
  its OWN ADR.
- No rule reads the two fields in this phase: the rules over the command
  line and the user are §B3 (owner decision, S36, 2026-10-12).
- Also carries the parent at the edge (SPEC-016 §Scope): the agent stamps
  the parent's image from its own process table (ADR-0011 §5 already lets
  it emit `parent_process.name`; the CGES schema models `parent_process`),
  seeded by an initial process enumeration at startup (the destination
  SPEC-006 §Operational §2 names). That removes both the pre-agent false
  negative and the need for the server-side look-back.
- Blocked by: B1, G and D.
- Why a separate phase (S27 verification): the wired Kernel-Process
  provider (keyword 0x10, `agent/cg-agent/src/etw/session.rs:114`) does
  NOT expose CommandLine or a usable UserSID in ProcessStart. The agent
  already TRIES both (`session.rs:217-218`, each `unwrap_or_default()`)
  but the manifest carries neither property, so both resolve to EMPTY —
  absent, not present-but-empty (documented:
  `rules/windows/office_spawns_script_host.yml:8-9`;
  `read-model.ts:114-115`; SPEC-006 §Operational §4).
- Capture options, as listed before ADR-0020 decided between them: (1), in
  a second session joined to Kernel-Process, because the kernel's events
  carry no path (ADR-0020 §Context 5); (2) and (3) are its A2 and A3:
  - (1) NT Kernel Logger process events (Process_TypeGroup1) — carry
    CommandLine + ImageFileName; kernel-trace session.
  - (2) Security-Auditing 4688 + "include command line in process
    creation" audit policy — see Part (b) §6 (this option depends on the
    phase-F deployment contract).
  - (3) Out-of-band PEB read (NtQueryInformationProcess) — racy for
    short-lived processes; privilege concern.
- Sequencing: after D — owner decision, S27 (2026-09-19).

### B3 — Rules over the command line and the user

- Does: the detection rules that read a process's command line or its
  user, once B2 carries both fields to the detection read-model. The
  evaluator admits the command line and the user, which SPEC-015
  refuses today, and each rule lands with a wired test, as C's ten did.
  B2 carries the user as a SID, not the account name Sigma's `User`
  holds, so B3's SPEC states what a user selection matches.
- Unblocks: detection of what only the command line or the user shows.
  The blueprint's suspicious-PowerShell case was re-planted on process
  lineage because the command line was empty (SPEC-006 §Operational §4).
- Owner-STOP before its SPEC is accepted: which rules, that is which
  threats they cover (Part (b) §9).
- **Status: PARKED with B2** (S38, 2026-10-14).
- Blocked by: B2.
- Sequencing: right after B2, as a phase of its own — owner decision,
  S36 (2026-10-12). B2 delivers the fields and no rule, as D delivered
  its classes and no rule.
- Gate (own): the rules' tests in CI, and the elevated gate with
  `detect_ac_001`, since the rule set it evaluates changes.

### E — SOAR (criterion 6)

- **Status: DONE** — landed as `0dbe0a9..edbf8e9` (S38, 2026-10-14):
  ADR-0021 (ratified by Manuel, `71fbcca`), SPEC-022 (`0246936`) and its
  two amendments (`9f362e1`, `df69aa7`), the implementation after three
  fresh-context code reviews, and elevated gate 4 green at `4c4e5b0`
  (`docs/handoff-session-38.md`). Criterion 6 is delivered; F is the MVP
  criterion left.
- Does: one playbook executor on the incident-create seam. The seam
  exists: a create-only branch fires in `runDetectionCycle`
  (`services/ingest/src/detect/index.ts:77`) beside
  `notifyIncidentCreated`. SOAR attaches there as a second optional
  carrier, same fire-and-forget contract.
- Unblocks: criterion 6.
- Blocked by: A (incidents must actually be created in prod).
- Needed: ADR-0021 (Accepted) and its SPEC (SPEC-022); `services/soar/`
  and `playbooks/` were README stubs. SOAR now runs inside
  `services/ingest` (`src/soar/`), and `playbooks/` holds the shipped
  playbook.
- Precedent to copy: the email fire-and-forget
  (`docs/specs/SPEC-014-incident-notification.md`,
  `docs/adr/0017-incident-email-notification.md`; call site
  `services/ingest/src/detect/index.ts:77-84`).

### F — Installation / deployment (criterion 7)

- Does: a production compose, operator runbook, config/secrets surface,
  TLS/CA provisioning, published images, and dashboard in compose. Today
  only the dev stack exists (`infra/dev/docker-compose.dev.yml`, "not for
  production"); root `docker-compose.yml` is `services: {}`; `deploy/` is
  READMEs.
- Unblocks: criterion 7 (the "<30-min self-deploy" promise).
- Blocked by: the deployment-contract decisions of Part (b): compose
  location (#12) and forensic trust anchoring. Manuel delegated them on
  2026-10-14, so they are decided in F's SPEC; the ADR amendments they need
  wait for his explicit "sí". (B2 option (2)'s audit-policy dependency no
  longer waits here: ADR-0020 ruled the option out.)
- Needs: an infra/deploy SPEC (`SPEC-XXX-infra-docker`).
- Carries, from S39 (`docs/handoff-session-39.md` §Debts and the threat
  model's Amendment 2026-10-15): the dashboard's TOTP enrollment and user
  management (#83, with #98; its login and logout landed in S40,
  `1697e60`) and the transport between the dashboard and the api (#97);
  the agents' certificate renewal (#88; the server certificate's reissue
  landed in S40, `b293500`); a Redis user and a Postgres role per
  service (#85, #86); TLS on the enrollment listener (#89); non-root
  containers, and a TLS key file written before `b293500` (#92); Postgres logging without
  parameters (#94); per-service grants and retention for the logon and
  connection telemetry (#95, #47); the bootstrap CLI's password off the
  command line (#77); trusting a relay's private CA (#78); the proxy's
  forwarded address (the `API_TRUSTED_PROXIES` list for its compose,
  SPEC-008's second Amendment 2026-10-15, built in S40 as `c9dad1c`, which
  closed #79; the listed proxies between the dashboard and the api append to
  `X-Forwarded-For` or pass it on unchanged, item 4). From S40
  (`docs/handoff-session-40.md`): the dashboard served only behind a TLS
  proxy (SPEC-009's second Amendment 2026-10-15, item 5) that appends to or
  sets `X-Forwarded-For` (#103), and that either passes the browser's `Host`
  as sent, dropping any `X-Forwarded-Host`, or sets `X-Forwarded-Host` to
  that `Host`; the dashboard's security headers (#104, without
  `Referrer-Policy: no-referrer`); `__Host-` names for the dashboard's
  cookies (#106); a ClickHouse user per service, the api's read-only
  (#108).
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
6. RESOLVED (S37, 2026-10-13) — B2 capture source vs the deployment
   contract: B2 lands between D and F with option (2) (Security-Auditing
   4688 + audit policy) ruled out, through option (1), the kernel's
   process events joined to Kernel-Process. Owner decision (S36,
   2026-10-11), recorded in ADR-0020, Accepted on 2026-10-13.
7. RESOLVED (S32, 2026-10-04) — an unelevated agent on the normal run
   path exits with code 9 (SPEC-017 §Operational §1). Owner decision.
8. RESOLVED (S36, 2026-10-12) — the rules over the command line and the
   user are a phase of their own right after B2 (§B3), not part of B2.
   Owner decision.
9. B3's rule set: which threats the rules over the command line and the
   user cover. Owner decision before B3's SPEC is accepted; Claude Code
   proposes the candidates in its draft. Parked with §B3 (S38,
   2026-10-14).

Manuel delegated §1–§4 to Claude Code on 2026-10-14 ("lo que este
pendiente de mi decidelo tu en base al beneficio del proyecto"). They are
decided for an enterprise buyer: a security lead and a data-protection
officer.

- §1 is an amendment to ADR-0002 that names the server side TypeScript,
  with the Go firehose kept behind its exit.
- §2–§4 are decided in phase F's SPEC.
- The ADR amendments they need (ADR-0002; ADR-0001 for §4; ADR-0016 for
  §3) still wait for Manuel's explicit "sí" before they are Accepted.

§5–§8 are resolved (above), and §9 is parked.
