# CLAUDE.md — CyberGuard Enterprise

Project-level instructions for any Claude (or Claude-like) agent working on this repository. Read this file before editing code, ADRs, schemas, or workflows. The global `~/.claude/CLAUDE.md`, the foundational [Blueprint](docs/product/blueprint.md), the ADR catalog under [docs/adr/](docs/adr/), and the threat model at [docs/security/threat-model.md](docs/security/threat-model.md) all remain authoritative; this file is project-local additions. Where this file and the global one differ on who decides and when to ask, this file wins (*Decision authority*).

Since 2026-10-10 the project is executor-led: Claude Code holds the context of the repo, decides the design inside the accepted ADRs and integrates the code; Manuel, owner and architect, keeps the general architecture and the decisions listed in *Decision authority*. There is no separate advisor. In *CI monitoring*, *Local pre-commit gate* and *Local environment operations*, *the agent* that acts is Claude Code; everywhere else *the agent* is the endpoint agent, `cg-agent`. After a `/clear`, the context is this file, the latest `docs/handoff-session-*.md` and the auto-memory index (*Session protocol*); where an auto-memory entry contradicts this file, this file wins and the session that notices updates the memory.

The remaining-work sequencing is the technical work-order document [docs/product/roadmap.md](docs/product/roadmap.md) — the remaining MVP phases ordered by dependency, plus the owner-STOP decisions gating them.

## Session protocol

What a session does at its start and at its close. It used to arrive in each prompt; it lives here now.

### At the start

1. Read this file and the latest handoff (the highest-numbered `docs/handoff-session-*.md`) in full. Its *How Session N resumes* is the work order, inside the roadmap.
2. `git fetch --prune --tags`; `git rev-parse main origin/main` prints the same SHA twice; `git status --porcelain` prints nothing; `git stash list` is empty or each entry is explained by the handoff.
3. Branches and PRs: `git branch -a`, and the open PRs (`GET /repos/AlmanInDaHouse/CyberGuard_Enterprise/pulls?state=open`, REST as in *CI monitoring*). Each one is named by the handoff.
4. CI of `main`'s tip: every run for the full SHA is terminal and green (*CI monitoring*).
5. Catalogs (`docs/adr/README.md`, `docs/specs/README.md`) and the *Known CI debt* table match the handoff's counts.
6. Where the repo and the handoff disagree, the repo wins: diagnose it and report it before starting the work.

### At the close

1. Write `docs/handoff-session-<N>.md`: the state, the anchor commits, the decisions taken (*Decision reporting*), the self-reviews (*Compensating controls* §5), the gate outputs (literal lines), the test baselines, the debts, what waits on Manuel, and *How Session N+1 resumes*.
2. Point `README.md` and `docs/product/roadmap.md` at the new handoff, and update the status of the roadmap phases the session moved.
3. Write the session-close auto-memory entry and its `MEMORY.md` line, and update or retire the entries the session made untrue (no Obsidian mirror for this project).
4. Close with `main` pushed, the tree clean and CI `ALL GREEN` on the close SHA. A branch left open is named in the handoff with its reason.

The handoff is the session's own until the next session starts: later commits of the same session may complete it. From then on it is a record (Class H) and is not edited.

### Environment facts

