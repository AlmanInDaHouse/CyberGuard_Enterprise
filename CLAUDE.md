# CLAUDE.md — CyberGuard Enterprise

Project-level instructions for any Claude (or Claude-like) agent working on this repository. Read this file before editing code, ADRs, schemas, or workflows. The global `~/.claude/CLAUDE.md`, the foundational [Blueprint](docs/product/blueprint.md), the ADR catalog under [docs/adr/](docs/adr/), and the threat model at [docs/security/threat-model.md](docs/security/threat-model.md) all remain authoritative; this file is project-local additions.

## CI monitoring (mandatory after every push)

After every `git push` to any branch, the agent MUST verify the status of the GitHub Actions workflows triggered by the pushed SHA. The session is NOT closed, the task is NOT declared complete, and no follow-on work begins until every workflow for that SHA has reached a terminal state and the overall verdict is `ALL GREEN` — or until a red workflow has been explicitly downgraded to *Known CI debt* in chat by Manuel.

**Terminal states:** `success`, `failure`, `cancelled`, `timed_out`, `skipped`.

### Mechanism — primary (when `gh` CLI is available)

```sh
gh run list --commit <SHA> --json name,status,conclusion,databaseId
# For any run still in_progress or queued:
gh run view <id> --json status,conclusion
```

Poll any non-terminal run at approximately 10-second intervals. Hard timeout per workflow: 10 minutes. If a run exceeds the timeout, surface the run URL to Manuel and ask how to proceed — do not silently continue.

### Mechanism — fallback (when `gh` is not available in the agent environment)

Use the GitHub REST API directly, with a bearer token retrieved from the local git credential helper:

```sh
TOKEN=$(echo -e "protocol=https\nhost=github.com\n" | git credential fill \
  | grep '^password=' | sed 's/^password=//')
curl -s -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/AlmanInDaHouse/CyberGuard_Enterprise/actions/runs?head_sha=<SHA>"
unset TOKEN
```

Same poll cadence (≈10 s) and hard timeout (10 min per workflow). Never log the token value.

### Report format after every push

```text
SHA <short-sha>
  <workflow-name-1> → <conclusion>     [run URL if failure]
  <workflow-name-2> → <conclusion>     [run URL if failure]
Verdict: ALL GREEN | FAILURES | IN PROGRESS
```

`IN PROGRESS` is acceptable only as an interim status during the poll loop. The final report after the poll must read either `ALL GREEN` or `FAILURES`.

### Hard rule on failures

If any workflow ends in `failure`, `cancelled`, or `timed_out`, the agent does NOT close the session, declare the task complete, or proceed to the next task until either:

1. The failure is diagnosed, fixed, and re-pushed until that workflow reports `success`, **or**
2. Manuel explicitly accepts the failure as known debt in chat, in which case the agent records it in the *Known CI debt* table below.

There is no third option. Assuming the failure is unrelated to the current change and ignoring it is not permitted.

`skipped` workflows (workflow not triggered by the path filters of the pushed commit) are reported as `skipped` and count toward `ALL GREEN`.

### Harness-first red phases and debt co-locality

When a commit will turn a workflow RED **by design** — the harness-first red phase, where acceptance-criteria tests land before the implementation — the *Known CI debt* declaration MUST live in the **same SHA** that turns the workflow red, not in a separate prior or follow-up commit. The spirit of the rule is that the red workflow and its debt entry are visible together at one commit: anyone inspecting that SHA sees both the failing run and the recorded, accepted reason.

Splitting them across commits to exploit path filters (e.g. landing the debt row in a docs-only commit that does not trigger the workflow, then landing the red tests separately) satisfies the letter but violates the spirit, and is not permitted. The implementation commit that turns the workflow green removes the debt row in that same SHA (already required above).

This was the implicit lesson of Sessions 6–7; it is codified here so future sessions inherit it.

## Local environment operations

The agent operates Manuel's local environment directly (not just the repository) for tools the project depends on. The scope, the package manager preference, the allowed operations, and the confirmation rules are below.

### Scope

The agent installs and configures ONLY tools that are declared prerequisites of CyberGuard. The list lives under *Approved local toolchain* below and is extended explicitly per session when a new ADR or SPEC introduces a dependency. Anything outside this list requires explicit chat confirmation.

