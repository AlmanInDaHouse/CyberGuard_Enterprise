# Handoff — End of Session 9

Canonical state-of-the-world at the close of Session 9. This document is the
contract Session 10 resumes from, independent of memory compaction in Claude
Code or chat. Everything below is verified against `main`, not recalled.

- **Anchor commit:** `f407c05` (`feat(services): implement SPEC-004 server ingest`)
- **Branch:** `main`
- **Date:** 2026-05-22
- **CI verdict at anchor:** ALL GREEN
- **Known CI debt:** none

## State of `main` at `f407c05`

Phase 1 (the agent: heartbeat, enrollment, mTLS + signed envelope) and the
first slice of Phase 2 (the server: minimal ingest that terminates the agent
protocol and persists) are complete. A real `cg-agent` enrolls and heartbeats
against a real ingest service backed by real Postgres, ClickHouse, and Redis,
and lands real rows — reproducible locally and in CI.

CI workflows for `f407c05`:

```text
ts-ci             → success   (8/8: 7 SPEC-004 ACs + /health, incl. the real-agent marquee)
rust-ci           → success   (cargo fmt --check + clippy -D warnings + test --all)
markdown-lint     → success
schema-validation → skipped   (no schemas/ paths touched by this commit)
```

Session 9 commit map (newest first):

```text
f407c05  feat(services): implement SPEC-004 server ingest          (B5)
b46a6e2  test(services): harness for SPEC-004 ACs (RED by design)  (B4)
acaa2da  ci(ts): drop pnpm cache to fix ts-ci cache-save failure   (Session 8 close)
e18f2e1  chore(services): scaffold services/ingest TypeScript app + ts-ci
efe0dad  chore(deploy): add ingest service to docker-compose.dev.yml
2650b31  feat(agent): support server.heartbeat_url per SPEC-003 amendment
8fc9161  docs(spec): amend SPEC-003 to allow separate heartbeat URL
```

The marquee proof (real agent → real server → real persistence), captured at
close:

```text
PG agents:   agent_id=019e5127-...-2bd8 (UUIDv7), pubkey=32 bytes stored,
             enrolled_at set, last_seen set (≠ null)
CH heartbeats: seq 1..5, status=online, ordered by arrived_at
```

## Accepted ADRs

All `Accepted`; binding. Full catalog and dependency graph in
[docs/adr/README.md](adr/README.md).

| ADR | Title | Note |
|---|---|---|
| [0001](adr/0001-monorepo-layout.md) | Monorepo layout | `agent/`, `services/`, `docs/`, `schemas/`, `infra/` |
| [0002](adr/0002-language-per-component.md) | Language per component | Rust agent; TS for untrusted-input services (amended by 0007) |
| [0003](adr/0003-polyglot-storage.md) | Polyglot storage | Postgres relational, ClickHouse events, Redis nonces |
| [0004](adr/0004-agent-server-protocol.md) | Agent-Server secure protocol | mTLS 1.3, signed envelope, server validation order |
| [0005](adr/0005-detection-rules-and-ml-in-parallel.md) | Detection — rules and ML in parallel | Detection principle; stack-independent |
| [0006](adr/0006-cges-ocsf-alignment.md) | CGES alignment with OCSF v1.3 | Event schema; the envelope body is not yet a CGES event |
| [0007](adr/0007-ingest-language-typescript-mvp.md) | Ingest language — TypeScript for MVP | Reassigns `services/ingest/` to TS; firehose language deferred |

## Accepted SPECs

All `Accepted`. Catalog in [docs/specs/README.md](specs/README.md).

| SPEC | Title | Note |
|---|---|---|
| [001](specs/SPEC-001-agent-heartbeat.md) | Agent heartbeat | Plain-HTTP heartbeat + scheduling/retry |
| [002](specs/SPEC-002-agent-enrollment.md) | Agent enrollment | Token → Ed25519 identity → `identity.json` |
| [003](specs/SPEC-003-mtls-signed-envelope.md) | mTLS 1.3 and signed envelope | Two amendments (below) |
| [004](specs/SPEC-004-server-ingest-minimal.md) | Server ingest minimal | Implemented + Accepted this session |

