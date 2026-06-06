# SPEC-012: Forensic evidence hash-chain — per-event SHA-256 chain + dedicated Ed25519 root signature

- **ID:** SPEC-012
- **Title:** Forensic evidence hash-chain over the canonicalized drill output (escalón 3 — implements ADR-0016)
- **Status:** Accepted
- **Depends on:** ADR-0016 (the model this SPEC implements — evidence unit = canonicalized drill output, per-event chain, on-demand snapshot, dedicated api Ed25519 key, JCS discipline); SPEC-010 (the drill output `EventTimeline` that is the evidence, and its `(time, event_id)` total-order amendment — `services/api/src/read/queries.ts:268`, `:252`, `:262`, `:259`, `:251`); SPEC-011 / SPEC-007 (the incident the evidence is scoped to — its grouped alerts + aggregated severity); SPEC-003 (the JCS / RFC 8785 canonicalization + Ed25519 signing discipline reused, `services/ingest/src/jcs.ts`).
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0016 fixed the **model** for forensic evidence integrity (escalón 3 of the *"auditable forensic report"* promise, `docs/product/blueprint.md:33`): a per-event SHA-256 hash-chain over the **canonicalized output of the SPEC-010 drill** (`EventTimeline`), with an **Ed25519 root signature** under a **dedicated `services/api` key** (never the ingest CA). Escalones 1 (drill / timeline) and 2 (incident severity) are delivered; this SPEC is the **implementer** of escalón 3, scope recorte **(a) chain model + (c) evidence definition**, explicitly **NOT (b) MinIO / physical at-rest**.

This SPEC fixes the **mechanics** ADR-0016 deferred to its implementing SPEC: the genesis seed, the chain concatenation, the timestamp treatment, the canonicalization library wiring, the forensic-key table shape, and the export/verify contract. It does **not** re-debate the model (ratified in ADR-0016).

The evidence is deterministic in content by construction: the drill reads ClickHouse with `FINAL` (`queries.ts:259`) + de-duplicates event ids with a TS `Set` (`queries.ts:251`), and SPEC-010's amendment pins a **total order `(time, event_id)`** (`queries.ts:262`). The threat boundary is the one ADR-0016 §Context recorded: tamper-evidence is **relative to the event store at snapshot time, under a no-rewrite assumption** (a `ReplacingMergeTree` re-ingest of the same `event_id` with a later `arrived_at` would change the hash by RMT mechanics, not malice; strong write-once immutability is out of scope here).

## Scope

### In scope

1. **The chain computation** over a snapshot's canonicalized evidence (the math in §Data contracts §2).
2. **Canonicalization wiring in `services/api`** — adopt the `canonicalize` npm package (RFC 8785 / JCS, `^2.0.0`, the same version `services/ingest` already depends on, `services/ingest/package.json:28`) as an **api dependency** plus a **thin local wrapper**. The api does **NOT** import `services/ingest/src/jcs.ts` (ingest-coupled; `@cyberguard/api` does not depend on `@cyberguard/ingest`, which exports no entry point). **No shared canonicalization package is extracted in this session** (see §Out of scope).
3. **The dedicated forensic Ed25519 key** in `services/api`, **encrypted at rest with pgcrypto** — a single-row table mirroring the ingest `ca` precedent (`services/ingest/src/db/migrations/0001_initial.ts:10-16`; `services/ingest/src/ca.ts:85-88`, `:101`). pgcrypto is already enabled in the api Postgres (`services/api/src/db/migrations/0001_users.ts:15`).
4. **The on-demand snapshot export** — the canonicalized evidence + the chain root + the Ed25519 `root_signature` + the **embedded** forensic public key (§Data contracts §3).
5. **External verification** — recompute the chain from the exported evidence and Ed25519-verify `chain_N` against a given public key (§Data contracts §4).
6. **Acceptance criteria** (§AC) as **CI-able** integration tests (the throwaway-DB / real-backend pattern of `incident_severity_ac_004`; **no marquee, no `skipIf(win32)`**).

### Out of scope

Each names its destination:

- **(b) Physical at-rest / MinIO persistence** of the export — ADR-0003's MinIO home (zero consumer). A later forensic-persistence increment.
- **PDF / HTML render** of the report — a later forensic-render SPEC.
- **Incident-closure lifecycle** (`closed_at`, status transitions) — the on-demand snapshot replaces the on-close trigger (ADR-0016 §4).
- **Extracting a shared canonicalization package** (so api and ingest share one JCS module instead of each depending on `canonicalize`) — a later refactor; this SPEC duplicates the thin wrapper deliberately.
- **Out-of-band trust anchoring of the forensic public key** (resistance to a *compromised server*) — a deployment-contract decision, **deferred to §Open questions** below. Until it is settled, *"auditable"* in this SPEC means **integrity verifiable under a trusted key**, not authenticity against a compromised server.

