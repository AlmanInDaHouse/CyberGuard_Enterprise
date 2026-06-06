# ADR-0007: Ingest service language — TypeScript for the MVP control plane

- Status: Accepted
- Date: 2026-05-22
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0002 assigned `services/ingest/` to **Go**, justified by *"I/O concurrency, NATS and ClickHouse client maturity"* and, in its rejected alternatives, by the unsuitability of Node for *"high-throughput event ingestion (10–100k events per second)"* and for *"the SIEM correlation pipeline."*

SPEC-004 — the first server-side SPEC — does **not** build that high-throughput event firehose. Its scope is the agent **control plane**:

- `POST /v1/agents/enroll` — rare, one per agent lifetime.
- `POST /v1/agents/heartbeat` — one signed envelope per agent per 30 s (modest: 10k agents ≈ 333 req/s, not the 10–100k-eps event stream).
- Persist agent identities (Postgres) and heartbeats (ClickHouse); validate mTLS, signatures, nonces, timestamps.

This is exactly the workload ADR-0002 §A3 argues TypeScript is *best* for: *"an API and BFF that must validate untrusted input from the dashboard and from agents"* — type-safe validation (Fastify + Zod) of untrusted agent input, with a path to type-sharing the contracts with the dashboard. The high-throughput telemetry firehose (process / network / file events at 10–100k eps) that motivated `ingest = Go` is a **future** concern, out of SPEC-004 scope.

A decision-critical viability question — can a Node service issue the Ed25519 client certificates SPEC-002/003 require? — was investigated and **answered yes**: `@peculiar/x509` on Node 20+ (verified on Node 24) issues an X.509 certificate with an Ed25519 subject public key, signed by an Ed25519 CA private key, consuming the agent's raw 32-byte public key directly (the SPEC-002 wire form). So the language choice is not constrained by the cryptography.

Concretely verified (so this ADR is self-contained regardless of later SPEC-004 paths): `@peculiar/x509` **2.0.0** on **Node 24.12.0** using Node's native WebCrypto Ed25519 — import the raw key via `crypto.subtle.importKey("raw", <32 bytes>, { name: "Ed25519" }, …)`, issue with `X509CertificateGenerator.create({ signingAlgorithm: { name: "Ed25519" }, publicKey, signingKey: caPrivateKey })`. The issued certificate verified against the CA and round-tripped to the identical 32 subject-key bytes. (`@peculiar/x509` v2 requires `import "reflect-metadata"` at the entry point.)

The conflict surfaced during the SPEC-004 pre-redaction sanity reads and was escalated to the project owner rather than resolved silently, per ADR-0002's own Compliance clause: *"Adding a non-Go server-side artifact requires a new ADR that supersedes this one for that artifact."*

## Decision

For the MVP, `services/ingest/` is implemented in **TypeScript + Fastify**. This ADR amends the `services/ingest/` row of ADR-0002's component-language table — and only that row.

Unchanged by this ADR:

- Every other row of ADR-0002 (agent = Rust, pipeline / soar / forensic = Go, api = TypeScript, ml = Python, dashboard = TypeScript).
- ADR-0002's four-language bound `{Rust, Go, TypeScript, Python}`. TypeScript is already inside the bound (`services/api/`, `dashboard/`), so this adds **no new language** to the system.
- ADR-0002 Rules 1–6, including Rule 3's requirement that *future* non-Go server-side artifacts justify themselves against it.
- ADR-0004's agent-server protocol (mTLS 1.3, signed envelope, server validation order). The protocol is language-agnostic; the validation order it mandates is implemented identically in TypeScript.

Explicitly deferred: the **high-throughput event firehose** (raw process / network / file telemetry at 10–100k eps, persisted and forwarded to NATS for the correlation pipeline). It is out of SPEC-004 scope. If and when it materialises, it opens its own ADR with throughput evidence and may be a separate Go component, or may extend this service — decided then, not now. This ADR does **not** pre-authorise TypeScript for that firehose.

## Alternatives considered

### A1 — Keep `services/ingest/` in Go (ADR-0002 as written)

Pros: no amendment; Go's I/O concurrency and the mature ClickHouse / NATS Go clients are already the right tools for the future firehose; one consistent language for everything under `services/` except `api/`.

Cons: the MVP control-plane workload does not need Go's throughput ceiling; it forgoes type-sharing of the enroll/heartbeat contracts with the TypeScript dashboard; and it argues *against* ADR-0002 §A3's own reasoning, which places untrusted-agent-input validation in the TypeScript column. Two server languages (Go + TypeScript) exist either way.

Rejected for the MVP control plane; retained as the likely choice for the future firehose, to be settled in its own ADR.

### A2 — Put the control plane in `services/api/` (already TypeScript), leave `services/ingest/` empty

Pros: zero amendment to ADR-0002; `services/api/` is already TypeScript + Fastify.

