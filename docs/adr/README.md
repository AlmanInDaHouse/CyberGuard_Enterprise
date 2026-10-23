# Architecture Decision Records

This directory holds Architecture Decision Records following the [MADR](https://adr.github.io/madr/) format.

## Naming

`NNNN-short-kebab-title.md`, where `NNNN` is a zero-padded sequential id starting at `0001`.

## Status values

- `Proposed` — under discussion.
- `Accepted` — current binding decision.
- `Deprecated` — superseded by a newer ADR; kept for historical context.
- `Superseded by ADR-NNNN` — explicit replacement pointer.

## Catalog

Each row names every amendment of its ADR: its date, the record that made it or came with it when there is one, and what it changes. *Dependencies* lists the edges from each ADR to the records it rests on or amends, with the notes that go with them; an "amends nothing" line there means the ADR amends no other record.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-monorepo-layout.md) | Monorepo layout for CyberGuard — amended 2026-10-19 (with ADR-0022): no compose file at the root, the production compose in `deploy/docker/`, `infra/` for the developer environment, one release tag `vX.Y.Z` for the product | Accepted |
| [0002](0002-language-per-component.md) | Language per component — amended 2026-05-22 (ADR-0007): the `services/ingest/` row is TypeScript for the MVP's control-plane ingest (enroll and heartbeat), and the event firehose gets its own ADR; amended 2026-05-30 (ADR-0012): the MVP detection slice is TypeScript in `services/ingest`, transitorily; amended 2026-06-07 (SPEC-013): the forensic report render is a TypeScript module of `services/api`, not a Go service; amended 2026-10-14 (with ADR-0021): Rule 3's server-side default becomes TypeScript; the harness runner and the event firehose keep Go, and the event-firehose ADR confirms or changes the firehose's (ADR-0007 §Compliance); the four-language bound is unchanged | Accepted |
| [0003](0003-polyglot-storage.md) | Polyglot storage — amended 2026-05-30 (ADR-0012): alerts persist to Postgres only for the MVP; amended 2026-10-16: the dev stack runs the backends the code uses; NATS JetStream and MinIO join it with the first SPEC that uses them; amended 2026-10-19 (with ADR-0022): while MinIO is not built, an event's row stays in ClickHouse for its class's whole period, 365 days by default and the operator's setting; amended 2026-10-23 (with ADR-0022): the key-value store runs Valkey, a BSD-3-Clause fork that speaks Redis's protocol, in the deployment, the dev stack and the tests | Accepted |
| [0004](0004-agent-server-protocol.md) | Agent-Server secure protocol — amended 2026-05-23 (ADR-0009): the persistent disk buffer deferred, `sequence_number` persistence retired, a stale ADR-0008 cross-reference corrected; amended 2026-10-04 (SPEC-017): events travel inside the signed `body`, and the `batch_hash` verification step is retired; amended 2026-10-19 (with ADR-0022): enrollment runs over TLS 1.3, the agent validating the server against its configured trust anchor | Accepted |
| [0005](0005-detection-rules-and-ml-in-parallel.md) | Detection — rules and ML in parallel | Accepted |
| [0006](0006-cges-ocsf-alignment.md) | CGES alignment with OCSF v1.3 — amended 2026-10-13 (ADR-0020): while CGES is 0.x and no consumer outside the repository reads it, a change keeps the version, additive or breaking | Accepted |
| [0007](0007-ingest-language-typescript-mvp.md) | Ingest service language — TypeScript for the MVP control plane — amended 2026-06-07 (SPEC-013): its collateral "forensic = Go" mention is superseded | Accepted |
| [0008](0008-etw-crate-selection.md) | ETW crate selection for Windows event capture — amended 2026-10-19 (debt #115): the Phase 0 CI gate's question is answered; the hosted runner opens the Kernel-Process provider through ferrisetw in `rust-ci`'s Windows job, and the SPEC-005 marquee stays out of CI | Accepted |
| [0009](0009-event-delivery-and-buffer.md) | Event delivery semantics and agent buffer model — amended 2026-10-13 (ADR-0020): a dispatch callback may use structures of the agent's kept small besides the ring (the creation-time cache, a join table), with no I/O beyond its SPEC's anomaly lines, no lock of its own beyond theirs, and no wait for another event | Accepted |
| [0010](0010-agent-privilege-model-mvp.md) | Agent privilege model and installation posture for the MVP — amended 2026-05-29: the CI assumption is resolved by Fallback path 2; the SPEC-005 marquee leaves CI, since hosted `windows-latest` runners have no container runtime for testcontainers and the runner's ETW-open privilege was then untested, and is validated developer-local in an elevated shell; amended 2026-10-19 (debt #115): the CI assumption is validated by the agent's real-ETW tests in `rust-ci`'s Windows job, and the marquee stays out of CI, the hosted Windows runner having offered testcontainers no container runtime when measured | Accepted |
| [0011](0011-cges-process-activity-v0-1.md) | Per-class CGES jurisprudence — Process Activity v0.1 — amended 2026-05-23: §4's field formats (`process.created_time` integer nanoseconds UTC, `process.exit_code` signed int32); amended 2026-05-28: `process.created_time` and the event `time` travel as string-encoded nanoseconds; amended 2026-05-28 in §6: a Terminate gets its Launch's `process.uid` through the agent's in-memory `CreatedTimeCache`, and one whose Launch was not seen gets a uid of its own; amended 2026-10-04 (SPEC-017): the realized wire names (`process.image_file_name`, `process.parent_pid`, …) are the contract, and `image_file_name` carries the Win32 form when the translation resolves; amended 2026-10-09 (SPEC-017): a Terminate's `image_file_name` is ETW's base name, and the Win32 form applies to the path a Launch carries; amended 2026-10-13 (ADR-0020): the command line and the user come from the kernel's process events, the user on the wire as `process.user.uid` | Accepted |
| [0012](0012-normalize-before-correlate-pipeline.md) | Normalize-before-correlate pipeline (Detection MVP) — amended 2026-05-31 (ADR-0013): the correlation window is two distinct tunables, the 300 s dedup bucket and the incident window; amended 2026-06-07: production detection driver = in-process TypeScript scheduler in `services/ingest` (the Go `services/pipeline/` firehose is decoupled from the prod-caller role and still deferred); amended 2026-10-09 (SPEC-018): the read-model advances by an arrival cursor `(arrived_at, event_id)` behind a settle margin, the forward read drops `FINAL`, and the `dedup_key` makes a repeated match a no-op; amended 2026-10-11: classes 4001 (SPEC-019) and 3002 (SPEC-020) flow end to end into `cges_events`, the read-model does not read them, and the network, authentication, registry and file rules stay deferred; amended 2026-10-13 (ADR-0020): the command line and the user SID reach detection; the SID stays out of the `dedup_key` while alerts keep no retention | Accepted |
| [0013](0013-incident-correlation-windowing.md) | Incident correlation windowing — event-time basis | Accepted |
| [0014](0014-human-authentication-model.md) | Human authentication model — local self-hosted, password + TOTP | Accepted |
| [0015](0015-readonly-clickhouse-reader-in-api.md) | Read-only ClickHouse reader in `services/api` (forensic event-drill boundary) — amended 2026-10-14: three bullets of §Consequences and §Compliance name the three routes that read `cges_events`, the drill, the forensic export and the report (SPEC-010, SPEC-012, SPEC-013), not the drill alone (debt #70) | Accepted |
| [0016](0016-forensic-evidence-hash-chain.md) | Forensic evidence hash-chain — per-event SHA-256 chain + dedicated Ed25519 root signature — amended 2026-10-19 (with ADR-0022): the public key's trust anchor is its SHA-256 fingerprint, recorded outside CyberGuard at installation; a shipped verifier checks an export against it; no endpoint publishes the key | Accepted |
| [0017](0017-incident-email-notification.md) | Incident email notification — generic SMTP, fire-and-forget on incident create | Accepted |
| [0018](0018-cges-network-activity-v0-1.md) | Per-class CGES jurisprudence — Network Activity v0.1 | Accepted |
| [0019](0019-cges-authentication-v0-1.md) | Per-class CGES jurisprudence — Authentication v0.1 — amended 2026-10-11: the gate's successful logon is an S4U network logon of the current user (measured: a `net use` to the loopback with the caller's own credentials leaves no event); amended 2026-10-13 (ADR-0020): the other classes' rows keep no time-to-live, two columns of 1007 do; amended 2026-10-19 (with ADR-0022): retention is the operator's setting per class, 365 days by default for every class | Accepted |
| [0020](0020-process-command-line-and-user.md) | Process command line and user v0.1 — the kernel's process events joined to Kernel-Process, secrets replaced on the agent, the process's own SID — amended 2026-10-23 (with ADR-0022): its two items for roadmap §F, the ClickHouse server's settings and the operator setting for the retention or the pattern list, join the deployment contract with SPEC-021's code | Accepted |
| [0021](0021-soar-mvp-playbook-executor.md) | SOAR for the MVP — declarative playbooks run in-process on incident create, containment in dry-run only | Accepted |
| [0022](0022-single-node-production-deployment.md) | Single-node production deployment — Docker Compose behind a TLS proxy, a credential per service, published images | Accepted |

## Dependencies

- ADR-0002 → ADR-0001 (defines the top-level components this ADR assigns languages to)
- ADR-0003 → ADR-0001 (defines the components whose data this ADR routes)
- ADR-0003 → ADR-0002 (language choices cite NATS and ClickHouse Go client maturity)
- ADR-0004 → ADR-0001 (locates `agent/` and `services/ingest/`)
- ADR-0004 → ADR-0002 (Rust for agent is an input to the protocol; server language is language-agnostic per ADR-0007)
- ADR-0004 → ADR-0003 (Redis hosts nonce cache and revocation list)
- ADR-0005 → none (detection principle is independent of stack and storage)
- ADR-0006 → ADR-0001 (places `schemas/cges/` per the monorepo layout)
- ADR-0006 → ADR-0003 (ClickHouse partitioning on `event_id` and `occurred_at`; MinIO hosts the offloaded raw payload via `cg_raw_ref`)
- ADR-0006 → ADR-0004 (CGES is the body inside the signed envelope defined by the agent-server protocol)
- ADR-0006 → ADR-0005 (`cg_detection_source`, `cg_trust_sources`, and `cg_score` are first-class CGES fields because of the detection decision)
- ADR-0007 → ADR-0002 (amends the `services/ingest/` language row to TypeScript for the MVP)
- ADR-0007 → ADR-0004 (the agent-server protocol is language-agnostic; ingest implements it in TypeScript)
- ADR-0008 → ADR-0001 (the agent at `agent/cg-agent/` is where the ETW consumer lives)
- ADR-0008 → ADR-0002 (Rust for the agent; this ADR picks a Rust crate)
- ADR-0008 — additive new decision; supersedes nothing, amends nothing
- ADR-0009 → ADR-0003 (ClickHouse engine + partitioning unchanged from this ADR)
- ADR-0009 → ADR-0004 (amends in part via the in-place 2026-05-23 amendment; does not supersede)
- ADR-0009 → ADR-0006 (`event_id` is UUIDv7 per CGES; the dedup contract relies on this)
- ADR-0009 → ADR-0008 (the ring's enqueue side is the ferrisetw callback path)
- ADR-0010 → ADR-0001 (the privilege model attaches to the agent's process at `agent/cg-agent/`)
- ADR-0010 → ADR-0002 (Rust for the agent; this ADR specifies how that Rust process runs)
- ADR-0010 → ADR-0008 (ETW driver of the elevation requirement; ADR-0008 forward-references here)
- ADR-0010 — additive new decision; supersedes nothing, amends nothing
- ADR-0011 → ADR-0006 (per-class jurisprudence pattern under ADR-0006's framework deferral; first per-class ADR)
- ADR-0011 → ADR-0008 (ETW Kernel-Process provider whose crate ADR-0008 selects)
- ADR-0011 → ADR-0009 (the `process.uid` recipe's agent-side mapping, the in-memory `CreatedTimeCache` of §6's Amendment 2026-05-28, keeps to ADR-0009's "no on-disk event state" (§Consequences, §Compliance))
- ADR-0011 → ADR-0010 (elevated process precondition enables ETW Kernel-Process consumption)
- ADR-0011 — additive new decision; supersedes nothing, amends nothing
- ADR-0012 → ADR-0002 (amends in part: `services/pipeline/` Go assignment superseded for the MVP detection slice only; restored on the firehose ADR)
- ADR-0012 → ADR-0003 (amends in part: §Retention Alerts row → Postgres-only for the MVP)
- ADR-0012 → ADR-0005 (the anticipated normalize-before-correlate ADR; clarifies scoring composition for the absent-source case)
- ADR-0012 → ADR-0006 (emits the CGES Alert / score / mitre classes under its framework)
- ADR-0012 → ADR-0007 (extends the transitory-TypeScript-for-MVP logic to the detection slice; same firehose-deferred exit)
- ADR-0012 → ADR-0009 (FINAL / GROUP BY reads because of ReplacingMergeTree at-least-once delivery)
- ADR-0012 → ADR-0011 (reads Process Activity 1007; flags the §4 ETW-mapping contradiction)
- ADR-0013 → ADR-0012 (amends in part: ADR-0013 §2 amends ADR-0012 §8's single-tunable framing in place — incident/stateful correlation uses its own window, distinct from and wider than the 300 s dedup bucket; does not supersede)
- ADR-0014 → ADR-0001 (locates the human-facing `services/api` component in the monorepo)
- ADR-0014 → ADR-0002 (`services/api` = TypeScript + Fastify + Zod, per the language table row)
- ADR-0014 → ADR-0003 (consumes the storage homes: users / RBAC / audit_log → Postgres, sessions → Redis; does not amend)
- ADR-0015 → ADR-0003 (consumes the ClickHouse storage home as a read-only reader in `services/api`; does not amend or re-route storage)
- ADR-0015 → ADR-0014 (preserves the human/agent trust-boundary split — keeps the forensic event read in the user-facing api, not the agent boundary)
- ADR-0015 → SPEC-009 (amends-by-scope `:34`: the alert→source-event drill SPEC-009 deferred-with-destination is delivered by this ADR + SPEC-010)
- ADR-0016 → ADR-0014 (introduces a dedicated forensic signing key in the human-facing `services/api`; does NOT reuse the ingest CA, preserving the human/agent trust-boundary split)
- ADR-0016 → ADR-0003 (consumes the evidence definition; explicitly does NOT use the MinIO at-rest home — recorte part (b))
- ADR-0016 → SPEC-010 (the canonicalized drill output `EventTimeline` is the evidence unit; requires its `(time, event_id)` total-order amendment)
- ADR-0016 → SPEC-011 / SPEC-007 (the incident the evidence is scoped to: its grouped alerts and aggregated severity)
- ADR-0016 → SPEC-003 (reuses the JCS canonicalization discipline; does not amend it)
- ADR-0017 → ADR-0009 (best-effort notification is a downstream projection off the at-least-once durable record; the event-durable vs notify-best-effort asymmetry — a missed email is recoverable from the persisted incident, a dropped event is not)
- ADR-0017 → ADR-0012 (the in-process TypeScript scheduler added by Amendment 2026-06-07 gives `runDetectionCycle` its production caller, the deferred Go `services/pipeline/` firehose being decoupled from that role; notification rides whatever drives the cycle — test-validated altitude, now resolved by the prod-driver merge)
- ADR-0017 → SPEC-007 (hangs the notify off the `upsertIncident` incident-grouping seam, on incident create only; discharges the SPEC-007 `:37` / SPEC-008 `:42` notifier deferral, incident-notification half)
- ADR-0018 → ADR-0011 (second instance of the per-class pattern of §1; reuses the dual layer of §3 and the `process.uid` recipe of §6, so a connection and the Launch of its process share one uid)
- ADR-0018 → ADR-0008 (Kernel-Network through ferrisetw, as a second provider of the agent's existing session; no raw Win32 call) / ADR-0010 (no privilege beyond the one that session already requires)
- ADR-0018 → ADR-0012 (class 4001 stops being schema-only; the read-model keeps selecting class 1007, and rules over 4001 stay deferred) / SPEC-019 (the production specification)
- ADR-0018 extends decision D6 (`docs/handoff-session-10.md`) to a second class: one `cges_events` table, told apart by `class_uid`; a reader that interprets `activity_id` or class-specific columns must select by it, the forensic drill excepted on a stated condition
- ADR-0019 → ADR-0011 (third instance of the per-class pattern of §1; the `time` encoding) / ADR-0018 (stores its class in `cges_events` as §Compliance asks, and keeps the reader's obligation of §9)
- ADR-0019 → ADR-0010 (the Security log is read with the elevated token that posture already gives; no privilege added) / ADR-0008 (its rule on raw Win32 calls concerns ETW; the Event Log calls are this ADR's) / ADR-0009 (live only: no cursor on disk while the disk buffer is deferred)
- ADR-0019 → ADR-0006 (`cg_elevated_token` is a `cg_*` extension) / ADR-0012 (class 3002 stops being schema-only once SPEC-020 lands)
- ADR-0019 → ADR-0015 (the forensic drill reads no class 3002 while no alert cites one; access to logon data is decided by the SPEC of the first surface that shows it) / SPEC-020 (the production specification)
- ADR-0019 → ADR-0016 (logon rows are deleted 365 days after arrival; the first rule that cites a logon reconciles that retention with the evidence an alert seals)
- ADR-0020 → ADR-0011 (amends §4's `process.cmd_line` and `process.user.uid` rows and the wire name `process.subject_user_sid`, and decides the replacement of secrets that §Out of scope deferred, which also bounds what §2's `raw_data` may carry; co-located, landed with ADR-0020) / ADR-0012 (amends §3's names and population finding and replaces §5's forward path, the SID staying out of the `dedup_key` until alerts have a retention; co-located, landed with ADR-0020; resolves §3's flag on the ETW mapping)
- ADR-0020 → ADR-0008 (a second session through ferrisetw's `KernelTrace`, no new crate, because one session with the system process provider needs build 20348 and the agent runs on Windows 10 and Windows Server 2019, Manuel's decision of 2026-10-12; the side-channel helper by session name) / ADR-0009 (amends what a dispatch callback may do: use structures kept small besides the ring, the join table and its slots among them; the callbacks complete the join, the batch reads it on the drain path; co-located, landed with ADR-0020) / ADR-0010 (the elevated token opens the kernel session; no privilege added)
- ADR-0020 → ADR-0006 (`cg_cmd_line_redacted`, `cg_cmd_line_possibly_truncated` and `cg_kernel_record_unavailable` are `cg_*` extensions; the user follows OCSF naming on the wire, `process.user.uid`; amends §Decision's and §Compliance's versioning rule while CGES is 0.x, which the two CGES READMEs restate, co-located, landed with ADR-0020) / ADR-0019 (the 365 days from arrival; the logons that carry a person's name; amends §9's and §Consequences' statements on the other classes' time-to-live, co-located) / ADR-0003 (two columns of class 1007 gain a time-to-live; the retention table stays unimplemented, debt #47) / ADR-0016 (the first SPEC that puts the command line or the user SID into the drill or a report reconciles their 365 days with the evidence an alert seals) / ADR-0015 (the api's reader, read-only in use and enforced by no grant, could read the new columns) / ADR-0018 (readers select by class, §9)
- ADR-0020 → threat model (`docs/security/threat-model.md`: adds the command lines and the users' SIDs to §1 and §8, a Tampering entry on command lines shaped against detection, and two accepted threats; co-located, landed with ADR-0020) / roadmap (the rules over the two fields are §B3, Manuel's decision of 2026-10-12)
- ADR-0021 → ADR-0002 (SOAR is a TypeScript module inside `services/ingest`, under Rule 3 as amended 2026-10-14; the playbook schema in `schemas/playbooks/v0.1/` under Rule 6) / ADR-0001 (playbooks in `playbooks/`; `services/soar/` stays a placeholder)
- ADR-0021 → ADR-0005 (applies the action policy by reversibility and `trust_sources`: no destructive action, semi-reversible ones in dry-run only, so no `approval` step) / ADR-0017 (a second fire-and-forget carrier on the incident-create seam)
- ADR-0021 → ADR-0016 and SPEC-012 (the per-step hash chain of an execution record uses SPEC-012's construction, without a signature) / ADR-0003 (the record in Postgres; the MVP takes no SOAR lock, a unique key in Postgres prevents a second run)
- ADR-0021 → ADR-0012 (the module lives on the transitional seam in ingest and moves with the cycle) / ADR-0007 (the event-firehose ADR confirms or changes that workload's language) / ADR-0004 and ADR-0010 (no channel from the server to the agent; a live containment action would change both)
- ADR-0021 → threat model (`docs/security/threat-model.md`: restates §5 cg-soar's trust boundary and mitigations for the in-process MVP module, adds the api's read of execution records, and accepts that containment is not performed, that SOAR delays detection, and that the database's owner can rewrite a record)
- ADR-0022 → ADR-0001 (the bundle in `deploy/docker/`, by ADR-0001's Amendment 2026-10-19) / ADR-0003 (runs the backends its SPEC lists, as the Amendment 2026-10-16 provides; one instance each, with a credential per service; its retention table for normalized events realised in ClickHouse for the whole period, by its Amendment 2026-10-19, its MinIO tier not built; the key-value store run as Valkey, by its Amendment 2026-10-23)
- ADR-0022 → ADR-0004 (enrollment over server-authenticated TLS, by ADR-0004's Amendment 2026-10-19, in place as its earlier amendments were; the agents' mTLS terminated by ingest, its cipher suites held to ADR-0004's two) / ADR-0010 (the agent's installation posture is left to a later SPEC of roadmap §F, and the first release waits for it, as its §Consequences asks before any external deployment)
- ADR-0022 → ADR-0020 (its two items for roadmap §F go with SPEC-021's code, by ADR-0020's Amendment 2026-10-23) / ADR-0021 (the playbook records' write-once triggers held by ingest's owner role)
- ADR-0022 → ADR-0014 and ADR-0015 (the api reachable from no network outside the host; its read-only ClickHouse reader enforced by a grant and a row policy) / ADR-0016 and ADR-0019 (the forensic key's anchor and the retention, by their Amendments 2026-10-19)
- ADR-0022 → threat model (`docs/security/threat-model.md`: closes or narrows R2, R3, R4, R7, R8, R10, R11, R12, R13, R15 and R16 once SPEC-023's code lands, R5 left to the renewal SPEC, R1 moved with debt #87 to the agent-installation SPEC and R14 with debt #98 to the dashboard SPEC; the secrets plan of §Secrets management realised as generated environment-variable secrets)
