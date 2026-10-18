# ADR-0021: SOAR for the MVP — declarative playbooks run in-process on incident create, containment in dry-run only

- Status: Accepted
- Date: 2026-10-14
- Last updated: 2026-10-14
- Deciders: Manuel (project owner), Claude Code (proposal and implementation)

## Context

MVP criterion 6 is "1 SOAR playbook operational (dry-run + notification, no destructive actions yet)" (`docs/product/blueprint.md:751`). Roadmap §E makes it the next phase. Manuel delegated the decisions pending on him on 2026-10-14, and B2 and B3 are parked (`docs/product/roadmap.md`). The repository fixes much of the ground already:

1. **Where an incident is created.** `runDetectionCycle` creates incidents in `services/ingest/src/detect/index.ts`. Right after `upsertIncident` reports a create (`incident.created`, `index.ts:87`; the `xmax = 0` signal of `services/ingest/src/detect/incidents.ts`), it calls `notifyIncidentCreated` (SPEC-014). ADR-0017 §2 made that call the pipeline's first external side effect: after the commit, outside the transaction, fire-and-forget, never failing the cycle. No other code learns of a new incident. An incident whose severity rises later, through SPEC-011's MAX on update, sends no create.
2. **No bus.** The Blueprint's SOAR is a Go service, `cg-soar` (`docs/product/blueprint.md:110`), and the threat model triggers it from NATS subjects (`docs/security/threat-model.md:109`). The bus does not exist and waits for the event-firehose ADR, which also decides that workload's language and component boundary (ADR-0007 §Compliance, `docs/adr/0007-ingest-language-typescript-mvp.md:86`). `services/soar/` and `playbooks/` hold only READMEs.
3. **The action policy is decided.** ADR-0005 §Action policy by reversibility (`docs/adr/0005-detection-rules-and-ml-in-parallel.md:37-45`) sorts actions:
   - reversible: tag, notify, …;
   - semi-reversible: isolate an endpoint, kill a process;
   - destructive: always with a logged human approval.

   Its §Playbook governance (`:47-57`) has each playbook declare `trust_sources`. Its §Compliance (`:157`) requires that declaration and forbids a destructive action without an `approval` step. The Blueprint's own table calls `agent.isolate` destructive (`docs/product/blueprint.md:463`); this ADR follows ADR-0005, which is Accepted.
4. **Playbooks are configuration.** ADR-0002 Rule 6 (`docs/adr/0002-language-per-component.md:63`) makes playbooks declarative YAML under `playbooks/`, with their schemas versioned in `schemas/` and validated in CI. ADR-0001 places `playbooks/` at the root (`docs/adr/0001-monorepo-layout.md:40`). The Blueprint's model (`docs/product/blueprint.md:411-469`) has a trigger, ordered steps, a dry-run per action, human approval for destructive actions, a rollback per step and an execution audit record. Its example names the mail action `email.send` and triggers on `incident.severity >= 8`; this ADR names it `notify.email`, after SPEC-014, and reads OCSF's `severity_id` (§Context 7).
5. **The threat model's mitigations for cg-soar** (`docs/security/threat-model.md:115-121`): an immutable execution record, hash-chained per step; execution timeouts; a maximum step count per execution; and idempotency keys that block re-entry. It also says that no destructive action runs without a logged `human.approve`.
6. **The agent receives no commands.** The agent–server protocol carries events and heartbeats from the agent to the server (ADR-0004), and the heartbeat's answer carries no instruction. A live containment action would need a new channel: a new trust boundary and a change to the agent's privileges (ADR-0010).
7. **The incident's severity is OCSF's** (`severity_id`, 0–6, SPEC-011). Of the ten rules, one is `critical` and six are `high`, which map to 5 and 4 (`services/ingest/src/detect/engine.ts`).
8. **SPEC-014's SMTP transport sets no timeout of its own** (`services/ingest/src/notify/transport.ts`), so a send can wait for the mail library's defaults, which are minutes.

## Decision

### 1. An in-process module of `services/ingest`, on the incident-create seam

SOAR is a TypeScript module, `services/ingest/src/soar/`, called from `runDetectionCycle` right after the incident-create notification, under ADR-0017 §2's terms:

- it runs after the incident's commit and outside its transaction;
- its failure is logged and never reaches the cycle;
- there is no retry.

