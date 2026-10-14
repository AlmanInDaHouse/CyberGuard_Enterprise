# SPEC-022: SOAR for the MVP — playbooks, their execution record, and one playbook

- **ID:** SPEC-022
- **Title:** SOAR for the MVP — playbooks, their execution record, and one playbook
- **Status:** Accepted
- **Depends on:**
  - ADR-0021 — the decisions this SPEC implements: the in-process module on the incident-create seam (§1), playbooks validated at boot and in CI (§2), the closed action list with containment in dry-run only (§3), the execution record (§4), and the one playbook (§5).
  - ADR-0005 — the action policy by reversibility and `trust_sources`.
  - ADR-0002 — Rule 6 (the playbook schema in `schemas/`, validated in CI), and Rule 3 as amended on 2026-10-14.
  - ADR-0017 and SPEC-014 — the seam's fire-and-forget terms and the SMTP transport. This SPEC amends SPEC-014 **by scope**: its send gains the bound of §Operational §6, and its failure log carries the error's code instead of its message. Its acceptance criteria are unchanged.
  - SPEC-012 — the chain construction that §Data contracts §3 reuses.
  - SPEC-009 — the read API's error semantics (Amendment 2026-10-14), for the new read route of §Data contracts §4. This SPEC also delivers **by scope** part of its §Out of scope "Cases, Playbook executions, …" (`:38`): the read of playbook executions, with no dashboard view.
  - SPEC-007 and SPEC-011 — the incident, and its severity at creation.
  - `docs/product/roadmap.md` — §E, the phase this SPEC is the contract for.
- **Authors:** Manuel (project owner), Claude Code (proposal and implementation)

## Context

MVP criterion 6 is "1 SOAR playbook operational (dry-run + notification, no destructive actions yet)" (`docs/product/blueprint.md:751`). ADR-0021, Accepted on 2026-10-14, decides how. This SPEC fixes the mechanics ADR-0021 leaves to it:

- the playbook document and its schema;
- the loader and its policy;
- the trigger;
- the execution record's table, its write-once trigger, and its chain;
- the bounds, and how an SMTP send is held to them;
- the api's read route;
- the shipped playbook.

It also fixes where the code goes, how CI validates the playbooks, and which tests cover each criterion.

## Scope

### In scope

- The playbook schema `schemas/playbooks/v0.1/playbook.json`, with leaf READMEs in `schemas/playbooks/` and `schemas/playbooks/v0.1/`, and a row in `schemas/README.md`.
- The loader in `services/ingest/src/soar/`, with `ajv` (8.20.0, MIT) as a direct dependency of `services/ingest` (ADR-0021 §2), and `canonicalize` exported from `services/ingest/src/jcs.ts`.
- The variable `INGEST_SOAR_PLAYBOOKS_DIR` and its default. The Dockerfile's copies of `playbooks/` and `schemas/playbooks/`. The dev compose's variable and `infra/dev/.env.example`.
- The run of the matching playbooks at incident create, from `runDetectionCycle`, after SPEC-014's notification, with the production driver passing the module in.
- The actions `agent.isolate` (dry-run only) and `notify.email`.
- The table `playbook_executions` (an ingest migration), its write-once triggers, and the chain over its steps.
- The step and run bounds, and a bounded SMTP send, used by `notify.email` and by SPEC-014's notification.
- The api route `GET /v1/incidents/:id/playbook-executions`.
- The playbook `playbooks/pb_high_severity_containment.yml`, and `playbooks/README.md` brought up to date.
- The acceptance tests of §Acceptance criteria. CI's path filters, and the validation step of `.github/workflows/schema-validation.yml` and `task validate-schemas` (§Operational §8).

### Out of scope

Each item has its destination in brackets.