## Data contracts

### 1. The evidence unit

`evidence_n` is the **`n`-th `TimelineEvent`** of the SPEC-010 drill output (`services/api/src/read/types.ts:57-67`), in the **total order `(time ASC, event_id ASC)`** (SPEC-010 Amendment 2026-06-06). `n` indexes the events; the chain **accumulates per event** (ADR-0016 §3). `EventTimeline = { incident_id, events: TimelineEvent[] }` is constructed at `queries.ts:268` (empty-case `:252`).

`canonical(evidence_n)` is the **JCS (RFC 8785)** serialization of `evidence_n` via the `canonicalize` package — byte-for-byte reproducible.

### 2. The chain (mechanics fixed here)

```text
evidence_n.hash = SHA-256( UTF-8 bytes of canonical(evidence_n) )      # 32 bytes
chain_0         = SHA-256( "" )                                         # SHA-256 of the empty input
chain_n         = SHA-256( chain_{n-1} ++ evidence_n.hash )             # 32-byte digests concatenated, NO separator
root_signature  = Ed25519_sign( forensic_key, chain_N )                # signed over the 32 bytes of chain_N
```

- **`chain_0 = SHA-256("")`** — a fixed genesis seed (the SHA-256 of the empty byte string), independent of the incident.
- **Concatenation is over the raw 32 bytes** of each digest (`chain_{n-1}` then `evidence_n.hash`), with **no delimiter**.
- **`timestamp_n` is NOT a separate chain term.** This **refines** the blueprint recurrence (`blueprint.md:531`, `chain_n = SHA-256(chain_{n-1} || evidence_n.hash || timestamp_n)`): the event's occurrence time (`event_time`) already lives **inside `canonical(evidence_n)`** (it is a field of `TimelineEvent`), so it is already bound into `evidence_n.hash`. Adding a separate `timestamp_n` would double-bind the same fact; this SPEC folds it into the evidence. (Documented deviation from blueprint mechanics; the *intent* — chaining time-ordered evidence — is preserved.)
- `chain_N` (the head, after the last event) is the signed root. For an incident whose drill resolves to **zero** events, `N = 0` and the root is `chain_0` (a well-defined empty-evidence root).

### 3. The snapshot export

The on-demand export carries everything an external verifier needs, with the forensic **public key embedded for convenience**:

- the `incident_id`;
- the **canonicalized evidence** — the `events: TimelineEvent[]` in the total order (the same `EventTimeline` shape, unchanged by this SPEC);
- the **chain root** `chain_N` (hex);
- the **`root_signature`** (Ed25519 over the 32 bytes of `chain_N`, base64url);
- the **embedded `forensic_pubkey`** (the raw 32-byte Ed25519 public key, base64url).

The exact endpoint surface and wire field names are the implementation's (ADR-0016 deferred "the pubkey-exposure surface"); this SPEC fixes the **contents** above. The forensic **private** key never appears in the export or any response.

### 4. The forensic key, at rest

A dedicated Ed25519 keypair owned by `services/api`, generated on first use, stored in a **single-row** api Postgres table (planned migration `0003_forensic_key`, a later gate), private key **encrypted with `pgp_sym_encrypt(…, passphrase)`** and read back with `pgp_sym_decrypt` — a 1:1 mirror of the ingest CA persistence (`ca.ts:85-88`, `:101`; the `id smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1)` + `bytea` shape of `0001_initial.ts:10-16`). The api MUST NOT reach into `services/ingest` for key material (ADR-0014 boundary; ADR-0016 §5).

### 5. Verification

Given an export and a **trusted** public key: re-serialize each `evidence_n` to JCS, recompute `evidence_n.hash`, recompute the chain (§2), and **Ed25519-verify** the exported `root_signature` against `chain_N` under the trusted key. Verification succeeds iff the recomputed `chain_N` matches and the signature verifies. (Trust in the key is an external precondition — see §Open questions / §Compliance.)

## Acceptance criteria

Each AC maps to a **CI-able** integration test under `services/api/test/` (the real-backend / throwaway-DB pattern of `incident_severity_ac_004`; **not** a marquee — no `skipIf(process.platform !== "win32")`).