SPEC-003 carries two amendments, both dated 2026-05-22:

- **(a) separate enroll and heartbeat URLs** — adds optional
  `server.heartbeat_url`; the secure heartbeat target is `heartbeat_url` if
  set, else `server.url`. Landed Session 8 (`8fc9161` + `2650b31`).
- **(b) relax `server.url` scheme constraint** — the `https://` requirement
  moves from `server.url` to the secure heartbeat *target*; it fires only when
  `heartbeat_url` is absent. This unblocks the SPEC-004 two-port topology
  (plain-HTTP enroll on `server.url`, mTLS heartbeat on `heartbeat_url`).
  Landed Session 9 in `f407c05`. Strictly additive; locked by two new
  `config.rs` unit tests; no prior test changed.

## Established workflows (process contracts)

These are binding for every session. Authoritative text in
[CLAUDE.md](../CLAUDE.md); summarized here so Session 10 inherits them.

- **Spec-Driven order.** SPEC → ADR → schemas → harness → implementation → CI
  verification. No code before its SPEC/ADR is Accepted.
- **CI monitoring after every push.** Poll every workflow for the pushed SHA
  to a terminal state; do not close the session or start follow-on work until
  the verdict is `ALL GREEN` (or a red workflow is explicitly downgraded to
  *Known CI debt* by Manuel). `gh` primary, REST API fallback via the git
  credential helper.
- **Known CI debt co-locality.** When a commit turns a workflow RED *by
  design* (harness-first red phase), the debt declaration lands in the **same
  SHA**; the implementation commit that turns it green removes the row in that
  same SHA. The debt table in CLAUDE.md is live state, not history.
- **SPEC amendment workflow.** When implementation contradicts an Accepted
  SPEC/ADR, append an `## Amendment <date>: <title>` section in place; status
  stays `Accepted`; bump `Last updated`; prefer additive, backward-compatible
  amendments. Amending a ratified SPEC/ADR is a STOP — the agent drafts and
  proposes; Manuel ratifies in chat before code lands.
- **Decision authority.** Technical, reversible, in-scope decisions are the
  agent's (decide + communicate). Product scope, money, credentials,
  irreversible high-impact ops, and new/amended ADRs are ask-first.

CI workflows on the repo (GitHub Actions, all path-filtered):

```text
rust-ci            triggers on agent/**, Cargo*, rust-toolchain.toml, its own yml
ts-ci              triggers on services/ingest/**, its own yml; builds cg-agent for the marquee
markdown-lint      triggers on **/*.md
schema-validation  triggers on schemas/**
```

## Approved local toolchain

Authoritative table in [CLAUDE.md](../CLAUDE.md) (§Approved local toolchain).
The agent may install/configure only these without asking.

| Tool | Reason | Introduced |
|---|---|---|
| Task (go-task) | Project build runner (ADR-0001) | Session 1 |
| Docker Desktop | Runtime for `infra/dev/docker-compose.dev.yml` (ADR-0003) | Session 4 |
| Rust toolchain (rustup/cargo/clippy/rustfmt) | `cg-agent` crate (ADR-0002, SPEC-001); pinned by `rust-toolchain.toml` | Session 5 |
| Node.js 22 LTS | `services/ingest/` runtime (ADR-0007, SPEC-004); local tolerates ≥22 (on 24) | Session 8 |
| pnpm (via Corepack) | TS workspace package manager (ADR-0007); pinned by `packageManager` | Session 8 |

Anticipated next addition: a Go toolchain when the event-firehose ingest
begins (its row lands with the ADR/SPEC that introduces it).

## Lessons recorded (auto-memory)