Like SPEC-014's send (`index.ts:88`), the run is awaited, so it delays the cycle. That is the concern for which ADR-0017 rejected blocking the cycle until a send ends (its A2). Here §4's bounds limit the delay, and A7 defers the queue that would remove it.

It is triggered by `incident.created` only, the one event the seam knows. The severity it sees is the incident's at creation. The Blueprint's separate Go service is not built. The module lives on the transitional seam ADR-0012 §1 placed in ingest, and if the event-firehose ADR moves the cycle elsewhere, the module moves with it, as ADR-0017's notification does (`docs/adr/0017-incident-email-notification.md:69`). ADR-0002's Amendment 2026-10-14 makes TypeScript the server-side default, which this module relies on.

### 2. Declarative playbooks, validated at boot and in CI

Playbooks are YAML documents, loaded and validated at every boot, whether or not the detection driver runs, from `INGEST_SOAR_PLAYBOOKS_DIR`. The default is the repository's `playbooks/` bundled into the image, as the rules' directory is.

- Each playbook declares `id`, `version`, `name`, `trust_sources` (ADR-0005), a `trigger` (`on: incident.created`, and a `when` on the incident's `severity_id`) and its ordered `steps`.
- Their JSON Schema is versioned in `schemas/playbooks/v0.1/` (ADR-0002 Rule 6). The loader validates each playbook against it and against the policy of §3. CI validates the shipped playbooks against it too, and the schema-validation workflow's path filter gains `playbooks/**`.
- The validator is `ajv` (8.20.0, MIT), which becomes a direct dependency of `services/ingest`. It is already in the tree through Fastify, and it does no cryptography and no networking, runs no install script and needs no native build. CI's validation reads the YAML playbooks through the same loader, since `ajv-cli` reads JSON.
- A playbook that fails the schema or the policy stops the boot, as a rule the loader cannot read does.
- An empty or absent directory leaves SOAR idle, which the boot logs. Unlike the rules, no playbooks is a legitimate state.

The variable is a new surface of the deployment contract, decided under Manuel's delegation of 2026-10-14, with a safe default. The SPEC fixes its name, and it joins the operator surface of roadmap §F.

### 3. A closed set of actions, and containment in dry-run only

The MVP's actions are a closed list. Each is classed as ADR-0005 classes it, and each runs `live` or `dry_run`, `dry_run` by default:

| Action | ADR-0005 class | Live in the MVP | What it does |
|---|---|---|---|
| `agent.isolate` | semi-reversible | no: `dry_run` only | Records the agent it names and what isolating it would do. Nothing reaches the agent (§Context 6). |
| `notify.email` | reversible | yes | Sends the playbook's message, with the outcomes of the steps before it, over SPEC-014's SMTP transport, to SPEC-014's recipient. It is recorded as skipped when notification is not configured. |

The loader refuses any action outside the list, a `live` `agent.isolate`, and any destructive action. So no step needs `human.approve` in the MVP, and none exists.

A playbook runs when the source of the alert that created the incident is one of its `trust_sources`. Every MVP incident comes from a rule (`cg_detection_source = rule`). The schema admits ADR-0005's three values (`rule`, `ml`, `hybrid_only`), and how `ml` and `hybrid_only` match an incident is decided when ML produces alerts.

### 4. One execution record per playbook and incident: claimed first, completed once

Each run has one row in a new Postgres table, in `services/ingest`'s migrations beside `alerts` and `incidents`. The row holds:

- the execution id, the org, the playbook's id and version, and the incident;
- the start and end times and the outcome;
- the steps, each with its action, its mode, its outcome (`done`, `would_do`, `skipped` or `failed`) and a short detail.

The design follows from the threat model's mitigations:

- **Idempotency key, before any step.** The run first inserts its row, marked `running`, under a unique pair (playbook, incident), with `ON CONFLICT DO NOTHING`. A run that finds the row already there does nothing, so a re-entry sends nothing and writes nothing.
- **Completed once.** At its end the run updates the row once, from `running` to its outcome, with its steps. A trigger in Postgres then refuses any update or delete of a completed row. A run that dies midway leaves its row `running`, visible as unfinished, and is not retried.
- **Hash chain.** The steps are chained in SPEC-012's construction (`docs/specs/SPEC-012-forensic-evidence-hashchain.md`, §Data contracts §2):
  - `chain_0` is `SHA-256("")`;
  - each link is the SHA-256 of the previous link and of the SHA-256 of the step's canonical (JCS) form;
  - the step's time is inside that form.

  The chain is unsigned. It shows an edit that does not recompute it, and nothing more.
- **What resists whom.** The trigger and the chain hold against a writer that goes through the application. The owner of the database can bypass both. In the dev stack, ingest and the api share one Postgres user, the image's superuser (`infra/dev/docker-compose.dev.yml:96`, `:167`). Resisting that owner needs a separate role or an external anchor (§Out of scope).
- **Bounds.**
  - A playbook has at most 10 steps.
  - A step has at most 15 s, and a run at most 30 s. A step cut by either is `failed`.
  - A failed step ends the run, and the steps after it are recorded as `skipped`.
  - The SPEC bounds a whole send to its step's time: the transport's connection, greeting and socket timeouts, and the connection closed when the step's time is out. SPEC-014's own notification gains the same bound.
- **Reading it.** The api reads the record, scoped to the session's org. The route and its read model are the SPEC's.

The cycle awaits each run, as it awaits SPEC-014's notification. A cycle that creates N incidents matching M playbooks can therefore be delayed by up to N × M × 30 s.

### 5. The MVP's playbook

One playbook ships, `playbooks/pb_high_severity_containment.yml`. Its trigger is `incident.created` with `severity_id` of 4 (High) or more, and its `trust_sources` is `["rule"]`. It has two steps:

1. `agent.isolate`, in dry-run: it records the isolation of the incident's agent that it would do.
2. `notify.email`, live: it tells the SOC that containment of that agent is pending, with what step 1 recorded.

This is criterion 6 as the Blueprint states it: dry-run and notification, with no destructive action. For such an incident, SPEC-014 still sends its own create notification, so the SOC gets two emails with different purposes.

## Alternatives considered

### A1 — The Go `services/soar/` service, fed by NATS

The Blueprint's design. It needs the event bus, which does not exist and is deferred to the event-firehose ADR, which also decides that workload's language. Rejected for the MVP.

### A2 — SOAR in `services/api`, polling for new incidents

The api already reads incidents. But only ingest's upsert knows that an incident was created rather than updated (ADR-0017's create signal). Polling needs its own cursor, adds latency, and opens a second path that writes. Rejected.