- **hashchain_ac_001 (integrity — recompute + verify).** `services/api/test/hashchain-ac-001-integrity.test.ts`. For an incident with a known drill, recompute the chain from the canonicalized evidence and **Ed25519-verify `chain_N`** against the exported (or supplied) forensic public key → verification passes.
- **hashchain_ac_002 (canonicalization determinism).** `hashchain-ac-002-canonical-determinism.test.ts`. The JCS serialization of an `evidence_n` is **byte-for-byte stable** across repeated runs / key orderings of the input object → identical `evidence_n.hash`.
- **hashchain_ac_003 (total order is load-bearing).** `hashchain-ac-003-total-order.test.ts`. Two events with **identical `time`** produce the **same chain** because the `(time, event_id)` tiebreaker pins their order — justifying the already-landed SPEC-010 amendment. Removing the tiebreaker would make the chain order-dependent; the test pins the deterministic outcome.
- **hashchain_ac_004 (tamper-evidence).** `hashchain-ac-004-tamper.test.ts`. **Mutating a single byte** of any `evidence_n` (or reordering, or dropping/adding an event) makes recompute-and-verify **fail**.
- **hashchain_ac_005 (forensic key at rest — round-trip).** `hashchain-ac-005-key-at-rest.test.ts`. The forensic private key **round-trips** encrypt → decrypt → sign via pgcrypto; the stored column is ciphertext (**never plaintext**); a wrong passphrase cannot decrypt.
- **hashchain_ac_006 (embedded pubkey verifiable; private never crosses the boundary).** `hashchain-ac-006-export-pubkey.test.ts`. The export **embeds the forensic public key**, and a verifier using the embedded key validates `root_signature`; the export (and every response) **never** contains the private key.

## Test scenarios

Per ADR-0005 §Harness obligation; each maps 1:1 to an AC.

- **SC-HC-001 — verifiable integrity.** Recompute + Ed25519-verify a real incident's snapshot → passes. Realised by hashchain_ac_001.
- **SC-HC-002 — stable canonical form.** JCS is byte-stable → identical hash. Realised by hashchain_ac_002.
- **SC-HC-003 — order determinism under time ties.** Equal-`time` events → one deterministic chain via the `event_id` tiebreaker. Realised by hashchain_ac_003.
- **SC-HC-004 — tamper detection.** Any byte mutation → verification fails. Realised by hashchain_ac_004.
- **SC-HC-005 — key never in plaintext.** Encrypt→decrypt→sign round-trip; ciphertext at rest. Realised by hashchain_ac_005.
- **SC-HC-006 — auditor-facing pubkey, server-side private.** Embedded pubkey verifies; private key never exported. Realised by hashchain_ac_006.

## Compliance

- The chain MUST use `chain_0 = SHA-256("")`, raw-32-byte concatenation with **no separator**, and **no separate `timestamp_n` term** (the time is bound inside `canonical(evidence_n)`); the root is `Ed25519_sign(forensic_key, chain_N)`.
- Canonicalization MUST be **JCS (RFC 8785)** via the `canonicalize` package, in an api-local wrapper; the api MUST NOT import `services/ingest/src/jcs.ts`.
- The forensic signing key MUST be a **dedicated api-owned Ed25519 key**, encrypted at rest with pgcrypto, and MUST NOT be the ingest CA key.
- **The verifier MUST treat the embedded `forensic_pubkey` as a convenience, NOT a trust root.** Verifying a signature **only** against the key embedded in the same export it is checking is a **forbidden anti-pattern** (a compromised server could re-sign with its own key and embed the matching pubkey — the signature would "verify" against itself). Until the out-of-band anchoring of §Open questions is settled, **trust-in-the-key is an external precondition**: the guarantee is integrity *under a key the auditor already trusts*, supplied independently of the export.

## Risks

| Risk | Mitigation |
| --- | --- |
| The embedded pubkey is mistaken for a trust root, giving false assurance against a compromised server | §Compliance forbids verify-against-embedded-only as an anti-pattern; the residual authenticity gap is named in §Open questions with an explicit reopen condition |
| RMT re-ingest changes a snapshot's hash by mechanics, not malice | Documented in ADR-0016 §Context (snapshot-relative under a no-rewrite assumption); strong write-once immutability is out of scope |
| The api gains a new at-rest secret (forensic passphrase) + a Postgres table | Mirrors the established ingest CA pattern (`ca.ts`); pgcrypto already enabled in api (`0001_users.ts:15`) |
| Duplicating the JCS wrapper (api + ingest) risks drift from ingest's RFC-8785 usage | Both pin the same `canonicalize` `^2.0.0`; the shared-package extraction is a named later refactor (§Out of scope); hashchain_ac_002 pins byte-stability |

## Open questions

