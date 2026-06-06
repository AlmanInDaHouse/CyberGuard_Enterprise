# SPEC-013: Forensic report render — per-incident PDF over the composed read-layer outputs

- **ID:** SPEC-013
- **Title:** Forensic report render (escalón 4 — per-incident PDF, composing the SPEC-010/011/012 read-layer outputs as a module in `services/api`)
- **Status:** Accepted
- **Depends on:** SPEC-010 (the drill `EventTimeline` / `TimelineEvent` rendered as the timeline, `services/api/src/read/types.ts:57-72`); SPEC-011 (the incident `severity_id` aggregation in the header, `types.ts:34-35`); SPEC-012 (the evidence hash-chain seal — `chain_root` / `root_signature` / `forensic_pubkey` — the report transcribes, `services/api/src/forensic/export.ts:32-42`); ADR-0015 (the read-only ClickHouse reader the drill, hence the report's timeline, uses — reused unchanged). Also SPEC-009 (the `IncidentDetail` read-model — `severity_id` / `cg_mitre` / `title` / `status` / `assigned_to` / `alerts[]`, `services/api/src/read/queries.ts:161-193`); ADR-0007 (the TypeScript-language precedent that fixes the render in `services/api`, not the blueprint's aspirational Go service).
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

This is **escalón 4** of the *"exportable forensic report on the first incident"* promise (`docs/product/blueprint.md:33`; MVP acceptance criterion *"Incident PDF export"*, `blueprint.md:750`). Escalones 1–3 are delivered and each produces a structured output in the `services/api` read-layer: the drill / timeline (SPEC-010), the incident severity (SPEC-011), and the evidence hash-chain + Ed25519 root signature (SPEC-012). This SPEC renders those outputs as a downloadable per-incident **PDF** — the human-facing artifact the prior escalones fed.

A READ-ONLY audit (this session) established the constraints this SPEC fixes (it does not re-debate them):