### Approved local toolchain

| Tool | Reason | Introduced by |
|---|---|---|
| Task (go-task) | Project build runner per ADR-0001 §Decision. | Session 1 |
| Docker Desktop | Runtime for `infra/dev/docker-compose.dev.yml` per ADR-0003. | Session 4 |
| Rust toolchain (rustup, rustc, cargo, rustfmt, clippy) | Compiler and tooling for the `cg-agent` crate per ADR-0002 §Decision and SPEC-001. Pinned by `rust-toolchain.toml`. | Session 5 |
| Node.js 22 LTS | Runtime for the `services/ingest/` TypeScript service per ADR-0007 and SPEC-004. The Dockerfile and `ts-ci` pin Node 22; local dev tolerates ≥22 (verified on 24). | Session 8 |
| pnpm (via Corepack) | Package manager for the TypeScript workspace (`services/ingest/`, and the future `services/api/` + `dashboard/`) per ADR-0007. Activated with `corepack enable`; version pinned by `packageManager` in `package.json`. | Session 8 |

Future sessions will extend this table: Go toolchain when the event-firehose ingest begins; etc. A new row lands in the same commit as the ADR or SPEC that introduces the dependency.

### Package manager preference (Windows)

In order of preference:

1. **winget** — primary.
2. **scoop** — first fallback.
3. **chocolatey** — second fallback.
4. **Direct download from the tool's official GitHub release page** — last resort.

Never `iwr | iex` from non-official sources. Never `curl | bash` except from a URL documented on the tool's own official site.

### Operations allowed WITHOUT chat confirmation

- Install an *Approved local toolchain* entry via an official package manager (winget / scoop / choco) or via the tool's official GitHub release.
- Start Docker Desktop if it is installed but not running. Stop it deliberately if a task requires it.
- Verify versions and capabilities with `--version`, `--help`, or equivalent.
- Modify the **user** `PATH` to expose freshly installed binaries.

### Operations that ALWAYS require explicit chat confirmation

- Credentials of any kind: Docker Hub login, additional GitHub PATs beyond the one already configured, cloud provider auth, npm publish credentials, etc.
- Modifying **system** environment variables (HKLM-level on Windows; anything beyond the current user).
- Touching firewall, antivirus, or WSL2 configuration.
- Any installation outside the Approved local toolchain.
- Any command that writes outside the repository AND outside the standard package-manager paths.

### Reporting

After any local-environment operation, report:

- What was installed, configured, or changed.
- The exact command used.
- The verification that passed (`--version` output, `docker ps`, or equivalent).

### On failure

If an installation fails, report the full error and stop. The agent does NOT try alternative non-official sources on its own initiative.

## Decision authority

Technical, reversible, in-scope decisions are the agent's. The agent decides, applies, and communicates what was decided and why — it does NOT ask first.

### Decisions the agent takes WITHOUT asking (decide + communicate)

- Default values in dev configs: `.env.example` ports, compose defaults, healthcheck intervals, resource limits for dev.
- Tooling versions within the *Approved local toolchain*: pinning a minor version, choosing between equivalent images of the same project.
- File and directory structure consistent with existing conventions and ADR-0001.
- Commit message wording within the conventional-commits format already in use.
- Internal naming (variables, functions, types) consistent with the language conventions of ADR-0002.
- Healthcheck and test parameters in dev that do not change behaviour.
- Refactors that preserve external behaviour and pass the harness.
- Linter and formatter rule choices within the accepted tool defaults.
- Anything where the trade-off space is small and the harness, SPECs and ADRs constrain the answer.

### Decisions that STILL require Manuel's explicit OK (ask first)

- Product scope: what enters or leaves MVP, what gets deferred.
- Money: paid services, licences, recurring costs, paid tiers.
- Business-domain decisions that depend on information the agent does not have.
- Irreversible high-impact operations: force-push to `main`, history rewrites, dropping data, breaking public API contracts, deleting branches with unmerged work.
- Credentials, secrets, authentication setup.
- Installations requiring personal EULA acceptance (e.g. Docker Desktop first install).
- Anything outside the *Approved local toolchain*.
- New ADRs or amendments to accepted ADRs — the agent drafts and proposes; Manuel ratifies before status changes to `Accepted`.
- Schema-breaking changes to CGES once it stabilises.