1. **Trust anchoring of the `forensic_pubkey` (deployment contract — DEFERRED).** This SPEC's verification proves **integrity under a given key**; it does **not** by itself prove **authenticity against a compromised server**. Closing that gap requires an **out-of-band** channel by which an auditor obtains the *authentic* forensic public key **independent of the server** (otherwise a compromised server can re-sign the evidence with a key of its own and embed the matching pubkey). The likely shape is a deployment-configured trust pin (e.g. a `CG_FORENSIC_PUBKEY_FINGERPRINT` the operator publishes/distributes out of band), but the **distribution model depends on the client's operating model, which is not yet determined** — so it is a **deployment-contract decision, not an engineering default**, and is deferred. **Reopen when:** the client operating model is defined, **or** the render / export escalón lands (which needs a settled trust-distribution story). Until then, *"auditable"* in SPEC-012 = **integrity verifiable under a trusted key**, not authenticity against a compromised server.

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern, per SPEC-005..011).

1. **Evidence unit = canonicalized drill output; per-event chain; on-demand snapshot; dedicated api Ed25519 key** — all inherited from ADR-0016 (not re-litigated here).
2. **Mechanics fixed:** `chain_0 = SHA-256("")`; raw-32-byte concat, no separator; **`timestamp_n` folded into `canonical(evidence_n)`** (refinement of `blueprint.md:531`, documented).
3. **Canonicalization:** api adopts the `canonicalize` npm package + a thin local wrapper; **does not** import ingest's `jcs.ts`; **no shared package extracted** this session.
4. **Embedded pubkey is convenience, not trust root** — verify-against-embedded-only is a forbidden anti-pattern; out-of-band anchoring is an **Open question** (deployment contract), not an AC.
5. **All 6 ACs are CI-able** (throwaway-DB / real-backend pattern), not marquees.
6. **Doc-only this gate** — the api migration, `package.json` dependency, and route/service code are a later implementation gate.

## Amendment 2026-06-06: implementation gates A+B and C landed

Two forward-looking statements in the original text are superseded by the implementation (authorized in chat; CLAUDE.md §SPEC amendment workflow — the STOP that surfaced the staleness is the ratification). Where the original text is a point-in-time record it stays: §Ratification record item 6 (*"doc-only this gate"*) accurately describes the gate at which SPEC-012 was **accepted**; this amendment supersedes it only where current state differs.

- **`canonicalize` is now a current api dependency.** `services/api/package.json:29` carries `canonicalize ^2.0.0`, added with the **gate A+B** chain landing (`services/api/src/forensic/canonical.ts` imports it) — no longer "a later implementation gate". The §Out of scope bullet that listed the dependency edit + the implementation code as deferred is removed; §References is corrected in place.
- **The forensic-key implementation landed in gate C** (this session): `services/api/src/db/migrations/0003_forensic_key.ts` (the single-row, pgcrypto at-rest table), `src/forensic/key.ts` (`ensureForensicKey` — the dedicated Ed25519 key), `src/forensic/export.ts` (`buildForensicExport` — chain head + `root_signature` + embedded pubkey), and the `GET /v1/incidents/:id/forensic-export` route.

**Still out of scope** (unchanged by this amendment): out-of-band trust anchoring of the forensic public key (§Open questions 1), the shared-canonicalization-package extraction, PDF/HTML render, and MinIO physical at-rest.

## References

- [ADR-0016](../adr/0016-forensic-evidence-hash-chain.md) — the model this SPEC implements (evidence unit, per-event chain, on-demand snapshot, dedicated api key, JCS discipline, RMT threat boundary).
- [SPEC-010](SPEC-010-forensic-event-drill.md) — the drill `EventTimeline` that is the evidence; the `(time, event_id)` total-order amendment (`## Amendment 2026-06-06`). Code: `services/api/src/read/queries.ts:268` / `:252` (construction), `:262` (order), `:259` (`FINAL`), `:251` (`Set` dedup); `types.ts:57-67` (`TimelineEvent`).
- [SPEC-011](SPEC-011-incident-severity.md) / [SPEC-007](SPEC-007-incident-grouping-mvp.md) — the incident scope of the evidence.
- [SPEC-003](SPEC-003-mtls-signed-envelope.md) — the JCS + Ed25519 discipline reused; `services/ingest/src/jcs.ts` (the `canonicalize` usage api mirrors but does not import).
- `services/ingest/package.json:28` (`canonicalize ^2.0.0`, the package api adopts as its own dep); `services/api/package.json:29` (`canonicalize ^2.0.0`, added with the gate A+B chain landing — see Amendment 2026-06-06).
- `services/ingest/src/ca.ts:85-88` / `:101` + `services/ingest/src/db/migrations/0001_initial.ts:10-16` — the single-row, pgcrypto-encrypted key-at-rest pattern the forensic-key table mirrors; `services/api/src/db/migrations/0001_users.ts:15` (pgcrypto already enabled in the api Postgres).
- [Blueprint](../product/blueprint.md) — `:33` (the *"auditable"* promise), `:527-535` (the hash-chain scheme; `:531` the `timestamp_n` term this SPEC folds into the evidence).