- Everything listed in ADR-0021 §Out of scope [its destinations there].
- A dashboard view of executions [a later dashboard increment].
- A route that verifies a record's chain [a later SOAR SPEC; soar_ac_005 shows the recomputation].
- Sharing the chain code with `services/api` in a package [debt #7's decision, Manuel's: a new component].

## Data contracts

### 1. The playbook document

A YAML file, `*.yml` or `*.yaml`, holding one playbook. Its JSON Schema is `schemas/playbooks/v0.1/playbook.json` (draft 2020-12), with `additionalProperties: false` at every level:

| Field | Type | Rule |
|---|---|---|
| `id` | string | `^pb_[a-z0-9_]{1,60}$`; unique across the directory. |
| `version` | integer | 1 or more. |
| `name` | string | 1 to 120 characters. |
| `description` | string | Optional; up to 500 characters. |
| `trust_sources` | array of strings | 1 to 3 unique values of `rule`, `ml`, `hybrid_only` (ADR-0005). In the MVP only `rule` matches an incident (§Operational §2). |
| `trigger.on` | string | `incident.created`. |
| `trigger.when.severity_id_gte` | integer | 0 to 6. |
| `steps` | array | 1 to 10 steps. |
| `steps[].id` | string | `^[a-z][a-z0-9_]{0,31}$`; unique in the playbook (the loader checks it, §Operational §1). |
| `steps[].action` | string | `agent.isolate` or `notify.email`. |
| `steps[].mode` | string | `live` or `dry_run`; `dry_run` when absent. |
| `steps[].params` | object | Absent for `agent.isolate`. Required for `notify.email`, with `subject` (1 to 200 characters) and `body` (1 to 4000 characters), both required. |

A `subject` or `body` is a template. It may hold only these placeholders, each written `{{name}}`:

- `incident.id`, `incident.title`, `incident.severity_id`, `incident.agent_id`, `incident.org_id`;
- `playbook.id`, `execution.id`;
- `steps` (in `body` only).

`{{steps}}` expands to one line per step that ran before the current one: `<step id> <action> <mode>: <outcome> — <detail>`. A rendered subject has every line break replaced with a space.

### 2. The execution record

Each run is one row in `playbook_executions`, which a new ingest migration (`0008_playbook_executions`) creates on the shared Postgres:

| Column | Type | Rule |
|---|---|---|
| `execution_id` | `uuid` | Primary key; a UUIDv7 generated by ingest. |
| `org_id` | `text` | Not null. |
| `incident_id` | `uuid` | Not null; references `incidents`. |
| `agent_id` | `uuid` | Not null; the incident's agent. |
| `playbook_id` | `text` | Not null. |
| `playbook_version` | `integer` | Not null. |
| `status` | `text` | `running`, `completed` or `failed`. |
| `started_at` | `timestamptz` | Not null; the time of the claim. |
| `finished_at` | `timestamptz` | Null while `running`. |
| `steps` | `jsonb` | An array; empty while `running`. |
| `chain_head` | `text` | 64 lowercase hex characters; null while `running`. |

A unique constraint on (`org_id`, `playbook_id`, `incident_id`) is the idempotency key: ADR-0021's pair (playbook, incident) with the org, which `incident_id` already determines. The function `playbook_executions_write_once` backs these triggers:

- **On insert**, it refuses a row that is not `running` with empty `steps` and null `finished_at` and `chain_head`.
- **On update**, it refuses a row whose old `status` is not `running`, one whose new `status` is still `running`, and a change to any column other than `status`, `finished_at`, `steps` and `chain_head`.
- **On delete**, and on `TRUNCATE` (a statement trigger), it always refuses.

Each element of `steps`:

| Field | Value |
|---|---|
| `step_id`, `action`, `mode` | As the playbook declares them. |
| `outcome` | `done`, `would_do`, `skipped` or `failed`. |
| `detail` | The text of §Operational §4, cut to 500 Unicode code points. |
| `started_at`, `finished_at` | ISO 8601 UTC with milliseconds, as `toISOString()` writes them. |
| `link` | 64 lowercase hex characters (§3). |

`status` ends `completed` when no step is `failed`, and `failed` otherwise.

### 3. The chain over the steps

SPEC-012's construction, over steps instead of events:

```text
link_0 = SHA-256( "" )
step_n.hash = SHA-256( JCS(step_n without "link") )
link_n = SHA-256( link_{n-1} ++ step_n.hash )      # raw 32-byte digests, no separator
chain_head = link_N
```

Each step stores its own `link_n`. The JCS canonicalisation is ingest's: `services/ingest/src/jcs.ts` gains an export of its `canonicalize`, which today only its envelope verification uses. The chain is unsigned (ADR-0021 §4). The code repeats the api's construction (`services/api/src/forensic/hashchain.ts`), and a known-answer test pins it (soar_ac_005).

### 4. The read route

`GET /v1/incidents/:id/playbook-executions`, in `services/api`. It sits behind SPEC-008's session preHandler, is open to all three roles as SPEC-009's reads are, and is read-only.

- **200:** `{ incident_id, executions: [...] }`.
  - The executions are ordered by `started_at`, then `execution_id`.
  - Each one carries `execution_id`, `playbook_id`, `playbook_version`, `status`, `started_at`, `finished_at` or null, `chain_head` or null, and its `steps` in their stored order.
  - An incident with no execution answers `executions: []`.
- **404 `{ error: "not_found" }`:** a malformed `:id`, an absent incident, or another org's incident. The org-scoped lookup of the incident comes first, as SPEC-010's step 1 does.
- **503 and 500:** as SPEC-009's Amendment 2026-10-14 states.

## Operational

### 1. Loading

`startIngest` loads the playbooks first, before it builds any client or migrates. A refusal therefore leaves nothing open (the class of debt #19). It does so at every boot, whether or not the detection driver runs, and logs through the boot's logger at the configured level.

- **The variable.** `INGEST_SOAR_PLAYBOOKS_DIR` is optional, with no Zod default, so the loader can tell set from unset. An empty value counts as unset (`blankToUndefined`, `services/ingest/src/config.ts`).
- **The directory.** When the variable is unset, the default resolves as the rules' directory does (`services/ingest/src/config.ts`): `<cwd>/playbooks` when present (in the image, `COPY playbooks ./playbooks`), otherwise the repository's `playbooks/`. The dev compose passes `INGEST_SOAR_PLAYBOOKS_DIR: ${CG_INGEST_SOAR_PLAYBOOKS_DIR:-/app/playbooks}`, and `infra/dev/.env.example` lists the variable.
- **The schema.** It resolves the same way: `<cwd>/schemas/playbooks/v0.1/playbook.json` when present (the image copies `schemas/playbooks/`), otherwise the repository's. It is compiled with ajv's draft 2020-12 class.
- **The files.** The loader reads `*.yml` and `*.yaml` files only, in name order; the README and any other file are ignored. It parses each one with `yaml`, validates it against the schema with `ajv`, and applies the policy:
  - no `live` `agent.isolate`;
  - unique playbook ids across the directory, and unique step ids in each playbook;
  - only the placeholders of §Data contracts §1.
- **A refusal.** A file that fails is refused, naming the file and the first error, and `startIngest` rejects. The process then exits, as it does when the rules cannot load.
- **An absent or empty directory.** Either leaves SOAR idle, as ADR-0021 §2 decides, and so does a directory with no YAML file. The boot logs `soar_idle` with the reason. When the variable is set and its directory does not exist, the line is at `warn`, so a mistyped path is visible; otherwise it is at `info`. An empty directory is how an operator turns SOAR off.
- **Once per boot.** Playbooks are read once per boot, never per cycle.

### 2. Trigger

`runDetectionCycle(config, notify?, soar?)` gains `soar` as a third optional carrier, separate from `DetectConfig` (SPEC-014's pattern). The production driver passes `services.soar` as it passes `services.notify`.

When `upsertIncident` reports a create, and after SPEC-014's notification, the cycle calls `runPlaybooks`. It runs each matching playbook in id order, one after another. In the MVP a playbook matches when both hold:

- its `trust_sources` holds `rule`, and the creating alert's `cg_detection_source` is `rule` (every MVP rule's is; `services/ingest/src/detect/engine.ts` refuses any other);
- the incident's `severity_id` at creation is at least `when.severity_id_gte`.

How `ml` and `hybrid_only` match is decided when ML produces alerts (ADR-0021 §3). Until then, a playbook whose `trust_sources` lacks `rule` never runs. `runPlaybooks` never throws, and the cycle awaits it.

### 3. One run

1. **Claim.** `INSERT … ON CONFLICT (org_id, playbook_id, incident_id) DO NOTHING RETURNING execution_id`.
   - If no row is returned, another run holds the claim. This run sends nothing and writes nothing, and logs `soar_run_skipped_claimed` at `debug`.
   - If the insert fails, no step runs, and the run logs `soar_run_claim_failed` at `warn`.
2. **Steps.** They run in order, each under its bound (§6). A `failed` step ends the run, and the steps after it are recorded `skipped`, with detail `an earlier step failed`.
3. **Complete.** One update sets `status`, `finished_at`, `steps` and `chain_head`. If that update fails, the row stays `running`, and the run logs `soar_run_record_failed` at `warn` and is not retried.
4. **Log.** The run logs `soar_run` at `info`, with `playbook_id`, `incident_id`, `execution_id`, `status`, and each step's id and outcome.

A process that dies after the claim leaves the row `running`, and nothing runs it again (ADR-0021 §4).

### 4. The actions

- **`agent.isolate`, `dry_run`:** `would_do`, with detail `would isolate agent <agent_id>; no command channel to the agent exists (ADR-0021 §Context 6)`. Nothing leaves the process.
- **`notify.email`, `live`:** one message through the bounded send (§6), from SPEC-014's sender to SPEC-014's recipient, with its rendered subject and body. The step uses the cycle's `notify` carrier.
  - Sent: `done`, with detail `sent`.
  - The cycle has no `notify` carrier, because notification is not configured: `skipped`, with detail `notification is not configured`. The run can still end `completed`.
  - The send fails: `failed`, with detail `send failed: <code>`. `<code>` is the error's code (for example `EAUTH`, `EENVELOPE`, `ECONNECTION`), with the server's numeric response code when it gave one, and `unknown` otherwise.
  - The send is cut by its bound: `failed`, with detail `timed out after <n> ms`. A send that the mailer rejects at its own time limit counts as cut, so a timeout always reads this way.
  - The detail never holds the error's message, which can quote the recipient.
- **`notify.email`, `dry_run`:** `would_do`, with detail `would send: <rendered subject>`.

Every detail is cut to 500 Unicode code points.

### 5. The shipped playbook

`playbooks/pb_high_severity_containment.yml`:

- `id: pb_high_severity_containment`, `version: 1`, `name: "Contain the host of a high-severity incident (dry-run)"`, `trust_sources: ["rule"]`.
- `trigger: { on: incident.created, when: { severity_id_gte: 4 } }`.
- Step `isolate`: `agent.isolate`, `dry_run`.
- Step `notify`: `notify.email`, `live`, with subject `[CyberGuard] Containment pending: {{incident.title}}`. Its body names the incident, its severity and its agent, says that containment awaits an analyst, and lists `{{steps}}`.

### 6. Bounds

- At most 10 steps per playbook (the schema).
- A step has at most 15 000 ms, and a run at most 30 000 ms. The tests may inject shorter bounds; these are the defaults.
  - A step cut by its own bound is `failed`, with detail `timed out after 15000 ms`.
  - A step cut by the run's bound is `failed`, with detail `run timed out after 30000 ms`.
  - Either way, the steps after it are `skipped`.
- **The bounded send.** The mailer's interface takes the message and its time limit, which is the step's bound or the run's remaining time, whichever is shorter.
  - It composes the message with nodemailer's `MailComposer` (`nodemailer/lib/mail-composer`).
  - It opens its own connection with nodemailer's `SMTPConnection` (`nodemailer/lib/smtp-connection`): connection timeout 5000 ms, greeting timeout 5000 ms, socket timeout 10 000 ms.
  - Its TLS is SPEC-014's: implicit TLS on port 465, otherwise STARTTLS when the server offers it, as the current transport does (`services/ingest/src/notify/transport.ts`).
  - It authenticates with SPEC-014's credentials, sends and quits.
  - At the time limit it closes that connection and rejects, so no send outlives its step.
  - A test's spy mailer implements the same interface.
- **SPEC-014's notification** uses the same bounded send, with a limit of 15 000 ms. Its failure log carries the error's code, as the step's detail does, and not its message. This amends SPEC-014 by scope: its transport stays generic SMTP (its §Compliance), and its acceptance criteria are unchanged.

### 7. Tests and the marquees

The test backends' Config (`services/ingest/test/helpers/backends.ts`) sets `INGEST_SOAR_PLAYBOOKS_DIR` to an empty directory. The marquees that boot `startIngest` with the production driver on therefore run with SOAR idle, as they run with notification unset. `detect_ac_001` calls the cycle with no carrier. A `soar_ac_*` test builds its own carrier from its own directory.

### 8. CI

- `ts-ci`'s path filters gain `playbooks/**` and `schemas/playbooks/**`, so a change to a playbook or its schema runs soar_ac_001. That test loads the repository's `playbooks/` through the loader, schema and policy both. This is the CI validation ADR-0021 §2 decides.
- `.github/workflows/schema-validation.yml`'s path filter gains `playbooks/**`. The workflow gains a step that validates `playbooks/*.yml` and `playbooks/*.yaml` against the schema with `ajv-cli`, and `task validate-schemas` gains the same step. `ajv-cli` reads YAML data files. ADR-0021 §2's remark that it reads JSON is wrong, which is debt #73 (handoff-38). Its YAML parser (js-yaml) and the loader's (`yaml`) read a playbook's fields the same way, since none holds a date or another value the two parse differently.

## Non-functional requirements

- **NFR-022-001 (bounds).** At most 10 steps, 15 s per step and 30 s per run, as §Operational §6 states.
- **NFR-022-002 (detection delay).** A cycle that creates N incidents, each matching M playbooks, can be delayed by up to N × (15 + M × 30) s: SPEC-014's send, then each run's steps. Each run's claim and completion writes come on top of that.
- **NFR-022-003 (no secret, no address).** No record field and no log line holds an SMTP credential or the configured recipient's address. A failed send is reported by the error's code only. An address an operator writes into a playbook's own text is that operator's choice.

## Acceptance criteria

Each criterion maps to a test under `services/ingest/test/` (`soar-ac-NNN-*.test.ts`) or `services/api/test/`. All are CI-able: they use testcontainers' Postgres and ClickHouse, the synthetic-event path of the `detect_ac_*` and `notify_ac_*` suites, and spy mailers or local listeners, with no network beyond the host.

- **soar_ac_001 (loading).**
  - The repository's playbooks load.
  - The loader refuses each of these, naming the file:
    - an unknown field;
    - 11 steps;
    - a malformed id;
    - `params` on `agent.isolate`;
    - a `live` `agent.isolate`;
    - an unknown action;
    - a duplicate playbook id;
    - a duplicate step id;
    - an unknown placeholder;
    - `{{steps}}` in a subject;
    - text that is not YAML.
  - `startIngest` rejects with such a file in its directory, before it opens any client.
  - An empty directory leaves SOAR idle, and the boot logs `soar_idle` at `info`. A set directory that does not exist leaves it idle too, logged at `warn`.
  - A README in the directory is ignored.
- **soar_ac_002 (trigger).** Through `runDetectionCycle`:
  - an incident created by `office_spawns_script_host` (severity 4) runs the shipped playbook once;
  - an incident created by a medium rule (severity 3) runs none;
  - a playbook whose `trust_sources` lacks `rule` does not run;
  - an alert that joins an existing incident runs nothing.
- **soar_ac_003 (dry-run and notification).**
  - The record holds `isolate` as `would_do`, its detail naming the agent, and `notify` as `done`.
  - The spy mailer received the playbook's email, whose body holds the `isolate` line, and SPEC-014's own email: two emails.
  - Called directly with a title holding a line break (no rule's title has one), `runPlaybooks` sends a subject with a space instead.
  - With no `notify` carrier, `notify` is `skipped` and the run ends `completed`.
- **soar_ac_004 (claim and write once).**
  - With a row already claimed for the pair, a run sends nothing and writes nothing.
  - While its `notify` step sends, the row reads `running`.
  - Postgres refuses each of these:
    - updating or deleting a completed row;
    - an update whose new `status` is still `running`;
    - an update that changes `playbook_id`;
    - an insert that is already `completed`;
    - a `TRUNCATE`.
- **soar_ac_005 (chain).**
  - A fixed list of steps gives a fixed `chain_head` (a known answer, computed once with the api's `hashchain.ts`).
  - Recomputing a stored record's chain from its steps gives its `chain_head`.
  - Changing one step's `detail` changes the head.
- **soar_ac_006 (bounds and the cycle).**
  - With a step bound of 200 ms, a run bound of 5000 ms, and a mailer that never answers, `notify` is `failed` with exactly `timed out after 200 ms`. The steps after it are `skipped`, and the run is `failed`. The incident persists and the cycle returns.
  - With a step bound of 5000 ms and a run bound of 300 ms, a run of slow steps is cut with `run timed out after 300 ms`.
  - Against a local listener that never sends a greeting, the bounded send fails within the greeting timeout, and so does SPEC-014's notification.
- **soar_ac_007 (read route).** `services/api/test/soar-ac-007-read.test.ts`:
  - a session of the incident's org reads its executions;
  - another org's `:id`, an absent one or a malformed one answers `404`;
  - no session answers `401`;
  - a Postgres that cannot answer gives `503`;
  - an incident with no execution answers `executions: []`.
- **soar_ac_008 (no secret, no address).** A run with a configured mailer is made to succeed once, and once to fail with an error whose message holds the recipient's address. In both cases neither the record nor the captured log lines hold the SMTP password or the recipient's address.

The change touches `services/ingest/src/detect/index.ts`, so it also runs the elevated gate (CLAUDE.md, *Developer-local SPEC-006 marquee validation* §6).

## Test scenarios

- **SC-SOAR-001 — a High incident runs the playbook.** soar_ac_002, soar_ac_003.
- **SC-SOAR-002 — no live containment.** soar_ac_001 (refused at load), soar_ac_003 (`would_do`).
- **SC-SOAR-003 — once per incident, written once.** soar_ac_004.
- **SC-SOAR-004 — a stalled relay does not stall detection beyond the bound.** soar_ac_006.

## Risks

| Risk | Mitigation |
|---|---|
| A slow relay delays detection | The bounds of §Operational §6; NFR-022-002 states the worst case; ADR-0021 A7 defers a queue. |
| A longer shutdown | The driver's `stop()` waits for the cycle in flight (`services/ingest/src/detect/driver.ts`), which SOAR can lengthen up to NFR-022-002's bound. |
| Two emails per High incident | Their purposes differ: SPEC-014's announces, the playbook's asks for containment. The playbook can be removed from the directory. |
| A run lost to a crash after the claim | The row stays `running`, visible in the read route. |
| The chain code drifts from SPEC-012's construction | soar_ac_005's known answer, computed once with the api's `hashchain.ts`, which applies that construction. |

## Open questions

1. **Retention of execution records** — ADR-0021's Open question 1, unchanged.

## Decision record

Claude Code's decisions inside ADR-0021, each with the alternative it discards:

1. **A closed set of placeholders**, refused at load when unknown; `{{steps}}` in the body only. Alternative: a template language, which would be code an operator can write.
2. **One SMTP connection per send, closed at its bound**, through nodemailer's `MailComposer` and `SMTPConnection`. Alternative: nodemailer's transport, whose `close()` leaves a busy connection open, so a send could outlive its step.
3. **A failed send reported by its code.** Alternative: the error's message, which can quote the recipient.
4. **Write-once by Postgres triggers**, on insert, update, delete and truncate. Alternative: application discipline only.
5. **Loading before any client is built, and a set but absent directory logged at `warn`.** Alternatives: loading in `buildServices`, which leaves clients open on a refusal; refusing the boot on that directory, which ADR-0021 §2 decides against (an absent directory leaves SOAR idle).
6. **The chain code repeated in ingest, pinned by a known answer.** Alternative: a shared package, which may be a new component (debt #7, Manuel's).
7. **The read route nested under the incident**, as the drill, the export and the report are. Alternative: a top-level `/v1/playbook-executions` list, which no MVP view needs.
8. **Both emails kept.** Alternative: suppress SPEC-014's email when a playbook mails, which couples the two carriers.

## Amendment 2026-10-14: the bounded send authenticates when offered, and parses its envelope

**What surfaced it.** The fresh-context review of the implementation, before it landed. §Operational §6 says the bounded send "authenticates with SPEC-014's credentials, sends and quits". Read literally, a send authenticates on every connection. With a server that advertises no AUTH, nodemailer's `SMTPConnection` then falls back to PLAIN and sends the credentials: in clear text when the server is not on port 465 and offers no STARTTLS. SPEC-014's nodemailer transport never did that. It logged in only when the server advertised AUTH, and it took the envelope (sender and recipients) from the composed message's headers, so a sender written with a display name worked.

**The amendment.** §Operational §6's bullet reads: it authenticates with SPEC-014's credentials when the server advertises AUTH, as SPEC-014's transport did, and otherwise sends without authenticating. It takes the envelope from the composed message's headers, as that transport did. It then sends and quits. Its TLS is unchanged: implicit TLS on port 465, otherwise STARTTLS when the server offers it.

**Effect.** soar_ac_006 gains two cases, against a local SMTP server that records the commands it receives:

- a send authenticates when the server advertises AUTH, and does not when it does not;
- a sender with a display name gives the bare address in `MAIL FROM`.

SPEC-014's notification, which uses the same send, keeps the behaviour its transport had. No other section changes. Status stays **Accepted**.

## References

- [ADR-0021](../adr/0021-soar-mvp-playbook-executor.md) — the decisions implemented here.
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — the action policy and `trust_sources`.
- [ADR-0017](../adr/0017-incident-email-notification.md) and [SPEC-014](SPEC-014-incident-notification.md) — the seam and the SMTP transport.
- [SPEC-012](SPEC-012-forensic-evidence-hashchain.md) — the chain construction.
- [SPEC-009](SPEC-009-read-slice.md) — the read API's error semantics (Amendment 2026-10-14), and its §Out of scope deferral of playbook executions.
- [Threat model](../security/threat-model.md) — the Amendment 2026-10-14 (ADR-0021).
- `services/ingest/src/detect/index.ts`, `services/ingest/src/notify/`, `services/api/src/read/routes.ts` — where the code attaches.