### Communication contract

- When the agent takes an autonomous decision, it REPORTS in the same turn: what was decided, the alternatives considered (one line each, max two), and why this choice.
- If confidence on the right answer is below roughly 70%, the agent treats the decision as ask-first rather than decide-and-communicate.
- If a decision turns out wrong, the agent owns the rollback the same way it owned the decision. Report and fix.

### Decision reporting

When reporting autonomous decisions taken during a session, distinguish:

1. **Anticipated decisions** — choices made up front based on the briefing or SPEC, before implementation revealed issues.
2. **Reactive corrections** — changes made because a test or check failed, or because implementation revealed a SPEC gap.

Both are valid. (1) speaks to briefing or SPEC quality; (2) speaks to what reality surfaced. Bundling them loses signal.

### SPEC amendment workflow

When implementation reality contradicts an already-`Accepted` SPEC (or ADR) in a way that needs a contract change, amend it in place rather than rewriting history:

- Append an explicit `## Amendment <YYYY-MM-DD>: <short title>` section near the bottom of the SPEC (before `## References`), stating what surfaced the conflict, the amendment, and its effect (or lack of effect) on each affected section. The original requirement text stays; the amendment supersedes it where they differ.
- **Status stays `Accepted`**; bump `Last updated`. Summarise the amendment in the catalog (`docs/specs/README.md` / `docs/adr/README.md`) if one exists.
- **No re-ratification pause is required if the amendment was authorized in chat at the moment the conflict was surfaced** (the STOP that raised it *is* the ratification). If it was not, surface it and wait, like any ask-first decision.
- Prefer additive, backward-compatible amendments (a new optional field) so prior tests need no revision; call out explicitly when an amendment is *not* backward-compatible.

This was established when SPEC-004's marquee AC surfaced that the agent's single `server.url` could not address SPEC-004's two-port topology, amended into SPEC-003 (optional `server.heartbeat_url`).

### Stop conditions

The agent STOPS and reports to Manuel only for:

- Decisions in the *ask-first* list above.
- Failures whose root cause cannot be diagnosed with confidence.
- Repeated failure (third retry on the same target) suggesting a deeper issue.
- Anything that would require touching the host system beyond the *Local environment operations* scope (firewall, antivirus, WSL config, system-level env vars).
- Genuinely unexpected output the agent cannot interpret.

The agent does NOT stop for:

- Routine technical fixes within scope: port defaults, escape syntax, healthcheck timing, dev config tweaks.
- Lint or format errors the agent can fix.
- Mismatch between briefing and reality where the briefing was written without full info — adapt and report in the same turn.

## Known CI debt

Workflows that are currently red on `main` and that Manuel has explicitly downgraded to accepted debt. The table is the live state, not a history — when a debt is cleared (workflow back to green), the row is removed in the same commit that clears it.

| Workflow | Declared on SHA | Reason | Owner | Target SHA / date |
|---|---|---|---|---|
| `ts-ci` | `0c4d302` | SPEC-005 Phase 3.4 harness-first RED phase (TypeScript side). AC-001 polyglot marquee lands as `services/ingest/test/spec-005-marquee.test.ts` reusing the existing prepareAgent + startIngest + testcontainers scaffolding. Three-layer RED expected: (i) agent has no ETW code yet, (ii) ingest's OuterEnvelopeSchema in `src/schemas.ts` rejects envelopes carrying `events[]`, (iii) `cges_events` ClickHouse table does not exist yet (D6 PARTIALLY DONE). All three layers clear in Phase 3.5 as a coordinated implementation phase. `test/helpers/db.ts` extended with `getCgesEvents` accessor + `CgesEventRow` interface; the accessor compiles cleanly and runs once Phase 3.5 lands the DDL. Target: Phase 3.5 implementation commit(s) that land (i) the SPEC-005 envelope schema acceptance in `src/schemas.ts`, (ii) the cges_events ClickHouse migration, (iii) the events[] persistence path in the heartbeat handler; this row is removed in the same SHA that flips ts-ci to success. | Architect-Claude (planning), Claude Code (implementation) | Phase 3.5 SHA TBD |

When adding an entry, also link to the relevant memory (e.g. `[[project-pending-...]]`) or the chat decision so the rationale is retrievable later.
