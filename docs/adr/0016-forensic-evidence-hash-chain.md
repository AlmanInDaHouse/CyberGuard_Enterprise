# ADR-0016: Forensic evidence hash-chain — per-event SHA-256 chain over the canonicalized drill output, dedicated Ed25519 root signature

- Status: Proposed
- Date: 2026-06-06
- Last updated: 2026-06-06
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

The product promise is *"… an exportable forensic report on the first incident"* (`docs/product/blueprint.md:33`), and the blueprint specifies an **evidence hash-chain** to make that report *"auditable"* — a Merkle-style per-evidence SHA-256 chain with an Ed25519 root signature (`docs/product/blueprint.md:527-535`):

```text
evidence_n.hash = SHA-256(evidence_n.content)
chain_n         = SHA-256(chain_{n-1} || evidence_n.hash || timestamp_n)
root_signature  = Ed25519_sign(server_key, chain_N)
```

A READ-ONLY design audit (this session) established the repo-grounded facts that constrain the model, **before** its implementing SPEC specifies the mechanics:

- **"Auditable" is not yet honest in code.** No hashing of stored content exists; tamper-evidence rests only on append-only tables + the agent's transport-layer Ed25519 signing (verified server-side, then **discarded, never persisted**). Forensic escalón 1 (drill / timeline — SPEC-010, ADR-0015) and escalón 2 (incident severity — SPEC-011) are delivered; this is **escalón 3**.
- **The drill output is a deterministic content set, decoupled from storage non-determinism.** The SPEC-010 drill resolves an incident → its alerts' `source_events` → the raw `cges_events`, reading ClickHouse with `FINAL` (`services/api/src/read/queries.ts:259`) — which collapses the `ReplacingMergeTree(arrived_at)` at-least-once duplicates — and de-duplicating the event ids in TS with a `Set` (`services/api/src/read/queries.ts:251`). So the **set of rows for an incident is deterministic in content**, independent of ClickHouse merge timing.
- **But the drill's row ORDER is not yet a total order.** The drill query is `ORDER BY time ASC` with **no secondary tiebreaker** (`services/api/src/read/queries.ts:262`): events sharing the same `time` have a non-deterministic relative order. A reproducible chain needs a total order — this ADR therefore requires the co-located **SPEC-010 amendment** (in this same diff) that adds `event_id` as the tiebreaker.
- **No incident-closure lifecycle exists.** The `incidents` table has a `status` column with terminal-ish enum values (`services/ingest/src/db/migrations/0005_incidents.ts:32-33`, `… 'resolved', 'false_positive'`), but there is **no embarked production transition** that sets it (the detection upsert preserves `status`; the api is read-only) and **no `closed_at` column** (AUSENTE, confirmed). So an *"on incident close"* trigger has nothing to bind to.
- **The only server-side Ed25519 key is the ingest CA.** `services/ingest/src/ca.ts:12` (`ALG = { name: "Ed25519" }`) generates a self-signed Ed25519 root, stored in the single-row `ca` table (`private_key bytea NOT NULL`, `services/ingest/src/db/migrations/0001_initial.ts:10,13`), **encrypted at rest** with pgcrypto `pgp_sym_encrypt(…, passphrase)` (`services/ingest/src/ca.ts:85-88`). It lives in **`services/ingest`** (the agent-mTLS boundary) and exists to **sign X.509 certs** (`services/ingest/src/cert.ts:53,105`). The forensic read runs in **`services/api`** (the human boundary, ADR-0014 §3), which has no Ed25519 key today.
- **Threat-model boundary — tamper-evidence is *relative to the event store at snapshot time*, under a no-rewrite assumption.** The chain attests that the evidence is *what the system resolved for this incident at snapshot time*; that guarantee assumes a given `event_id` is not re-written after ingest. `cges_events` is a `ReplacingMergeTree(arrived_at)`, so `FINAL` (`queries.ts:259`) + the TS `Set` dedup (`queries.ts:251`) give **read-determinism today** — but a **re-ingest of the same `event_id` with a later `arrived_at`** would change the drill's content, and therefore its hash, by **RMT mechanics, not by malicious tampering**. The hash-chain still detects after-the-fact alteration of a *captured* snapshot; what it does **not** by itself guarantee is **write-once immutability of the underlying store**. That **strong (write-once) immutability is left as an OPEN QUESTION** for the implementing SPEC or a later forensic escalón — this ADR does not resolve it.