- **The render is a MODULE in `services/api`, not a new service.** The blueprint names a Go report service with headless Chromium / WeasyPrint (`blueprint.md:393`), but the repo has **zero Go** (no `go.mod`, no `*.go`) and the one comparable planned-Go service — `ingest` — was built in **TypeScript** (ADR-0007). The api already owns the read-layer + the forensic-export this report composes, and already reads Postgres + ClickHouse in-process (`services/api/src/services.ts`). Location = `services/api`, language = TS — **ratified (Manuel's STOP); no new ADR**.
- **PDF only.** The MVP promise names PDF (`blueprint.md:750`). HTML and JSON are dropped: there is no pure-JS HTML→PDF library that works without a browser engine (every real HTML→PDF — `html-pdf-node`, `html-pdf`, `puppeteer` — requires Chromium / PhantomJS, excluded; WeasyPrint is Python), and JSON is already served by the SPEC-012 forensic-export endpoint.
- **The PDF is BUILT programmatically, not converted from HTML**, via `@react-pdf/renderer` (a declarative PDF-builder over React primitives; ESM; maintained). The report composes the read-layer shapes into the document; it parses no HTML.
- **On-demand, like forensic-export. No persistence** (MinIO at-rest is recorte (b), still deferred). No new service. No Chromium.

This SPEC fixes the report **contents + the endpoint**; the `@react-pdf/renderer` dependency edit, the render module, the route code, and the PDF-text-extraction test dependency are the **implementation gate** (a later commit), not this doc.

## Scope

### In scope

1. **A render module in `services/api`** that composes a per-incident forensic PDF from the read-layer outputs (§Data contracts).
2. **The PDF builder = `@react-pdf/renderer`**, server-side (its Node render API, e.g. `renderToBuffer`), building the document **programmatically** from the composed data — no HTML, no Chromium.
3. **The on-demand endpoint `GET /v1/incidents/:id/report`** returning `application/pdf` — the **first non-JSON Content-Type** in the api (`reply.type("application/pdf").send(buffer)`) — behind the SPEC-008 session preHandler, org-scoped to `session.org_id`, 404 mirroring the sibling drill / forensic-export handlers.
4. **Acceptance criteria** as **CI-able** integration tests (the throwaway-DB / real-backend pattern of the SPEC-012 ACs; **no marquee, no `skipIf`**), asserting by **extracted content / structure, not by PDF bytes**.

### Out of scope

Each names its destination:

- **Physical at-rest / MinIO persistence of the rendered PDF** — recorte (b), still deferred (ADR-0016 / SPEC-012 §Out of scope). A later forensic-persistence increment.
- **HTML and JSON report formats** — dropped: no pure-JS HTML→PDF without Chromium (the ratified constraint), and JSON is already the SPEC-012 forensic-export response.
- **A standalone Go forensic service** (`blueprint.md:393`'s *"Go + headless Chromium"*) — superseded; the render is a TS module in `services/api` per ADR-0007 + ratification.
- **Out-of-band trust anchoring of the forensic public key** — unchanged: SPEC-012 §Open questions 1. The report **transcribes** SPEC-012's seal and therefore inherits its exact property — **integrity verifiable under a trusted key, NOT authenticity against a compromised server**. The report does **not** resolve it (see §Open questions).
- **The AI-generated incident summary** (`services/forensic/README.md:12`) — not part of the MVP PDF promise; a later increment.
- **The dependency edit + render module + route code + the PDF-extraction test dependency** — the implementation gate (a later commit); this SPEC is the doc layer only.

## Data contracts

The report **composes** two already-produced read-layer outputs for one incident; it **re-derives nothing**.

### Sources (transcribed / rendered, unchanged)

- **`ForensicExport`** (SPEC-012, `services/api/src/forensic/export.ts:32-42`) via `buildForensicExport(services, orgId, incidentId)` (`export.ts:44-64`): `{ incident_id, events: TimelineEvent[] (total order time ASC, event_id ASC), chain_root (hex), root_signature (base64url), forensic_pubkey (base64url) }`.
- **`IncidentDetail`** (SPEC-009 / SPEC-011, `services/api/src/read/types.ts:29-42`) via `getIncidentDetail(services.pg, orgId, incidentId)` (`services/api/src/read/queries.ts:161-193`): `{ incident_id, agent_id, status, title, severity_id, cg_mitre {tactics[],techniques[]} | null, window_start, assigned_to, created_at, updated_at, alerts: ResolvedAlert[] }`.

Both are invokable from one handler with what `services` exposes today (`pg`, `ch`, `forensicKey` — `services/api/src/services.ts:18-28`); both are org-scoped and return `null → 404`.

### Report sections

1. **Header** — `title`, `status`, `severity_id` (OCSF 0–6), `assigned_to` (or *"unassigned"*), `agent_id`, `window_start`, `created_at` / `updated_at`. From `IncidentDetail`.
2. **MITRE ATT&CK** — `cg_mitre.tactics` / `cg_mitre.techniques`. **Clean degradation when `cg_mitre` is `null`** (render *"none mapped"*, never crash). From `IncidentDetail` (`types.ts:36`).
3. **Timeline** — the `events: TimelineEvent[]` in total order (time ASC, event_id ASC), each event's projected fields (`event_time`, `process_name`, `process_pid`, `image_file_name`, `process_parent_pid`, `activity_id`, `agent_id`). From `ForensicExport.events` (≡ the SPEC-010 drill).
4. **Alerts** — `alerts: ResolvedAlert[]` (`title`, `severity_id`, `status`, `rule_id`, `event_time`, `final_score`, per-alert `cg_mitre`). From `IncidentDetail.alerts`.
5. **Integrity block** — `chain_root`, `root_signature`, `forensic_pubkey` **TRANSCRIBED verbatim** from `ForensicExport` (**not** re-derived, **not** re-signed). An auditor reads this block to verify tamper-evidence (under a trusted key; §Compliance). The forensic **private** key never appears (inherited from SPEC-012). The block MUST also render a **visible limitation note** — text **in the PDF**, not only in this doc — stated with ontological precision (what the seal proves *and* what it does not). Canonical text (the impl renders this or a faithful equivalent): *"Integrity, not authenticity — this signature proves the evidence in this report is intact under the key whose public half (forensic_pubkey) is shown above; it does not prove that key is authentic. A verifier must confirm forensic_pubkey against a source trusted independently of this server (out-of-band anchoring; pending a deployment decision — SPEC-012 §Open questions 1)."* This note is the **provisional mitigation** while out-of-band anchoring stays deferred (§Open questions), **not** its resolution.

The PDF is built programmatically from these sections via `@react-pdf/renderer`; there is **no HTML intermediate**.

## Acceptance criteria

Each maps to a **CI-able** integration test under `services/api/test/` (the throwaway-DB / real-backend pattern of the SPEC-012 ACs; **not** a marquee, no `skipIf`). Assertions are by **extracted content / structure** — the PDF byte-output is **non-deterministic** (embedded timestamps / ids), so byte golden-files are out; a PDF-text-extraction **test** dependency is declared at the implementation gate (§Test scenarios).

- **report_ac_001 (complete composition).** `services/api/test/report-ac-001-composition.test.ts`. An incident seeded with events + alerts + severity + MITRE → the rendered PDF contains **all** sections: the timeline event rows, the severity, the MITRE tactics / techniques, the title, the alerts, and the integrity block. Verified by extracting the PDF text / structure and asserting each section's data is present.
- **report_ac_002 (seal fidelity).** `report-ac-002-seal-fidelity.test.ts`. The integrity block in the PDF equals **exactly** the `chain_root` / `root_signature` / `forensic_pubkey` that `buildForensicExport` returns for the same incident — **transcribed, not re-derived or altered**. Binds the report to SPEC-012's chain of custody (a report whose seal diverged from the export would be a custody break).
- **report_ac_003 (transport).** `report-ac-003-transport.test.ts`. `GET /v1/incidents/:id/report` returns `Content-Type: application/pdf` (a valid PDF — `%PDF` magic header), behind `requireSession` (401 without a session), org-scoped (cross-org → 404), and 404 on a non-existent / malformed incident id. The **first non-JSON response** in the api.
- **report_ac_004 (empty-evidence edge).** `report-ac-004-empty.test.ts`. An incident that resolves to **zero events** → a valid (non-crashing) PDF whose integrity block carries the empty-evidence seal over `chain_0 = SHA-256("")` (coherent with SPEC-012's N=0 case); a `null` `cg_mitre` degrades cleanly (*"none mapped"*).
- **report_ac_005 (visible limitation note).** `report-ac-005-limitation-note.test.ts`. The rendered PDF **contains the limitation note** in the integrity block — verified by extracting the PDF text and asserting the note's substance is present (the seal proves integrity under the shown key, **not** authenticity; out-of-band confirmation of `forensic_pubkey` is required). Makes the provisional mitigation for the deferred out-of-band anchoring auditable in the artifact itself.

## Test scenarios

Per ADR-0005 §Harness obligation; each maps 1:1 to an AC.

- **SC-RPT-001 — full report.** A seeded incident renders to a PDF with every section. Realised by report_ac_001.
- **SC-RPT-002 — custody-faithful seal.** The report's seal ≡ the forensic-export seal. Realised by report_ac_002.
- **SC-RPT-003 — PDF transport.** `application/pdf`, authed, org-scoped, 404 on absent. Realised by report_ac_003.
- **SC-RPT-004 — empty incident.** Zero events → a valid PDF, `chain_0` seal, null MITRE clean. Realised by report_ac_004.
- **SC-RPT-005 — auditable limitation.** The PDF carries the integrity-not-authenticity note in the integrity block. Realised by report_ac_005.

The content assertions need a **PDF-text-extraction dependency** (e.g. `pdf-parse` or `pdfjs-dist`) as a `services/api` **test (dev) dependency** — declared here, **added at the implementation gate** (not this doc). **No production PDF-parsing dependency** is introduced: the api **builds** PDFs, it never parses them at runtime.

## Compliance

- The report's integrity block MUST **transcribe** SPEC-012's seal (`chain_root` / `root_signature` / `forensic_pubkey`) verbatim from `buildForensicExport`; it MUST NOT re-derive the chain, re-sign, or alter the seal — **preserving SPEC-012's chain of custody**.
- The endpoint MUST NOT expose the forensic **private** key (inherited from SPEC-012; the report carries only the public key + signature; the private key is non-extractable and never serialized — `services/api/src/forensic/key.ts`).
- The render MUST be **programmatic** (`@react-pdf/renderer`) with **no HTML intermediate** and **no browser engine** (no Chromium / PhantomJS).
- The endpoint MUST sit behind the SPEC-008 session preHandler and be **org-scoped** to `session.org_id` (never the request), mirroring the sibling read routes; **GET, read-only** (no role gate, no CSRF — consistent with `/events` and `/forensic-export`).
- The PDF MUST render **without crashing** for the empty-evidence and `null`-MITRE cases (report_ac_004).

## Risks

| Risk | Mitigation |
| --- | --- |
| PDF byte-output is non-deterministic (embedded CreationDate / ids) → byte golden-files are flaky | ACs assert by extracted content / structure, not bytes (§Acceptance criteria); if byte-determinism is later needed, pin the document metadata at the impl gate |
| `@react-pdf/renderer` pulls React into a Fastify (non-React) service | A bounded, ratified dependency; used only via its Node render API in the render module — an implementation detail of the impl gate; no React elsewhere in the api |
| The seal could drift from the export if the report re-derived it | §Compliance forbids re-derivation; report_ac_002 pins seal ≡ `buildForensicExport` output |
| The transcribed seal gives integrity, not authenticity vs a compromised server | The same property as SPEC-012 (inherited, §Out of scope + §Open questions); the PDF renders a **visible limitation note** (§Data contracts §5) making the boundary auditable in-artifact, pinned by report_ac_005 — the provisional mitigation, not a resolution |
| First non-JSON response in the api (a new `reply.type` path) | Bounded to the one route; report_ac_003 pins the Content-Type + PDF validity |

## Open questions

This SPEC introduces **no new** Open question. It inherits **SPEC-012 §Open questions 1** (out-of-band trust anchoring of `forensic_pubkey`) **unchanged** — the transcribed seal proves integrity under a trusted key, not authenticity against a compromised server.

Note a chaining effect, **flagged not resolved**: SPEC-012's reopen condition for that Open question was *"the client operating model is defined, **or** the render / export escalón lands"*. **Landing this SPEC satisfies the second trigger** — this escalón *is* the render/export the condition names. The trust-distribution / out-of-band-anchoring decision is therefore now **live for Manuel** as the deployment-contract **owner-STOP** it has always been (CLAUDE.md §Decision authority); it is **not** resolved here and MUST NOT be picked by design inertia. It remains tracked in SPEC-012 §Open questions. **The report's visible limitation note (§Data contracts §5, report_ac_005) is the provisional mitigation** — it makes the integrity-not-authenticity boundary auditable in the artifact while anchoring stays deferred; it is **not** the resolution (which is the owner-STOP deployment decision).

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern, per SPEC-005..012).

1. **Location = module in `services/api`; language = TS** — not a new Go service. ADR-0007 precedent (ingest landed TS, not the blueprint's Go) + the api already owns the read-layer + forensic-export this report composes. **No new ADR** (the decision is registered here).
2. **PDF only** — the MVP promise (`blueprint.md:750`). HTML / JSON dropped (no pure-JS HTML→PDF without Chromium; JSON is already the forensic-export response).
3. **`@react-pdf/renderer`, programmatic build** — a PDF-builder, not HTML→PDF; ESM, maintained. The report composes the read-layer shapes; no HTML intermediate.
4. **Composes `ForensicExport` + `IncidentDetail`; transcribes the seal** — re-derives nothing; report_ac_002 binds the seal to SPEC-012's custody.
5. **On-demand, no persistence** — MinIO recorte (b) stays deferred.
6. **All 4 ACs CI-able** (throwaway-DB; content / structure assertions, not bytes); a test-only PDF-extraction dep at the impl gate.
7. **Doc-only this gate** — the dependency edit, the render module, the route, and the test dep are the implementation gate (a later commit).
8. **Visible limitation note in the PDF** (§Data contracts §5) — the provisional mitigation for the deferred out-of-band anchoring; makes integrity-not-authenticity auditable in the artifact (report_ac_005), **not** a resolution of SPEC-012 §Open questions 1.

## References

- [Blueprint](../product/blueprint.md) — `:750` (the MVP *"Incident PDF export"* promise this SPEC fulfils), `:33` (the *"exportable forensic report"* title promise), `:393` (the `report` pipeline stage — note the **realized stack is TS + `@react-pdf/renderer` in `services/api`**, NOT the aspirational *"Go + headless Chromium or WeasyPrint"* the blueprint names; superseded by ADR-0007 + ratification).
- [SPEC-012](SPEC-012-forensic-evidence-hashchain.md) — the evidence hash-chain seal the report transcribes (`chain_root` / `root_signature` / `forensic_pubkey`); the inherited trust-anchoring Open question. Code: `services/api/src/forensic/export.ts:32-42` (`ForensicExport`), `:44-64` (`buildForensicExport`).
- [SPEC-010](SPEC-010-forensic-event-drill.md) — the drill `EventTimeline` / `TimelineEvent` rendered as the timeline (`services/api/src/read/types.ts:57-72`).
- [SPEC-011](SPEC-011-incident-severity.md) — the incident `severity_id` in the header (`types.ts:34-35`).
- [SPEC-009](SPEC-009-read-slice.md) — the `IncidentDetail` read-model the header / MITRE / alerts come from (`services/api/src/read/queries.ts:161-193`).
- [ADR-0015](../adr/0015-readonly-clickhouse-reader-in-api.md) — the read-only ClickHouse reader the drill (hence the report's timeline) uses, reused unchanged.
- [ADR-0007](../adr/0007-ingest-language-typescript-mvp.md) — the precedent that a planned-Go server component lands in TypeScript; fixes the render language as TS in `services/api`.
