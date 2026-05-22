# ADR-0002: Language per component

- Status: Accepted
- Date: 2026-05-20
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

CyberGuard ships a polyglot system. The runtime artifacts comprise:

- A Rust endpoint agent (Windows first, Linux later) under `agent/`.
- A constellation of Go services under `services/{ingest, pipeline, soar, forensic}/`.
- A TypeScript / Fastify API and BFF under `services/api/`.
- A Python FastAPI ML service under `services/ml/`.
- A Next.js 15 dashboard under `dashboard/`.

ADR-0001 locked the monorepo layout — *which* components exist and *where* they live in the tree. This ADR locks *what language* lives in each component and the rules that govern future additions.

The choice matters because each language carries tooling, hiring, learning-curve and operational costs that compound across the project. A polyglot codebase chosen casually becomes a polyglot codebase that nobody can fully own; a polyglot codebase chosen deliberately becomes one where every language earns its place.

The decision recorded here must:

1. Match each language to the runtime profile of the component that uses it (footprint, concurrency model, throughput, ecosystem).
2. Bound the set of languages to the minimum that the product genuinely needs.
3. Be honest about the costs incurred — in particular, the learning curve of Rust for the agent.
4. Establish governance rules that prevent silent language drift as the project grows.

## Decision

Each top-level runtime component in the monorepo is implemented in a single language, assigned as follows:

| Component | Language | Primary justification |
|---|---|---|
| `agent/` (Rust workspace) | Rust | Footprint, memory safety, single binary, no runtime |
| `services/ingest/` | Go | I/O concurrency, NATS and ClickHouse client maturity |
| `services/pipeline/` | Go | I/O concurrency, NATS and ClickHouse client maturity |
| `services/soar/` | Go | I/O concurrency, NATS and ClickHouse client maturity |
| `services/forensic/` | Go | I/O concurrency, NATS and ClickHouse client maturity |
| `services/api/` | TypeScript | Type sharing with the dashboard, Fastify + Zod ergonomics |
| `services/ml/` | Python 3.12 | ML and AI ecosystem irreplaceable here; isolated to one service |
| `dashboard/` | TypeScript | Next.js 15 App Router, mature ecosystem for SOC-grade UI |

The total language surface is bounded to **four**: Rust, Go, TypeScript, Python.

> **Amended (2026-05-22):** the `services/ingest/` row is superseded for the MVP by [ADR-0007](0007-ingest-language-typescript-mvp.md) — the agent control-plane ingest (enroll + heartbeat) is **TypeScript + Fastify**. The high-throughput event firehose that motivated `ingest = Go` is deferred and will get its own ADR. The four-language bound is unaffected (TypeScript is already in the set).

### Cross-cutting rules

**Rule 1.** Python lives in `services/ml/` only. No Python anywhere else in the repository, including scripts, tooling, or harness internals. If a build or dev script is needed, write it as a Task target invoking Go, Node, or PowerShell / Bash — not Python.

**Rule 2.** The agent target is Windows-first. Linux comes later but stays in the same Rust workspace; macOS is explicitly out of scope per Blueprint §17.10.

**Rule 3.** Go is the default for new server-side code: services in `services/`, server-side tooling, and the harness runner in `harness/cmd/`. Adding a non-Go server-side artifact requires a new ADR that supersedes this one for that artifact.

**Rule 4.** TypeScript is the default for any user-facing or contract-shared layer (API request/response types, OpenAPI generation, dashboard).

**Rule 5.** Contract sharing — types that cross language boundaries (CGES events, API contracts) are generated *from* the canonical schemas in `schemas/`, not handwritten in each language. The principle is locked here; the specific tooling decision is deferred to a future ADR.

**Rule 6.** Detection rules (`rules/`) and SOAR playbooks (`playbooks/`) are declarative YAML, treated as schema-driven configuration, not code. They are not "a language choice" in the sense of this ADR but are listed here to make the boundary explicit: rules and playbooks must never be replaced by hand-rolled Go or TypeScript equivalents. Their schemas are versioned in `schemas/` and validated in CI.

## Alternatives considered

### A1 — Single-language monorepo

A monorepo where all server-side code is TypeScript (Node, with Bun for performance-critical paths), plus the Rust agent.

Pros: smaller cognitive surface, one tooling ecosystem on the server, faster onboarding for developers coming from the web stack.

Cons: Node is the wrong tool for high-throughput event ingestion and for the SIEM correlation pipeline (GC pauses, no real parallelism without worker threads); Python ML cannot disappear because local AI is a product principle (see A4); the consolidation gain is illusory because the agent forces Rust regardless and ML forces Python regardless. The repository ends with three languages either way and loses Go's I/O and concurrency strengths on the server.

Rejected because the imagined "single language" savings do not materialize once the agent and ML constraints are honoured.

### A2 — Rust everywhere on the server

Rust used for every server service (`services/ingest`, `services/pipeline`, `services/soar`, `services/forensic` and `services/api`) in addition to `agent/`.