This ADR fixes the **chain model + the evidence definition** (escalón 3 recorte parts (a) + (c)). It deliberately excludes **(b) physical at-rest / MinIO persistence** of evidence and reports (see §Out of scope).

## Decision

### 1. The unit of evidence is the canonicalized drill output, not raw storage rows

The evidence is the **SPEC-010 drill output** — `EventTimeline = { incident_id, events: TimelineEvent[] }` (`services/api/src/read/types.ts:57-67` + the runtime shape constructed at `services/api/src/read/queries.ts:268`, empty-case `:252`) — **canonicalized**, NOT the raw ClickHouse rows.

**Why the drill output, not raw rows:** the drill already produces a **deterministic content set** decoupled from `ReplacingMergeTree` non-determinism (`FINAL` at `queries.ts:259` + the TS `Set` dedup at `queries.ts:251`). Hashing the canonicalized drill projection means the evidence is **reproducible from the same read** and **survives ClickHouse merges** — a hash over raw storage rows would couple the evidence to merge timing and duplicate-collapse state. The drill is also the artifact a human/auditor actually sees (the timeline), so the hashed thing *is* the presented evidence.

### 2. Canonicalization = total order `(time, event_id)` + JCS-style serialization

- **Total order.** The evidence events are ordered by `(time ASC, event_id ASC)` — `event_id` is the secondary tiebreaker that makes the order total. The SPEC-010 drill is amended to carry this order (the co-located amendment in this diff records the contract change; the `queries.ts` edit lands with the implementing SPEC).
- **Serialization reuses SPEC-003's JCS discipline.** Each evidence piece is serialized to the **JCS** (RFC 8785) canonical form before hashing — the same canonicalization discipline already used for the agent envelope signature (SPEC-003; `services/ingest/src/jcs.ts:12,15`). No new canonical format is invented. JCS gives byte-for-byte reproducibility across runs and languages.

### 3. Per-event chain; `n` = one timeline event; root signature over the head

The chain follows the blueprint scheme verbatim (`blueprint.md:527-535`), with **`n` indexing the timeline events in the total order of §2**:

- `evidence_n` = the `n`-th `TimelineEvent` (canonicalized per §2).
- `evidence_n.hash = SHA-256(canonical(evidence_n))`.
- `chain_n = SHA-256(chain_{n-1} || evidence_n.hash || timestamp_n)` — the chain **accumulates per event**.
- `root_signature = Ed25519_sign(forensic_key, chain_N)` — signed over the head `chain_N` (the last accumulated link).

The genesis (`chain_0`) seed and the exact `||` concatenation/encoding of `timestamp_n` are **mechanics for the implementing SPEC**, not re-fixed here; this ADR fixes the recurrence and that `n` is per-event.

### 4. Trigger = on-demand forensic snapshot, NOT on incident close (documented blueprint deviation)

The chain is computed **on demand**, when a forensic snapshot is requested — **not** on incident close.

This is a **documented deviation from the blueprint**: `blueprint.md:535` frames tamper-evidence as *"… since the incident was **closed**."* That phrasing is superseded here by *"… as of **snapshot time**."* **Rationale (repo-grounded):** there is no incident-closure lifecycle in production — `status` exists (`0005_incidents.ts:32-33`) but has no embarked transition and there is no `closed_at` (AUSENTE, §Context). An on-close trigger has nothing to fire on. The **snapshot is the tamper-evidence boundary**: the chain attests *"this is the evidence as the system resolved it at snapshot time"*, which an append-only event store makes meaningful regardless of incident lifecycle.