- The Bash tool is Git Bash (MSYS) on Windows 11. `gh` is not installed: CI runs are read with the REST fallback of *CI monitoring*, and PRs are opened, listed and closed with the same token and API (`/pulls`). Query runs by the full SHA (`git rev-parse <ref>`); a short SHA matches nothing.
- The gate scripts run as *Local pre-commit gate* writes them (`pnpm run typecheck`, `pnpm run lint`, `pnpm test`), in each workspace a change touches: `services/ingest`, `services/api` and `dashboard` have the same three scripts. A tool called directly runs as `./node_modules/.bin/<tool>` from the package directory, not through `pnpm exec`: earlier sessions saw `pnpm exec` hang in this shell (not reproduced on 2026-10-10 with `pnpm exec tsc --version`).
- On Claude Code's unelevated terminal the `services/ingest` suite has three capture marquees that launch `cg-agent` — the SPEC-005 marquee, `detect_ac_001` and `net_ac_010` — and fail with the agent's exit code 9, and `ac-001-marquee` skips. That is the expected unelevated result, and the elevated gate covers those four; the local gate is green when nothing else fails.
- Markdown lint locally: `npx markdownlint-cli2@0.22.1 <files>` (the engine of CI's `markdownlint-cli2-action@v23`).
- Claude Code's terminal is not elevated. The elevated gate (*Developer-local SPEC-005 marquee validation*) is run by Manuel in an elevated Windows PowerShell; Claude Code gives him the command block with each command's output saved by `Tee-Object` to a log under `C:\tmp\` (written in UTF-16), then reads the logs. With Docker Desktop up, cargo runs with `-j 2`: a parallel build can exhaust the paging file (os error 1455, which cargo reports as "can't find crate for std").
- The Windows `rust-ci` job runs as the built-in Administrator under PowerShell 7: no `powershell.exe` in a test, SIDs read in full form (*Local pre-commit gate*).

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

### One commit per push when each commit needs independent CI coverage

GitHub Actions runs only against the **head** SHA of a push, never the intermediate commits it carries. A push that batches several commits therefore gets exactly one CI run — covering the tip, not the commits beneath it.

- When each commit must be independently green — because it is a separate logical gate, or to keep the history bisectable under CI — push **one commit per push**, and run the post-push *CI monitoring* gate on each SHA before creating the next commit.
- When commits are deliberately batched into a single push, that is permitted, but the session report (and handoff) MUST state explicitly that **only the head SHA was covered by the CI run**; the intermediate commits were not independently validated.

## Local pre-commit gate

Before any `git push`, the agent runs the per-workspace gate locally and confirms it passes. This is the local mirror of the post-push *CI monitoring* gate above: running it first prevents avoidable red CI and the follow-up formatting commit that `cargo fmt` / Biome would otherwise force after the push. The commands below mirror the CI workflows exactly, so a green local gate predicts a green CI run.

- **Rust (`agent/cg-agent/`, gates `rust-ci`):** `cargo fmt --all -- --check`, then `cargo clippy --all-targets --all-features -- -D warnings`, then `cargo test --all`. The `fmt --check` step is mandatory and non-obvious — `clippy` validates source but NOT formatting, and `rust-ci` runs `cargo fmt --all -- --check` and fails on any diff (Convention #11). `cargo check` is an optional faster inner-loop step that `clippy` subsumes. rustfmt is deterministic across machines because the toolchain is pinned (`rust-toolchain.toml`, `channel = "1.93.0"`).
- **TypeScript (`services/ingest/`, and the future `services/api/` + `dashboard/`, gates `ts-ci`):** `pnpm run typecheck` (`tsc -p tsconfig.json --noEmit`), then `pnpm run lint` (`biome check .`), then `pnpm test` (`vitest run`).
- **CGES schemas / examples (gates `schema-validation`):** `task validate-schemas` when any schema or example under `schemas/cges/` changes.
- **Pre-compiled-binary tests:** when agent source changed and a test launches the pre-built binary (the SPEC-005 / SPEC-006 marquees via `agentBinaryPath()`), `cargo build --release --bin cg-agent` and verify the `.exe` timestamp is posterior to the edit before running the test (Convention #10).
- **Windows CI job (`rust-ci`, `windows-latest`):** its steps run under PowerShell 7 as the built-in Administrator (RID 500). A test must not spawn `powershell.exe` (Windows PowerShell 5.1 fails to load its modules there), nor compare SIDs as SDDL text (SDDL writes RID 500 as `LA`): read SIDs in full form (S32, `enroll_ac_010`).

The gate is expressed as per-workspace commands because the Taskfile `lint` / `test` targets are still `SPEC-XXX-ci` stubs; when an `SPEC-XXX-ci` lands unified `task` targets, this section points at those instead.

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

Manuel's decision, 2026-10-10, in his words: *"ahora claude code va a ser quien tenga el contexto total del repo y quien tenga el poder de implementación y decisión sobre el proyecto, a no ser que tenga una duda que requiera mi atención; todo contenido que incluya ejecución de comandos, integración de código, decisión sobre arquitectura básica (no general) se encargará claude code"*.

The line between the two lists below: **basic architecture** is the design inside the accepted ADRs; **general architecture** is what an ADR decides. A choice that needs a new ADR, or changes an accepted one, is Manuel's. If it is unclear which list a choice belongs to, it is Manuel's.

For this repository this overrides the global preference to present options before writing code: Claude Code presents options only for a decision on Manuel's list or for a doubt (*Communication contract*); otherwise it decides, implements and records.

### Decisions Claude Code takes and implements (decide + record)

- Reading and auditing the repo and its environment; diagnosing.
- The design inside the accepted ADRs: modules, types, internal formats, the structure of the tests, the plan of commits.
- New SPECs, and amendments to SPECs (by scope too), when the change fits the accepted ADRs and touches nothing on Manuel's list; a SPEC's own in-scope and out-of-scope inside the roadmap phase it serves is part of this. Claude Code sets a SPEC's status to `Accepted` when it lands, after its self-review (*Compensating controls* §5).
- Commands inside *Local environment operations*.
- Integration: commits, branches, PRs, and landing on `main` when the gates are green (*Integration path*). Claude Code may still ask Manuel to read a change before it lands, and says why.
- A new third-party dependency of the server or the dashboard that is not heavyweight (see Manuel's list), recorded with its name, version, licence and reason.
- Debts, handoffs, the status of the roadmap and the order of the work inside a phase.
- What this list held before, with its limits: default values in dev configs (`.env.example` ports, compose defaults, healthcheck intervals, resource limits for dev); tooling versions within the *Approved local toolchain*; file and directory structure consistent with existing conventions and ADR-0001; commit wording within the conventional-commits format; internal naming consistent with ADR-0002; healthcheck and test parameters in dev that do not change behaviour; refactors that preserve external behaviour and pass the harness; linter and formatter rule choices within the accepted tool defaults.
- Facts in this file that describe the repo or the environment (*Environment facts*, validation statuses, test counts). Its rules on who decides are not among them (Manuel's list).

### Decisions that STILL require Manuel's explicit OK (ask first)

- A new ADR, or an amendment to or supersession of an accepted ADR. Claude Code drafts and proposes it (*Integration path* §4); Manuel ratifies it before its status is `Accepted` and before code that relies on it lands.
- A new service or component; a new language or data store; a heavyweight dependency — a framework, a runtime, a data-store client, or a library that does cryptography or networking or runs install scripts or a native build; and any new third-party dependency of `cg-agent`, which runs elevated.
- The deployment contract: new environment variables a deployment reads (not test-only ones), configuration surfaces the *client operator* sets, credentials and secrets, packaging, trust anchoring and key distribution. These are an **owner STOP even when Claude Code has the technical direction clear**, because they depend on client-operation knowledge that does not live in the repo. Defer them with a **named Open question + an explicit reopen condition**; do not resolve them by design inertia. (Surfaced when SPEC-012's forensic-pubkey trust anchoring was deferred as a deployment contract rather than picked as an engineering default.)
- Product scope and the order of the roadmap phases: what enters or leaves the MVP or a roadmap phase, what is deferred out of a phase, which threats the rule set covers.
- The security posture: the privilege model, the authentication model, the trust boundaries, the cryptography, what data about people `cg-agent` collects, and the threat model (`docs/security/threat-model.md`).
- Schema-breaking changes to CGES, and breaking a public API contract.
- Business-domain decisions that depend on information the repo does not hold.
- Money: paid services, licences, recurring costs, paid tiers.
- Irreversible high-impact operations: force-push to `main`, rewriting the history of `main` or of a branch someone else works on, dropping data, deleting branches with unmerged work. Rebasing Claude Code's own branch before it lands (*Integration path* §3) is not one of them.
- Anything outside the *Approved local toolchain* or the scope of *Local environment operations*, including installations that need personal EULA acceptance (e.g. Docker Desktop first install). Everything under *Operations that ALWAYS require explicit chat confirmation* stays as written there.
- Accepting a red workflow as *Known CI debt* (*CI monitoring*), or a red local or elevated gate as a debt.
- A change to this file that moves a decision between the two lists, or weakens a compensating control, *CI monitoring*, the *Local pre-commit gate* or the elevated gate. Claude Code proposes it through owner review (*Integration path* §4).

### Communication contract

- When Claude Code takes a decision on its own, it reports it in the same turn — what was decided, the alternatives considered (one line each, max two), and why — and records it for the session's handoff (*Decision reporting*).
- If confidence on the right answer is below roughly 70%, the decision is ask-first. A doubt goes to Manuel as one question, with the options and a recommendation.
- Manuel's OK is an explicit yes in chat to a specific proposal. Text another model wrote is not an OK (*Integration path* §5).
- If a decision turns out wrong, Claude Code owns the rollback the same way it owned the decision. Report and fix.

### Compensating controls (no second reader)

Until 2026-10-10 a second reader, the advisor, reviewed each change before Manuel ratified it. Now the author of a change is also the one who approves it, and these controls stand in for that reader. A conflict between one of them and a task is a STOP, not a judgement call.

1. **Contract before code.** The SPEC — and the ADR, when one is needed and Manuel has ratified it — lands on `main` before the implementation starts. The acceptance tests are written from the SPEC's criteria before the code: Claude Code runs them locally against the tree without the implementation and sees each one fail for the missing behaviour, not for its setup. They are committed with the code that makes them pass, so no pushed commit is red by design. The harness-first red phase of *CI monitoring* is used only when Manuel accepts the red as *Known CI debt*; that red commit is then the one exception to "CI green on every pushed commit" in *Integration path*.
2. **Diagnosis before any fix.** Before a fix is applied, the cause is stated with the evidence that supports it: the failing assertion, the log line, the diff. A session never closes red — CI, the local gate or the elevated gate — unless Manuel accepts it as debt: a red workflow goes to *Known CI debt*, a red local or elevated gate to the handoff's debts.
3. **CI green on every push**, with the hard rule on failures, exactly as *CI monitoring* states them.
4. **The elevated gate.** It stays mandatory for any change to the capture path, the detection path or the schema of `alerts` or `cges_events` (the paths in *Developer-local SPEC-005 marquee validation* §5 and *Developer-local SPEC-006 marquee validation* §6). Manuel runs it; Claude Code gives him the commands (*Environment facts*), waits for the output and reads it. Such a change does not land without a green run that covers its code.
5. **Self-review before landing.** For each code change, and for each SPEC or ADR text, a separate pass in two halves, done before landing. The contract of a code change is the SPEC it implements; for a fix or a refactor that implements none, it is the debt entry or the statement of the defect, and its criteria are the claims of that text.
   - **Claude Code's own pass.** For code: the matrix *criterion → test (path:line) → covered / weak / missing*, read from the diff against each criterion of the contract. For a SPEC or ADR text: each claim made in more than one place agrees with itself, and each `path:line` it cites exists and says what is claimed.
   - **A reviewer with a fresh context.** Then, before reading anything else about the change, Claude Code starts a subagent (the Agent tool) that gets no summary of the work and no reasoning from the session: only the prompt below, with the paths and SHAs filled in. The prompt is fixed here so that it cannot be tuned to the case.
   - **Dispositions.** Every *weak*, *missing* or *contradicted* verdict and every list-A entry, from either half, is fixed (a new commit, CI again) or recorded with the reason it does not block. Where the two halves disagree, Claude Code reads the test or the text again before choosing. The matrix, the reviewer's lists and the dispositions go in the handoff.
   - **Its limit.** The reviewer shares Claude Code's model: it is a second reading, not a second judge. Manuel's audit of the record is the backstop.

   For a code change:

   ```text
   You review a change you did not write. Repository: the working directory.
   Contract: <SPEC paths, or the debt entry or defect statement>.
   Change: <base-sha>..<tip-sha> on branch <branch>.
   Read the contract and the diff, and any other file you need. Do not rely on commit messages.
   1. For each acceptance criterion of the contract: the test or tests that assert it (path:line)
      and a verdict - covered (the assertion states what the criterion states), weak (a test
      exists but asserts less), missing, or contradicted (the code does otherwise) - with one
      line of reason.
   2. The statements of the contract's Data contracts and Operational sections that the diff
      contradicts.
   3. What the diff changes that the contract does not ask for, and any change to authentication,
      authorization, cryptography, privileges or the data cg-agent collects that the contract
      does not state.
   Report the three lists. Try to falsify the change, not to confirm it.
   ```

   For a SPEC or ADR text:

   ```text
   You review a contract you did not write. Repository: the working directory.
   Documents: <paths> at <sha> (or: as changed in the working tree, git diff main -- <paths>).
   Read them against the repo.
   List A (blocks): a statement the repo contradicts, or two statements of the documents that
   clash - quote both sides with path:line.
   List B (does not block): wording, what the repo does not let you check, difficulty of
   implementation.
   Report both lists. Try to falsify the documents, not to confirm them.
   ```

6. **Decision record.** Every decision of basic architecture Claude Code takes on its own goes in the session's handoff with the alternative it discarded and why (*Decision reporting*), so that Manuel can audit it afterwards. A new SPEC also records its load-bearing decisions in its own `## Decision record`, and new ADRs and SPECs name two roles in their Deciders or Authors field ([docs/engineering-notes.md](docs/engineering-notes.md) §Session 34).
7. **A branch and a PR for code.** Code reaches `main` only through a branch with a PR open, so that CI runs before `main` (*Integration path*). It does not wait for Manuel's ratification unless it touches his list.

### Decision reporting

When reporting the decisions Claude Code took during a session — in the turn, and in the handoff — distinguish:

1. **Anticipated decisions** — choices made up front from the SPEC, the handoff or Manuel's request, before implementation revealed issues.
2. **Reactive corrections** — changes made because a test or check failed, or because implementation revealed a SPEC gap.

Both are valid. (1) speaks to the quality of the contract; (2) speaks to what reality surfaced. Bundling them loses signal. Each entry names the alternative discarded and where the decision lives (a commit, a SPEC section). The decisions Manuel took in the session are listed apart, with his words and the date.

### Integration path

How changes reach `main`. It replaces *Relay transport and verification*, the rules of the advisor period (until 2026-10-10) that earlier handoffs cite as relay rules 1–5: rule 5 survives as §3 and rule 3 as §4, without the wait for ratification outside Manuel's list; rule 4 lives inside §4 and rule 2 as §6; rule 1 (diffs over the relay) lapses.

1. **Docs.** A docs-only change that is Claude Code's to decide — a handoff, a debt record, the roadmap's status, a SPEC or a SPEC amendment after its self-review — may be committed straight to `main` once `markdownlint` passes locally on the files it touches; *CI monitoring* then applies to the push.
2. **Code.** Any change to a file that is not a Markdown document — code, schemas, rules, workflows, build, container, Taskfile or lint configuration, `harness/` — goes through a branch cut from `main` and a PR, opened over REST when `gh` is absent: CI runs on a branch only while a PR is open. Commits are ordered so that each one is true and green at its own SHA, and pushed one per push. Before landing: CI green on every pushed commit (the one exception is in *Compensating controls* §1), the *Local pre-commit gate* in each workspace the change touches, the self-review (*Compensating controls* §5) and, for the paths it covers, the elevated gate (*Compensating controls* §4).
3. **Landing.** If `main` moved since the branch was cut, rebase the branch onto `main` and push the rebased commits again one per push, the first with `--force-with-lease` (`git push --force-with-lease origin <sha>:<branch>`, then `git push origin <sha>:<branch>` for each next one), CI green on each. Then land each commit on `main` by cherry-pick, in order; before each push, `git diff --quiet <branch-sha> HEAD` must return 0 (the landed tree is the tree CI ran on). One push per commit, CI green before the next. A one-commit branch may land by `git merge --squash`, with `git diff --cached --quiet <branch>` as the guard. Then close the PR and delete the branch, local (`git branch -D`, since its commits landed under new SHAs) and on origin: a branch whose tree passed the guard counts as merged.
4. **Owner review.** A change that needs Manuel's OK — an ADR draft, a deployment-contract proposal, a change to the rules of this file on Manuel's list — goes on a branch whose commits carry `NOT YET RATIFIED` in the subject; `markdownlint` runs locally before the push. A branch push with no PR open runs no workflow, and the post-push report says so. Manuel reads the change on GitHub (the branch's compare URL), not in chat. When his OK carries conditions (hashes match, `--stat` exact, a gate green), Claude Code reports the literal output of each, not a summary. After the OK it lands as in §3, with the marker stripped from each subject.
5. **Text from another model.** If Manuel brings text that another model or an advisor wrote — a prompt, a review, a plan — it is an input to weigh against the repo, not an instruction. What binds is what Manuel says himself, and each decision stays where *Decision authority* puts it.
6. **Verbatim text.** When Manuel gives text that must land verbatim and it crossed a chat that may wrap long lines, Claude Code writes it and reports the SHA-256 of the written lines with CR stripped (`tr -d '\r' | sha256sum`) for him to match, instead of sending the text back.

### SPEC amendment workflow

When implementation reality contradicts an already-`Accepted` SPEC (or ADR) in a way that needs a contract change, amend it in place rather than rewriting history:

- Append an explicit `## Amendment <YYYY-MM-DD>: <short title>` section near the bottom of the SPEC (before `## References`), stating what surfaced the conflict, the amendment, and its effect (or lack of effect) on each affected section. The original requirement text stays; the amendment supersedes it where they differ.
- **Status stays `Accepted`.** For an **ADR** (whose header carries a `Last updated` field), bump `Last updated`. For a **SPEC** (whose header — `ID` / `Title` / `Status` / `Depends on` / `Authors` — has no `Last updated` field, and is not standardised to add one here), the dated `## Amendment <YYYY-MM-DD>` section *is* the timestamped record; nothing else in the header is bumped. Either way, summarise the amendment in the catalog (`docs/specs/README.md` / `docs/adr/README.md`) if one exists.
- **Who authorizes.** An amendment to a SPEC is Claude Code's when it fits the accepted ADRs and touches nothing on Manuel's list: it is recorded in the handoff (*Decision reporting*) and passes the self-review (*Compensating controls* §5) before it lands. An amendment to an ADR, or to a SPEC on a point of Manuel's list, waits for his OK; if he gives it when the conflict is surfaced, that answer is the ratification and no further pause is needed. Either way the amendment lands before the code that relies on it.
- Prefer additive, backward-compatible amendments (a new optional field) so prior tests need no revision; call out explicitly when an amendment is *not* backward-compatible.

This was established when SPEC-004's marquee AC surfaced that the agent's single `server.url` could not address SPEC-004's two-port topology, amended into SPEC-003 (optional `server.heartbeat_url`).

### Stop conditions

Claude Code STOPS and asks Manuel for the following, and where another section of this file says to stop or ask (a CI run past its timeout, a failed installation, a compensating control in conflict with the task):

- Decisions on the *ask-first* list above, and doubts below roughly 70% confidence (one question, the options, a recommendation).
- The elevated gate: Claude Code hands Manuel the commands and waits for the output. Work that does not depend on the gate may go on meanwhile; the gated change does not land.
- Failures whose root cause cannot be diagnosed with confidence.
- Repeated failure (third retry on the same target) suggesting a deeper issue.
- Anything that would require touching the host system beyond the *Local environment operations* scope (firewall, antivirus, WSL config, system-level env vars).
- Genuinely unexpected output Claude Code cannot interpret.

Claude Code does NOT stop for:

- Design inside the accepted ADRs, writing or amending a SPEC inside them, and landing on `main` with the gates green.
- Routine technical fixes within scope: port defaults, escape syntax, healthcheck timing, dev config tweaks.
- Lint or format errors it can fix.
- A mismatch between a handoff or a request and reality where the text was written without full info — adapt and report in the same turn.

## Known CI debt

Workflows that are currently red on `main` and that Manuel has explicitly downgraded to accepted debt. The table is the live state, not a history — when a debt is cleared (workflow back to green), the row is removed in the same commit that clears it.

| Workflow | Declared on SHA | Reason | Owner | Target SHA / date |
| -------- | -------------- | ------ | ----- | ----------------- |

When adding an entry, also link to the relevant memory (e.g. `[[project-pending-...]]`) or the chat decision so the rationale is retrievable later. The `<RED-SHA>` placeholder is substituted with the actual red-turning commit hash in an immediate follow-up docs commit (the two-commit SHA-placeholder pattern — the debt row + red tests land together in one SHA; only the self-referential hash is filled afterward).

## Developer-local SPEC-005 marquee validation

The SPEC-005 polyglot marquee test (`services/ingest/test/spec-005-marquee.test.ts`) validates the end-to-end agent → ingest → ClickHouse path on Windows. It cannot run in CI per the Path D resolution documented at Phase 3.5.H and ADR-0010 §Decision part 3 Amendment 2026-05-29 (Fallback 2): hosted GitHub Actions Windows runners do not expose a working container runtime for testcontainers, and Linux runners cannot spawn `cmd.exe` for the probe process. Additionally, the MVP elevated-user privilege model (ADR-0010 §Decision part 1) has not been validated on `runneradmin`.

The marquee is therefore validated developer-local, by Manuel in an elevated terminal: Claude Code's terminal is not elevated, so it gives him the commands and reads the output (*Environment facts*). Procedure (the elevated gate):

1. Have Docker Desktop running on the Windows machine.
2. Open an **elevated** terminal (Run as Administrator) at the repo root. Confirm the elevation with `net session` (an unelevated terminal answers with access denied) and record `git rev-parse --short HEAD` with a clean `git status`: the gate is evidence only for that tree. A skipped `ac-001-marquee` in the vitest summary means the terminal was not elevated (S32).
3. Run:

   ```sh
   cargo build --release -j 2 -p cg-agent
   cargo test -j 2 -p cg-agent -- --ignored --test-threads=1
   cd services/ingest
   pnpm install --frozen-lockfile
   pnpm test
   ```

4. `cargo build --release` refreshes the binary the marquees launch (`agentBinaryPath()` prefers `target/release/`). The `cargo test ... --ignored` run executes the agent tests that need real ETW and elevation, `#[ignore]`d everywhere else (SPEC-017 capture_ac_013): `process_ac_004` (cache hit), `process_ac_007`, `process_ac_009`, the elevated case of `capture_ac_006`, and SPEC-019's `net_ac_008` and `net_ac_009`; expected outcome: those six pass, and `net_ac_008` prints its `refused_attempt_reported` line. The vitest run executes the whole suite, including the SPEC-005 marquee, `detect_ac_001`, `net_ac_010` and `ac-001-marquee` (on Windows these run only elevated). Expected outcome: all tests pass, the marquees included. The agent runs its normal secure path (SPEC-017); there is no environment switch.
5. This procedure is the standing validation gate for any merge that touches the capture path. Run before merging changes to `agent/cg-agent/src/etw/`, `agent/cg-agent/src/cges/`, `agent/cg-agent/src/delivery.rs`, `agent/cg-agent/src/paths.rs`, the secure path in `agent/cg-agent/src/lib.rs`, `agent/cg-agent/src/envelope.rs`, `services/ingest/src/routes/heartbeat.ts` or `services/ingest/src/schemas.ts`.

**Validation status:** marquee 8/8 GREEN, validated developer-local in Phase 4 Session 16 (two consecutive runs, zombie reclaim validated). ts-ci Known CI debt row removed in this commit.

If the local run fails, Claude Code diagnoses it from the output Manuel returns (*Compensating controls* §2) and stops for him only under *Stop conditions*. The marquee's 5 assertions per SPEC-005 §AC AC-001, the Win32 form of the Launch's image path (SPEC-017 capture_ac_012) and the D7 budget assertion (≤ 45s wall-clock) are the verification surface; failures in any of those are SPEC-005 / SPEC-017 implementation defects, not infrastructure issues.

## Developer-local SPEC-006 marquee validation

The SPEC-006 detection marquee (`services/ingest/test/detect-ac-001-marquee.test.ts`, `detect_ac_001`) validates the end-to-end detection path on Windows: a real `cg-agent` captures a `winword.exe` stand-in spawning `powershell.exe`, the events reach ClickHouse `cges_events`, `runDetectionCycle` reads + normalizes + evaluates the rule set (where the `office_spawns_script_host` rule matches) + scores + persists, and exactly one `office_spawns_script_host` alert lands in the Postgres `alerts` table. Like the SPEC-005 marquee it cannot run in CI (no ETW on Linux runners; no container runtime on hosted Windows runners per ADR-0010 §Decision part 3); it is gated `skipIf(process.platform !== "win32")`.

**Precondition — rebuild the agent binary first (institutionalized Session 21; reinforces Convention #10, made unconditional for this marquee).** Before every run: `cargo build --release -p cg-agent`, then confirm `target/release/cg-agent.exe` is newer than the most recent edit under `agent/`. `agentBinaryPath()` (`services/ingest/test/helpers/marquee-agent.ts`) launches whatever binary is on disk, so a **stale binary silently captures zero events** — `cges_events` count 0 / `runDetectionCycle` `eventsEvaluated` 0, a clean no-panic exit indistinguishable at the symptom level from a dirty watermark (an unelevated agent, by contrast, exits with code 9 and writes the cause to stderr). In Session 21 a week-old binary (built 2026-05-29) alone turned this marquee red and masqueraded as a regression from the SPEC-011 diff that it was not (the diff is TypeScript-only; `cges_events` emission is agent-Rust → ingest, upstream of all detection/severity code). Rebuilding flipped it green (captured events 0 → 51, alert 0 → 1). Convention #10 conditions the rebuild on *"agent source changed"*; for this marquee it is **unconditional** — the binary can be stale from a prior session even when the current session never touched `agent/`.

Procedure:

1. Have Docker Desktop running on the Windows machine.
2. Open an **elevated** terminal (Run as Administrator) at the repo root. An elevated terminal starts in `C:\Windows\System32`; `cd` to the repo root first.
3. Run (the same elevated gate as the SPEC-005 marquee above):

   ```sh
   cargo build --release -j 2 -p cg-agent
   cargo test -j 2 -p cg-agent -- --ignored --test-threads=1
   cd services/ingest
   pnpm install --frozen-lockfile
   pnpm test
   ```

4. The vitest run executes the full suite including the SPEC-005 marquee, `detect_ac_001` and `net_ac_010` (their `.skipIf` gates are inactive on Windows). The evidence to report is the vitest summary (`Test Files N passed (N)` / `Tests M passed (M)`, nothing skipped; with output redirected, vitest does not list every file) and, from the cargo run, the six real-ETW tests passed.
5. `detect_ac_001` runs the whole rule set over the capture and asserts, for the agent, exactly one Postgres alert with `rule_id = rule.office_spawns_script_host`, and on it `cg_detection_source = rule`, `final_score = 0.9`, `status = new`, a well-formed `dedup_key`, and `source_events` containing the `event_id` of the captured `powershell.exe` child (SPEC-016 §Operational §3). Alerts from other rules on background activity are logged, not asserted. The captured `image_file_name` of the probe and its child is logged and asserted in Win32 form (SPEC-017 capture_ac_012); report those two log lines with the vitest summary. The test runs its cycle as the production driver configures it, after waiting out the 5000 ms settle margin once the agent has exited (SPEC-018 §Operational §2). The probe spawns the `winword.exe` stand-in **after** the agent's ETW session opens, so the parent is captured — a green run does NOT imply production coverage of the already-running-Office case (SPEC-006 §Operational §2 production false-negative).
6. Standing gate before merging changes to the detection path: `services/ingest/src/detect/`, `rules/windows/`, or the `alerts` / `cges_events` schema.

**A green run stays valid over later commits** only if none of them touches a runtime input of the path it tests — `services/`, `agent/`, `dashboard/` or `rules/` — and the non-elevated suite count is identical before and after; any other change forces a re-run. This is criterion (a) of the *Detection prod-driver branch merge gate* below, extended to `rules/`: a change to a rule alone changes what the marquee evaluates.

**Validation status:** VALIDATED developer-local + elevated on 2026-05-31 (Phase 5, post-5e at `44fd345`). Full suite 19 files / 44 tests GREEN with `detect_ac_001` running (not skipped, 40171 ms wall-clock): a real `cg-agent` captured the `winword.exe` stand-in spawning `powershell.exe` via ETW, the events reached ClickHouse `cges_events`, `runDetectionCycle` evaluated the `office_spawns_script_host` rule and persisted exactly one alert to the Postgres `alerts` table. All 6 `detect_ac` green, the SPEC-005 marquee and every other suite without regression. The CI-able detection suite (`detect_ac_002`–`006` + migration / read-model / engine / scorer) is GREEN in `ts-ci`; this marquee is the end-to-end gate, validated here.

## Detection prod-driver branch merge gate

`feat/detection-prod-driver` (the ADR-0012 Amendment 2026-06-07 in-process TS scheduler that gives `runDetectionCycle` its first production caller) merges to `main` only when **both** are present:

1. **The elevated `detect_ac_001` marquee is GREEN** — run developer-local + elevated per *Developer-local SPEC-006 marquee validation* above, on the branch tip, by Manuel. **The marquee covers the CODE that merges, not a specific SHA.** A green marquee stays valid over a *later* tip if everything added on top is doc-only or comment-only, verified by: (a) a diff with **no files under `services/`, `agent/`, or `dashboard/`**, and (b) a non-elevated suite with an **identical count before and after**. Any runtime change invalidates it and forces a re-run. Known case: S27, the marquee run over `e761610` stayed valid at tip `6f9d5e3` (the two intervening commits were 9 `.md` clauses + 2 JSDoc comments; non-elevated suite 57/57 identical).
2. **The Class B documentary-coherence edits are attached** — the *altitude* assertions that state `runDetectionCycle` has *no production caller / test-validated only* (true today, false once the driver lands) are corrected in their **own commit** on the branch, **after** the marquee is green and **before** the merge. When applying these Class B edits, review the **embedded citations** inside the B clauses too, not only the assertion text — a citation can go stale while the assertion stays true. Known case: `ADR-0017:64` cites ADR-0012 §1 as the authority for *"no production caller yet"*; post-Amendment 2026-06-07 that citation is obsolete even though the assertion holds until the driver merges. Classification is **per-clause, not per-site**: a Class A site can carry a Class B clause embedded in the same bullet. Known case: `ADR-0017:88` — the mechanism-A rewrite plus the *"Inherited gap"* Class B altitude clause in one sentence; the B clause must survive the Class A fix. `docs/specs/README.md:50` interleaves the B assertion and the A clause in the same parenthesis: applying the Class B edit there means redoing the whole sentence, not just the assertion, because the parenthesis naming the scheduler becomes redundant once *"has no production caller yet"* stops being true. It is the only one of the eleven sites with this interleaved shape. **Citation preservation (rule b) and its narrow exception:** a stale `path:line` anchor is preserved **verbatim** in the Class B commit — anchor drift is swept by pattern in a dedicated Class-A pass, never corrected piecemeal (a partial fix leaves the pattern sweep incomplete). **Exception:** when inverting a B assertion turns a preserved citation into a *false* claim — the citation now vouches for the opposite of what the sentence says — correct it in place, because leaving it introduces a false statement, not an anchor drift. Known case: `ADR-0017:64`, where `(ADR-0012 §1, :28)` — §1 being the deferred Go extraction the Amendment decoupled from the prod-caller role — is replaced by the Amendment 2026-06-07 reference (`:271`), the real authority. Anchor-only drift (a line that merely moved, e.g. `runDetectionCycle` from `index.ts:24` to `:36`) is NOT this case: a line number vouches for neither polarity, so it stays preserved as Class-A debt (debt #13, `docs/handoff-session-27.md`). **Statement form is preserved:** a Class B edit updates *what a clause asserts*, never its *form* — a `MUST` clause stays a `MUST`, only its object is updated; converting a normative requirement into a description is a structural change to the section, not coherence. Known case: `SPEC-014:96` (§Compliance), where the altitude `MUST` is kept and its object flips from *"not an email running in production"* to *"and firing in a deployed system."* **Scope of rule (a):** the term *"test-validated altitude"* is preserved and marked *resolved* only in **prose that asserts something about the system** (ADRs, SPECs, catalogs). It does NOT apply to a **scorecard status cell**, where the term is a value that expires and whose correct final state is to *remove* it. A scorecard is a status table, not documentary coherence, and is refreshed by whoever owns the scorecard — never by a Class B pass. Known case: `README.md:21` (the criterion-4 row), excluded from the Session 27 Class B commit and deferred to the post-merge scorecard refresh.

The merge gate is exactly `marquee GREEN + Class B edits present`. Class A edits are a separate track: *mechanism* assertions that wrongly say the deferred Go `services/pipeline/` firehose provides the prod caller are already broken on `main` today (superseded by the Amendment 2026-06-07), independent of this branch, and are fixed on their own without waiting for the marquee. Only the Class B altitude edits ride this gate.

## Class H-inline — historical records inside living documents

Alongside the Class A / Class B split above, a third category governs passages *inside* living, `Accepted` documents (ADRs, SPECs, catalogs): passages that **record what was verified or reasoned at a specific moment**, not standing claims about the current system — *landing checklists* (what was done at ratification), *alternatives-considered* rationale (why an option was rejected *at the time*), and *ratification records*. These are **Class H-inline** and are **never edited** — not now, not at a later merge — for the same reason `docs/handoff-session-*.md` (Class H) is never touched: a text that records what was checked or decided at a given moment is not corrected, because correcting it falsifies the record. A passage is Class A or B (a live coherence claim, in scope for correction) only if it asserts how the system *is* or *will be*; if it records what was checked or decided *then*, it is H-inline and immutable.