Pros: maximum performance ceiling, one language across the whole backend, memory safety as a uniform property.

Cons: development velocity drops sharply for I/O-bound services where Go achieves equivalent runtime performance at a fraction of the development cost; an API and BFF in Rust loses the type-sharing advantage with the TypeScript dashboard; hiring is harder; compile times compound across many crates; this is the choice of companies operating at hyperscale (Cloudflare, Datadog) and is not justified at CyberGuard's stage.

Rejected. The principle captured in Blueprint §5 — *"Rust where it pays off; Go where it doesn't"* — is preserved.

### A3 — Python on the server

Python used on the server (FastAPI for the API, Celery or RQ for the pipeline, Python for SOAR) in addition to the Python ML service.

Pros: fastest prototyping, single language between the ML service and the rest of the server, large library ecosystem.

Cons: insufficient throughput for the SIEM ingest path (10-100k events per second target per Blueprint §9); the GIL limits true parallelism; deployment surface (interpreter version, packaging, native dependencies) is much larger than Go's static binaries; the type system is weaker than TypeScript for an API and BFF that must validate untrusted input from the dashboard and from agents.

Rejected. Python is contained to `services/ml/`.

### A4 — Go plus TypeScript only, no Python anywhere

The Python ML service is replaced by WASM-embedded inference or by calls to hosted AI APIs; the rest of the server is Go and TypeScript.

Pros: even smaller language surface; no Python operational overhead.

Cons: local AI is a product principle (Blueprint §1 specifies *"isolated Python microservice"*; Blueprint §11 UEBA depends on local models); WASM ML is immature for the embedding, RAG and incident-summary workloads that `cg-ml` must serve; hosted AI APIs violate the *self-deployable* promise of the product (Blueprint §2).

Rejected. The Python ML service stays, and the Python footprint is contained to that single service.

## Consequences

### Positive

- Each component runs in a language matched to its runtime profile, with no language doing a job it is poorly suited to.
- The polyglot toolchain is bounded and explicit: four languages, no more. Adding a fifth requires a new ADR.
- Type sharing between API and dashboard reduces a class of contract drift bugs at the boundary that matters most to the user.
- Python is isolated to `services/ml/`. If `cg-ml` ever needs replacing — by a different inference stack, by a different language, or by a hosted alternative under a future ADR — the change does not ripple through the rest of the system.
- The agent's footprint and security posture are defensible to enterprise customers and to security auditors. Memory safety by construction is a first-order sales argument for a security product.

### Negative

- A local development environment requires the Rust toolchain, Go, Node, and Python all installed and version-pinned. This is mitigated by `task bootstrap`, which is responsible for materializing the environment from a clean checkout.
- The CI matrix grows: separate jobs per language for lint, test and build. The cost is real and will surface in workflow design.
- Cross-language contract drift is now an active concern. It is mitigated by schema-first generation under Rule 5, but the specific tooling is deferred and the risk is not zero in the interim.
- Hiring or onboarding a single contributor who covers all four languages is unrealistic. The project assumes per-area contributors over time (agent, server, frontend, ML) rather than a single full-stack maintainer.

**Honest cost of choosing Rust for the agent:**

- Learning curve of 2-3 months to reach production quality without prior Rust experience.
- Compilation is slow at scale; `sccache` or an equivalent build cache is mandatory from day one.
- Cross-platform native APIs require careful handling (`windows-rs` on Windows, `nix` on Linux).

These costs are accepted because the agent is the only artifact distributed to endpoints, and the language properties (no GC pauses, no runtime, audit-grade memory safety) are first-order requirements there. They are not first-order requirements anywhere else in the system, which is why the rest of the server runs on Go.

### Neutral

- Future ADRs will lock specific framework choices (Fastify and Zod for the API, FastAPI for ML, `tokio` and `rustls` for the agent, Next.js 15 App Router for the dashboard) and the contract-generation tooling under Rule 5. This ADR locks languages only, not frameworks.
- Release tagging is per-component (`agent/vX.Y.Z`, `api/vX.Y.Z`, and so on), as already noted in ADR-0001 §Consequences/Neutral. The polyglot decision reaffirms but does not alter that convention.

## Compliance

Subsequent ADRs and SPECs that introduce code in a language outside the set {Rust, Go, TypeScript, Python} — or that move code across the boundaries defined in the table above and in Rules 1-6 — must reference this ADR and explicitly justify the addition or relocation. Cross-cutting Rule 3 in particular requires a superseding ADR for any non-Go server-side artifact.

The contract-generation tooling promised under Rule 5 is reserved for a dedicated future ADR (`ADR-0008 — Contract generation tooling`), to be triggered when the design of `cg-ingest` or `cg-api` forces the choice between candidate tools (quicktype, typeshare, protoc-gen-*, or a custom script).

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout (defines the top-level components this ADR assigns languages to)
- Blueprint §4 — Recommended Stack
- Blueprint §5 — Rust Decision
- Blueprint §17.10 — macOS support not in MVP
- Onboarding §5 — Technology stack (locked decisions)