### 5. Root signature uses a DEDICATED forensic Ed25519 key in `services/api`, never the ingest CA

A **new, dedicated Ed25519 keypair** signs `chain_N`, **owned by `services/api`** (where the forensic read runs):

- **Not** the ingest CA key (`services/ingest/src/ca.ts`). The CA key exists to **issue X.509 certs** in the **agent-mTLS** boundary; evidence attestation is a **different purpose** in the **human** boundary (ADR-0014 §3). Reusing the CA key would conflate two purposes and **cross a trust boundary** the threat model keeps separate.
- **Key-at-rest mirrors the `ca.ts` pgcrypto pattern.** The forensic private key is stored encrypted in the **api Postgres** via `pgp_sym_encrypt(…, passphrase)`, single-row table, decrypt-on-load — a 1:1 mirror of the ingest CA persistence (`services/ingest/src/ca.ts:85-88`, `:101`). The api gains its **first** signing key; it is not the CA.

### 6. Verification + auditor-facing public-key exposure

Verification (the operational meaning of *"auditable"*): re-fetch the drill, **re-canonicalize** (total order + JCS, §2), **recompute** the chain (§3), and **Ed25519-verify** `chain_N` against the **forensic public key**.

The forensic **public key is published for external auditors** — the report (and/or a public read endpoint) embeds the `forensic_pubkey` + `root_signature` so a third party can verify tamper-evidence **without trusting the server**. This ADR fixes that the pubkey is exposed; the implementing SPEC fixes the exact surface (endpoint vs. embedded-in-report).

## Alternatives considered

- **Hash the raw ClickHouse rows.** Rejected: non-deterministic without canonicalization, and couples the evidence to `ReplacingMergeTree` merge/duplicate state — the opposite of the `FINAL`+`Set` determinism the drill already buys (`queries.ts:251,259`).
- **Reuse the ingest CA Ed25519 key for `root_signature`.** Rejected: conflates cert-issuance with evidence-attestation and crosses the ADR-0014 human/agent boundary (the CA lives in `services/ingest`; the forensic read lives in `services/api`).
- **Trigger on incident close.** Rejected: no closure lifecycle exists (no transition, no `closed_at`; §Context). On-demand snapshot is the available, well-defined boundary.

## Consequences

### Positive

- **Makes *"auditable"* honest in code.** A third party can verify tamper-evidence with the published forensic public key — the blueprint scheme (`:527-535`) is finally consumed.
- **Deterministic and storage-decoupled.** Hashing the canonicalized drill output (total order + JCS) over the `FINAL`/`Set`-deterministic content set yields reproducible evidence independent of ClickHouse merges.
- **Reuses existing disciplines.** JCS canonicalization (SPEC-003 / `jcs.ts`); pgcrypto key-at-rest (`ca.ts:85-88`). No new canonical format and no new secret-storage primitive are invented.

### Negative

- **New key material + a new at-rest secret in `services/api`** (the forensic Ed25519 key) — a new operational surface and a passphrase to manage, mirroring the ingest CA's.
- **The SPEC-010 drill contract changes** — the row order gains an `event_id` tiebreaker (the co-located amendment). This is an **observable change to row order** for same-`time` events, even though the response **shape is unchanged**.

### Neutral

- **Does NOT implement physical at-rest / MinIO persistence** of evidence or reports (escalón 3 recorte part (b), explicitly excluded). ADR-0003's MinIO home is **untouched** (it remains zero-consumer).
- **Does NOT add an incident-closure lifecycle** (`closed_at`, status transitions) — the on-demand snapshot replaces the on-close trigger.
- **Does NOT render the report** (PDF / HTML); that remains a later forensic step.

## Compliance

- The canonical form MUST be a **total order `(time, event_id)`** + **JCS (RFC 8785)** serialization — byte-for-byte reproducible. No ad-hoc serialization.
- The forensic signing key MUST be a **dedicated `services/api`-owned Ed25519 key**, encrypted at rest with pgcrypto, and MUST NOT be the ingest CA key.
- The **forensic public key MUST be exposed** for external verification (the auditor-facing half of *"auditable"*).
- The chain MUST follow the `blueprint.md:527-535` recurrence (per-event SHA-256, chained with `timestamp_n`, Ed25519 over `chain_N`).
- The api MUST NOT reach into `services/ingest` for key material or signing (ADR-0014 boundary).