Cons: `services/api/` is the **dashboard-facing BFF**; folding the agent-facing enroll/heartbeat endpoints into it blurs a boundary that matters for a security product (the agent ingress and the operator UI have different threat surfaces and scaling profiles). The "ingest" identity — Docker Compose service name, the `task ingest:*` CLI, ClickHouse heartbeat persistence — belongs in `services/ingest/`, not in the BFF.

Rejected. The agent ingress deserves its own component.

### A3 — TypeScript at `services/ingest/` for the MVP, defer the firehose's language (chosen)

Pros: matches the control-plane workload to the language ADR-0002 §A3 already endorses for untrusted-input validation; keeps the agent ingress as a first-class, separately-scalable component; verified-viable Ed25519 cert issuance; no new language added to the four-language bound.

Cons: when the high-throughput firehose lands, `services/ingest/` may need to either absorb a Go sub-component or hand the firehose to a sibling Go service — a seam to be designed later. Accepted as a deferred, explicitly-flagged cost.

## Consequences

### Positive

- The agent control plane validates untrusted input with Fastify + Zod and TypeScript types, the posture ADR-0002 §A3 prescribes for exactly this boundary.
- Enroll/heartbeat contracts are sharable with the TypeScript dashboard and the future contract-generation tooling (ADR-0002 Rule 5 / the reserved ADR-0008).
- Ed25519 client-certificate issuance is verified viable in Node (`@peculiar/x509`), so SPEC-002/003 compatibility holds with no algorithm change.
- A new CI workflow (`ts-ci`: `tsc --noEmit`, lint, test) is added under a `services/ingest/**` path filter.

### Negative

- A second server-side language now has runtime code under `services/` (TypeScript joins Go). It is **not** a new language for the system, but it does mean the ingest tier is not uniformly Go.
- The deferred high-throughput firehose may reintroduce Go at or beside `services/ingest/`, requiring a future ADR and a component seam. This cost is named, not hidden. When that future ADR opens, it must explicitly decide whether the control-plane TypeScript ingest and the data-plane firehose coexist in `services/ingest/` as sub-components, split into `services/ingest/` (TS, control plane) + a sibling Go service (e.g. `services/event-ingest/`), or follow a different decomposition. The decision is deferred but the question is locked in.
- The local toolchain and CI matrix grow by a Node.js LTS install and the `ts-ci` workflow.

### Neutral

- ADR-0002 remains `Accepted`; only its `services/ingest/` row is superseded, and a pointer to this ADR is added there.
- ADR-0004's protocol decisions are untouched; the README dependency note "Go for ingest is an input to the protocol" is corrected to language-agnostic — the protocol does not depend on the ingest language.

## Compliance

This ADR supersedes the `services/ingest/` row of ADR-0002 for the MVP. Subsequent work on `services/ingest/` is TypeScript unless a further ADR supersedes this one.

Introducing the high-throughput event firehose (10–100k eps telemetry ingestion) requires a new ADR that presents throughput evidence and decides that workload's language and component boundary; this ADR does not pre-decide it.

## Amendment 2026-06-07 (SPEC-013): the collateral "forensic = Go" mention is superseded

The §Decision "Unchanged by this ADR" list states *"Every other row of ADR-0002 (agent = Rust, pipeline / soar / forensic = Go, api = TypeScript, ml = Python, dashboard = TypeScript)"* — asserting, collaterally, that ADR-0002's `services/forensic/ = Go` row was untouched by the ingest pivot. That collateral assertion is now superseded: [SPEC-013](../specs/SPEC-013-forensic-report-render.md) (Accepted) realized the per-incident forensic report render as a **TypeScript module in `services/api/`** (`@react-pdf/renderer`), not a standalone Go `services/forensic/` service. The **core decision of this ADR is unchanged** — `services/ingest/` is TypeScript for the MVP control plane; only the incidental restatement of "forensic = Go" no longer holds. The authoritative record of the forensic pivot lives in [ADR-0002](0002-language-per-component.md) §Amendment 2026-06-07 (SPEC-013). This ADR remains `Accepted`.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout (`services/ingest/` location).
- [ADR-0002](0002-language-per-component.md) — Language per component. This ADR amends its `services/ingest/` row; §A3 supplies the TypeScript-for-untrusted-input rationale.
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage (Postgres for agent identities, ClickHouse for heartbeats, Redis for nonces).
- [ADR-0004](0004-agent-server-protocol.md) — Agent-Server secure protocol. Language-agnostic; implemented server-side by this component.
- SPEC-004 (forthcoming) — Server ingest minimal; the first consumer of this decision.
- Verified viability probe: `@peculiar/x509` 2.0.0 on Node 24 issuing an Ed25519 client cert (raw 32-byte subject key) signed by an Ed25519 CA — documented in SPEC-004 §Functional requirements > cert issuance.