Auto-memory lives at `C:\Users\manul\.claude\projects\c--Users-manul-CyberGuard-Enterprise\memory\`
(`MEMORY.md` is the index). CyberGuard does **not** mirror to the Obsidian
vault. Lessons most relevant to Session 10:

- **Audit implicit constraints when amending a SPEC.** An additive amendment
  can leave a pre-existing, prose-implicit constraint behind that is jointly
  unsatisfiable with the new field. Session 9's second SPEC-003 amendment was
  exactly this — a review-side miss, surfaced only by the first end-to-end
  `main()` exercise (the marquee).
- **Checkpoint over rushing.** If the remaining build cannot meet the quality
  bar in-context, checkpoint at a green commit with a locked resume plan
  rather than ship broken work (Manuel-validated, Session 8 → 9).
- **Verify CI, never assume from silence.** Always confirm workflow
  conclusions via the API; a quiet push is not a green push.
- **Plan before execute; English in repo, Spanish in chat.**

## Session 10 — three viable scope options

All three draw from SPEC-004 §Out of scope (deferred) and the open ADR
threads. Product scope is Manuel's call; each option below states scope,
dependencies, what it unlocks, and the trade-off honestly.

### Option A — Telemetry / observability (ops SPEC)

- **Scope.** Structured log forwarding, metrics export (e.g. Prometheus),
  OpenTelemetry traces for the ingest service; readiness/liveness; optional
  stack dashboards. Deferred explicitly in SPEC-004 §Out of scope.
- **Dependencies.** Lowest — builds on the running ingest service; no new
  storage or schema decisions; touches `infra/` and `services/ingest/`.
- **Unlocks.** Operability and SLO verification (SPEC-004 NFR-001 p99 targets
  become measurable rather than asserted).
- **Trade-off.** Lowest risk, lowest product movement. It is plumbing about
  the plumbing; it does not advance the SOC/XDR story.

### Option B — Read API + dashboard

- **Scope.** A `services/api/` (TypeScript) query service that reads `agents`
  and `heartbeats` back out, plus a first `dashboard/` (React) fleet view —
  the read side of what SPEC-004 writes. Both names are already reserved in
  ADR-0007.
- **Dependencies.** Reads stores already populated by SPEC-004; reuses the TS
  workspace and toolchain. Forces a **minimal** auth decision (SPEC-004
  §Ratification 4 deferred RBAC — a real UI needs at least a thin auth front).
- **Unlocks.** The first human-visible surface — "see your fleet." Strong demo
  value; the natural complement to the marquee's wow moment.
- **Trade-off.** Medium scope. Drags in the deferred admin-user/RBAC question
  in at least minimal form. High visible payoff for moderate risk.

### Option C — Detection-rules MVP

- **Scope.** The correlation/detection path: CGES events flowing end-to-end, a
  rules engine, and a first set of detection rules. ADR-0005 (rules and ML in
  parallel) and ADR-0006 (CGES/OCSF alignment) already frame it.
- **Dependencies.** Largest. Requires CGES events on the wire — today the
  ingest carries only transport meta-messages (enroll/heartbeat), not CGES
  events. This forces the **deferred event-firehose language ADR** (ADR-0007
  §Consequences) and the event-ingest path before rules have data to match.
- **Unlocks.** The core product thesis — actual detection. The strategic
  destination.
- **Trade-off.** Highest value, highest upstream uncertainty, almost certainly
  multi-session. It opens the firehose-language ADR and the CGES event ingest
  in one move.

### Recommendation (honest, non-binding)

**Option B next, with C as the north star (sequence B → C).** SPEC-004 just
closed the write path; B turns the invisible rows into a visible fleet view at
moderate risk and only a minimal auth decision, maximizing demonstrable value
now. C is where the product is going but it forces the deferred firehose ADR
and a much larger build — better entered deliberately after the read surface
exists. A is low-risk plumbing that can fold into either B or C rather than
stand alone. Manuel decides; this is scope, which is ask-first.

## How Session 10 resumes

1. Read this document and `CLAUDE.md` first; they are the binding contract.
2. Confirm `main` is at `f407c05` and all CI is green before starting.
3. Pick a Session 10 scope option (Manuel's call); draft the SPEC/ADR for it
   per the Spec-Driven order before any code.
4. Auto-memory (`MEMORY.md` index) carries the narrative; this document
   carries the facts. Where they disagree, re-verify against `main`.