## Out of scope

Each deferred item names its destination:

- **(b) Physical at-rest / MinIO persistence** of evidence blobs and rendered reports — ADR-0003 homes these in MinIO (zero consumer today). A later forensic-persistence increment.
- **PDF / HTML render** of the report (timeline + MITRE + severity + the chain/signature) — a later forensic-render SPEC.
- **Incident-closure lifecycle** (`closed_at`, embarked status transitions, triage-writes) — orthogonal; the on-demand snapshot does not require it.
- **The exact mechanics** — endpoint(s), `chain_0` genesis seed, `timestamp_n` encoding, the forensic-key table schema, and the pubkey-exposure surface — belong to this ADR's **implementing SPEC** (forthcoming).

## Landing checklist (atomic on flip to Accepted)

When this ADR is ratified Proposed→Accepted, the same commit:

1. Flips the status header to `Accepted`.
2. Adds the catalog row to `docs/adr/README.md`.
3. Adds these dependency edges to `docs/adr/README.md` §Dependencies:
   - `ADR-0016 → ADR-0014` (introduces a dedicated forensic signing key in the **human** boundary; does NOT reuse the ingest CA, preserving the trust-boundary split).
   - `ADR-0016 → ADR-0003` (consumes the evidence definition; explicitly does **not** use the MinIO at-rest home — recorte part (b)).
   - `ADR-0016 → SPEC-010` (the drill output is the evidence unit; requires its `(time, event_id)` total-order amendment).
   - `ADR-0016 → SPEC-011` / `SPEC-007` (the incident the evidence is scoped to: its grouped alerts and aggregated severity).
   - `ADR-0016 → SPEC-003` (reuses the JCS canonicalization discipline; does not amend it).

## References

- [Blueprint](../product/blueprint.md) — `:33` (the *"auditable forensic report"* promise), `:527-535` (the evidence hash-chain scheme this ADR implements; `:535` the *"since the incident was closed"* phrasing this ADR supersedes with *"as of snapshot time"*).
- [SPEC-010](../specs/SPEC-010-forensic-event-drill.md) / [ADR-0015](0015-readonly-clickhouse-reader-in-api.md) — escalón 1, the drill whose canonicalized output is the evidence unit; the co-located total-order amendment.
- [SPEC-011](../specs/SPEC-011-incident-severity.md) / [SPEC-007](../specs/SPEC-007-incident-grouping-mvp.md) — escalón 2 + incident grouping, the incident scope of the evidence.
- [SPEC-003](../specs/SPEC-003-mtls-signed-envelope.md) — the JCS canonicalization + Ed25519 signing discipline reused (`services/ingest/src/jcs.ts:12,15`).
- [ADR-0014](0014-human-authentication-model.md) — the human/agent boundary this ADR preserves by NOT reusing the ingest CA key for forensic signing.
- [ADR-0003](0003-polyglot-storage.md) — homes forensic artifacts in MinIO; explicitly **not** consumed for at-rest here (recorte part (b)).
- `services/ingest/src/ca.ts:12` (the ingest CA Ed25519, NOT reused) / `:85-88` (the pgcrypto key-at-rest pattern the forensic key mirrors); `services/ingest/src/db/migrations/0001_initial.ts:10,13` (the `ca` table / encrypted `private_key` precedent).
- `services/api/src/read/queries.ts:251` (TS `Set` dedup), `:259` (`FINAL`), `:262` (the `time`-only order amended to a total order); `types.ts:57-67` (`TimelineEvent` — the evidence shape).
- `services/ingest/src/db/migrations/0005_incidents.ts:32-33` (`status` exists; no closure transition / `closed_at` → the on-demand-snapshot rationale).