### A3 — A playbook written as TypeScript code

Simpler to start, but ADR-0002 Rule 6 forbids replacing playbooks with hand-rolled code, and an operator could not read or change them. Rejected.

### A4 — Live isolation through a new command channel to the agent

Real containment. But it is a new trust boundary (the server commanding an elevated agent) and a change to the agent's privileges and protocol (ADR-0004, ADR-0010). It also needs `human.approve`, and the criterion says "no destructive actions yet". Deferred, with its own ADR.

### A5 — Executions in the api's `audit_log`

The audit table exists, but it is SPEC-008's, written by the api, and shaped for authentication events. Ingest writing into the api's table would cross ownership, and the table has no place for steps or a chain. Rejected.

### A6 — Locks in Redis for idempotency

ADR-0003 homes SOAR locks in Redis (`docs/adr/0003-polyglot-storage.md:31`). The MVP's runs take no lock. The claimed row's unique pair in Postgres prevents a second run, and the claim, the record and the key are one row, with nothing to expire. Redis stays the home for locks when runs become concurrent.

### A7 — A durable queue between the cycle and SOAR

An outbox of pending runs, drained by a worker, would keep SMTP from delaying detection and would survive a crash between the incident's commit and the run. It is ADR-0017's A3 for notification, and it brings a worker, a delivery state and their schema. Deferred, to be decided with notification's own delivery guarantees.

## Consequences

### Positive

- Criterion 6 can close with no new service, language or store. The `yaml` parser, the SMTP transport and the JCS canonicalisation are already in ingest, and SHA-256 comes with Node's crypto. One direct dependency is added, `ajv`, already in the tree.
- The action policy of ADR-0005 is enforced at load time, so no shipped or operator-written playbook can run a live semi-reversible or destructive action.
- Each run leaves one record, claimed before its first step, that the api can read.
- SPEC-014's send gains timeouts.

### Negative

- SOAR shares the detection process, and a slow SMTP relay delays the cycle, by up to 30 s per run (§4).
- Containment is only recorded, never done. The playbook says what it would isolate; an analyst must act.
- A run lost to a crash between the incident's commit and its claim is not run again (A7).
- The record resists edits through the application, not the database's owner (§4).
- An incident that becomes High after its creation does not run the playbook.
- The chain code exists in the api (SPEC-012). Ingest repeats it, or the two share it in a package. A shared package may count as a new component, which is Manuel's to decide, as debt #7 records for `canonicalize`.
- The call is added in `services/ingest/src/detect/index.ts`, a path the elevated gate covers, so the implementation runs that gate.

### Neutral

- `services/soar/` stays a placeholder.
- Per-organisation playbooks and trust configuration (ADR-0005 §Playbook governance) wait for a second organisation; the MVP runs one.

## Compliance

- ADR-0005: every playbook declares `trust_sources`. No destructive action exists in the MVP, and semi-reversible ones run only in dry-run, so no `approval` step is required. Any later live semi-reversible or destructive action needs `human.approve` and its own ADR (A4).
- ADR-0002: Rule 6, through the playbook schema in `schemas/playbooks/v0.1/`, validated at boot and in CI. Rule 3 as amended on 2026-10-14.
- ADR-0017 §2: the module is a second fire-and-forget carrier on the same seam, after the commit.
- The threat model's §5 is amended with this ADR (its Amendment 2026-10-14).

## Out of scope

Each item has its destination in brackets.

- `human.approve`, rollback, and any live semi-reversible or destructive action [their own ADR, A4].
- Other actions: `webhook.post`, `incident.tag`, `incident.assign` and third-party integrations [a later SOAR SPEC].
- Triggers other than `incident.created`, including a severity raise, and manual runs from the api [a later SOAR SPEC].
- A queue between the cycle and SOAR [A7, with notification's delivery guarantees].
- A record that resists the database's owner: a separate role, or an external anchor [the deployment contract of roadmap §F].
- Editing playbooks at run time, and per-organisation playbooks [after the MVP's single organisation].
- A dashboard view of executions [a later dashboard increment].

## Open questions

1. **Retention of execution records.** They are kept as long as incidents are, with no TTL. The threat model calls them an execution audit log, and ADR-0003's retention table has a row for the audit log (a year in Postgres, seven signed in MinIO). Which row applies is decided with the retention of incidents. **Reopen** at roadmap §F, or when incidents get a retention.

## Decision record

Proposed by Claude Code on 2026-10-14 under Manuel's delegation of the same day ("lo que este pendiente de mi decidelo tu en base al beneficio del proyecto"). The load-bearing decisions, each with the alternatives it discards:

- §1, in-process on the seam (A1, A2);
- §2, YAML validated at boot and in CI (A3);
- §3, containment in dry-run only (A4);
- §4, one record claimed before its steps and completed once, in ingest's Postgres (A5, A6), with the cycle awaiting the run (A7);
- §5, one playbook at severity High or more.

Ratified by Manuel on 2026-10-14, in chat: "si a 1, 2 y  3" (point 1 of three put to him: this ADR with the threat model's amendment). The status is Accepted from that date.

## References

- [ADR-0001](0001-monorepo-layout.md) — `playbooks/` at the root.
- [ADR-0002](0002-language-per-component.md) — Rule 6 (playbooks as schema-validated YAML); Rule 3, as amended on 2026-10-14.
- [ADR-0003](0003-polyglot-storage.md) — Postgres for relational state; Redis for SOAR locks (A6).
- [ADR-0004](0004-agent-server-protocol.md) — no channel from the server to the agent (§Context 6).
- [ADR-0005](0005-detection-rules-and-ml-in-parallel.md) — the action policy by reversibility and `trust_sources`.
- [ADR-0007](0007-ingest-language-typescript-mvp.md) — the event-firehose ADR decides that workload's language and boundary.
- [ADR-0010](0010-agent-privilege-model-mvp.md) — the agent's privileges, which a live containment channel would change.
- [ADR-0012](0012-normalize-before-correlate-pipeline.md) — the transitional seam in ingest, §1.
- [ADR-0016](0016-forensic-evidence-hash-chain.md) and [SPEC-012](../specs/SPEC-012-forensic-evidence-hashchain.md) — the chain construction §4 reuses.
- [ADR-0017](0017-incident-email-notification.md) — the seam and its fire-and-forget terms; the SMTP transport; A3, the queue A7 defers to.
- [SPEC-014](../specs/SPEC-014-incident-notification.md) — the transport and recipient `notify.email` uses.
- [Threat model](../security/threat-model.md) — §5 cg-soar, amended with this ADR.
- [Blueprint](../product/blueprint.md) — §10 SOAR Engine; §18 criterion 6.
