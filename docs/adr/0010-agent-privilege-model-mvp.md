# ADR-0010: Agent privilege model and installation posture for the MVP

- Status: Accepted
- Date: 2026-05-23
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0008 (ETW crate selection) committed the agent to consuming events from the `Microsoft-Windows-Kernel-Process` manifest provider via ferrisetw 1.2.0. Opening that provider's session requires elevated privileges (membership in the Performance Log Users group, `SeSystemProfilePrivilege`, or running as LocalSystem) — the Phase 0 spike validated empirically that an elevated user satisfies the privilege baseline (`IsInRole(Administrator) = True`, all three REQ-A/REQ-B gates GREEN, [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md)).

ADR-0008's §Consequences > Neutral block forward-references this ADR as the home of two specific decisions deferred from Phase 0:

1. **How does `cg-agent` acquire the privilege baseline at runtime?** The two realistic shapes are a Windows Service registered as LocalSystem (production-standard for security agents) and an elevated user process launched by an operator (minimal subsystem, no installer needed). The choice affects installer scope, logging story, crash-recovery design, and the SPEC-005 acceptance criteria for "what does the agent do when started without sufficient privilege".
2. **Is the CI assumption valid that `windows-latest`'s `runneradmin` carries the same effective ETW-open privileges as a locally-elevated user?** The Phase 0 plan included a CI validation step; it was [explicitly skipped by chat decision](../spikes/2026-05-23-etw-process-events.md#ci-gate-skipped--accepted-risk) and carried forward to this ADR as accepted risk with two named fallback paths.

This ADR settles both questions for the MVP. It does **not** pre-decide the packaging story (installer technology, service framework, auto-update, log rotation, Event Log integration) — those belong to a future packaging SPEC that supersedes the installation-posture portion of this ADR when it lands. The privilege-model portion (the "elevated baseline" requirement) carries through unchanged regardless of how packaging evolves.

## Decision

For SPEC-005 and immediate successors, the agent's privilege and installation posture is the minimum required to satisfy ADR-0008's ETW elevation requirement, deferring all packaging machinery to a future SPEC.

### 1. Privilege model in MVP — elevated user process

`cg-agent` runs as an **elevated user process** (the user must be a local Administrator, with elevation accepted via UAC). This is the minimum privilege baseline that satisfies the Kernel-Process ETW open requirement empirically validated in the Phase 0 spike. The agent does **not** require LocalSystem in MVP; LocalSystem is a strict superset of "elevated user" privileges, so a future migration to Service-as-LocalSystem (in the packaging SPEC) preserves all paths that work today.

The agent MUST detect insufficient privilege at startup and exit cleanly with a clear stderr message ("cg-agent: insufficient privilege to open Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem"). It MUST NOT panic, MUST NOT crash, and MUST NOT silently degrade. SPEC-005 specifies the acceptance criterion that validates this clean-fail behaviour.

### 2. Installation posture in MVP — operator-launched, no Service

`cg-agent` is launched **manually by an operator from an elevated shell** (an Administrator PowerShell or Command Prompt). There is no Windows Service registration, no MSI / NSIS / WiX installer, no auto-start-at-boot mechanism, no scheduled task, no service crash-recovery policy. After a host reboot, the operator re-launches `cg-agent` manually.

This posture is explicitly **MVP-only**. It is **not production-deployable for end-customer use**: a production endpoint cannot require an operator to manually launch the agent after each host reboot or process crash. The forthcoming packaging SPEC will introduce the production posture (Windows Service registered as LocalSystem, MSI installer, service lifecycle with auto-restart, log rotation / Event Log integration) and supersede this section of the ADR. The decision to defer is a scope discipline call: SPEC-005 ships process telemetry, not packaging.

Logging in MVP is stdout/stderr (during interactive development) and a file (the path settled by SPEC-005). The MVP does **not** depend on the existence of an interactive session for correctness; file logging is the primary path.

### 3. CI privilege assumption and accepted risk

CI runs the SPEC-005 acceptance criteria on GitHub Actions `windows-latest` runners. Jobs execute as the `runneradmin` user with Administrator membership.

**Assumption.** `runneradmin` carries the same effective ETW-open privileges as a locally-elevated user. The Phase 0 spike validated the locally-elevated case empirically; the CI delta — "does `runneradmin` have the same privilege bits at the kernel level" — was **not** re-validated as part of Phase 0, by chat decision.

**Validation point.** The SPEC-005 AC-001 marquee (real `cg-agent` capturing real ETW events end-to-end against the real ingest stack) is the **first-use validation** of the CI assumption. If AC-001 passes in CI on its first run, the assumption holds and this accepted risk closes silently. If AC-001 fails specifically because `runneradmin` cannot open the Kernel-Process provider, the failure mode is unambiguous and triggers one of the two fallback paths below.

**Fallback path 1 — elevate the runner via a SYSTEM trampoline.** Use `psexec /s`, a pre-elevated scheduled task, or a similar mechanism to launch the AC-001 marquee child process as LocalSystem rather than `runneradmin`. Pro: preserves the marquee's residence in CI. Con: adds runner-side setup that has to be maintained.

**Fallback path 2 — move the marquee AC out of CI.** Document the limitation explicitly, run the unit and integration layers in CI, and require the marquee to be executed manually on a developer Windows machine before each merge that touches the ETW path. Pro: zero CI complexity. Con: loses an automated regression guard for the highest-value test in the SPEC.

The choice between fallback 1 and fallback 2 is deferred until / unless the assumption is contradicted by AC-001 in CI. We do not pre-pick a fallback; we name them so the decision is fast when (if) the time comes.

**Amendment 2026-05-29.** The CI assumption was resolved via **Fallback path 2** (marquee moved out of CI). Two independent constraints forced this resolution, neither of which is the privilege assumption itself:

1. **Docker runtime unavailability on `windows-latest` hosted runners.** Testcontainers (the marquee's infrastructure dependency) cannot detect a working container runtime on GitHub Actions `windows-latest` runners. The `ts-ci-windows.yml` workflow was empirically falsified at Phase 3.5.H (Session 15) and removed via Path D resolution. This constraint is infrastructure-level, not privilege-level.

2. **Elevated user process model (ADR-0010 §Decision part 1).** The MVP agent requires elevation to open the Kernel-Process ETW session. The GitHub Actions `runneradmin` user — even if it had Docker — may not carry the same effective ETW-open privileges as a locally-elevated user. This constraint remains untested in CI.

The SPEC-005 marquee (AC-001) is validated developer-local on Windows with Docker Desktop running and an elevated shell, per the procedure documented in CLAUDE.md §Developer-local SPEC-005 marquee validation. This is the standing validation gate for any merge that touches the ETW path. Marquee validated 8/8 GREEN in Phase 4 Session 16.

## Alternatives considered

### A1 — Ship as a Windows Service (LocalSystem) from MVP

Pros: production-correct posture from day one; LocalSystem is the strictest superset of "elevated user" privileges, so all future paths work; automatic restart on failure via service-control-manager policy; hidden from the interactive desktop; industry-standard for security agents. The packaging SPEC eventually has to land this anyway.

Cons: requires the full packaging machinery — installer (MSI / NSIS / WiX), service registration code in the agent (`sc create` or a Windows-service Rust framework like `windows-service-rs`), service lifecycle handlers (start / stop / pause / resume / shutdown), log rotation / Event Log integration (no stdout when running as a service), crash-recovery design (Windows service auto-restart policy + backoff), uninstaller. Each is a non-trivial subsystem; bundling all of them into SPEC-005 blocks the first event SPEC on packaging work that is not SPEC-005's actual scope. The "production-correct from day one" framing is real, but day one is MVP; production-deployability is the packaging SPEC's deliverable, not SPEC-005's.

Rejected as the MVP path; **retained as the packaging SPEC's path**. The decision is "defer packaging until packaging is the SPEC", not "never ship as a Service".

### A2 — Scheduled task with elevation, no Service

Pros: slightly less work than full Service registration — a scheduled task with `RunLevel=Highest` can launch `cg-agent` at boot as LocalSystem (or as a specific elevated user) without writing service-lifecycle code; the agent stays a simple process.

Cons: still requires installer machinery to register the task at install time and remove it at uninstall time. The logging story remains complicated (no console when launched by Task Scheduler). Crash recovery is weaker than a Service (Task Scheduler can re-trigger on failure, but the policy surface is poor compared to SCM's). And if we're going to do the work of an installer anyway, the marginal cost of a proper Service registration over a scheduled task is small; the halfway-house isn't justified.

Rejected. Either operator-launched (minimum work) or Service (production-correct) — scheduled task is the worst of both.

### A3 — Elevated process launched manually by operator (chosen)

Pros: minimum subsystem to ship SPEC-005. No installer required. Logging stays simple (stdout/stderr during development, file as primary). The operator has full visibility of the agent process (it's a process they launched, in a shell they own). CI can run `cg-agent` as a child of the test harness without any service plumbing. The Phase 0 spike's `IsInRole(Administrator) = True` shell is the exact deployment shape this assumes.

Cons: no automatic startup at boot — the operator must re-launch after every reboot. No automatic crash recovery — if the agent crashes, it stays down until the operator notices. Not production-deployable for end-customer use; explicitly MVP-only. Each of these is a real cost, all are accepted as the price of deferring packaging.

Chosen.

## Consequences

### Positive

- SPEC-005 unblocked on the privilege side: the elevation requirement is locked, the failure mode for insufficient privilege is named, and no installer/service work is on SPEC-005's critical path.
- The Phase 0 spike's empirical setup (elevated PowerShell, `IsInRole(Administrator) = True`) is exactly the MVP deployment shape, so the spike's validation transfers directly to SPEC-005's runtime.
- The future packaging SPEC has a clear supersession contract: it inherits the privilege baseline (elevated minimum, LocalSystem permitted), replaces the installation posture, and explicitly references this ADR's structure.
- Logging in MVP is straightforward (stdout / stderr / file), no Windows Event Log plumbing required yet. The Event Log path arrives with the packaging SPEC's service-mode story, where it is the correct sink.

### Negative

- The MVP is not production-deployable for end-customer use. Anyone running `cg-agent` outside a developer or CI context is doing it manually from an elevated shell, which is fine for SPEC-005's "process telemetry MVP" scope and explicitly not fine for shipping to customers. The packaging SPEC must close this gap before any external deployment.
- No automatic crash recovery in MVP. If the agent process exits (panic, OS kill, OOM), it stays down until the operator manually relaunches. Acceptable for MVP; resolved by the Service-mode auto-restart policy in the packaging SPEC.
- The CI assumption about `runneradmin` is unvalidated until SPEC-005 AC-001's first CI run. If the assumption is wrong, one of the two fallback paths fires — neither is free.

### Neutral

- DPAPI-protected identity loading (the SPEC-002 cert + private key path) is **privilege-orthogonal**. SPEC-002 uses `CRYPTPROTECT_LOCAL_MACHINE` scope (per [agent/cg-agent/Cargo.toml](../../agent/cg-agent/Cargo.toml) Windows target deps), so identity load works identically whether `cg-agent` runs as a standard user, an elevated user, or LocalSystem. This ADR's privilege choice does not perturb ADR-0004's / SPEC-002's identity-load contract. (This is the reason ADR-0010 has no dependency-graph edge to ADR-0004 — there is no constraint passing in either direction.)
- The mTLS heartbeat path is similarly privilege-orthogonal — `rustls` makes no privilege requirements. ADR-0004's transport contract continues to hold under any privilege posture this ADR or its successor declare.

## Compliance

- `cg-agent` MUST detect insufficient privilege at startup (via the first attempt to open the Kernel-Process ETW session, returning a recognisable Win32 error code) and exit cleanly with a clear stderr message identifying the cause. SPEC-005 specifies the corresponding acceptance criterion.
- `cg-agent` MUST NOT assume the existence of an interactive session. File logging is the primary output path; stdout / stderr is a development convenience that may be unavailable when the future packaging SPEC migrates to Service mode.
- `cg-agent` MUST NOT require privileges beyond "elevated user" in MVP. Specifically, the code path that opens ETW sessions MUST work identically when run as an elevated user and when run as LocalSystem; the future packaging SPEC's service mode is then a deployment swap, not a code change.
- The installation-posture portion of this ADR (§Decision part 2) is **superseded** by the forthcoming packaging SPEC when it lands. The privilege-model portion (§Decision part 1) and the CI-validation framing (§Decision part 3) carry through unchanged unless that future SPEC explicitly amends them.
- No code outside the privilege-detection path may attempt to elevate, request additional privileges, or change the process's security context at runtime. The agent runs at whatever privilege level it was launched with; changing posture is the responsibility of the launcher (operator shell in MVP, service-control-manager in the packaging SPEC).

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout. The agent lives at `agent/cg-agent/`; the privilege model attaches to its process.
- [ADR-0002](0002-language-per-component.md) — Language per component. The agent is Rust; this ADR specifies how the Rust agent process runs.
- [ADR-0008](0008-etw-crate-selection.md) — ETW crate selection. The elevation requirement originates here (Microsoft-Windows-Kernel-Process opens require elevated context); this ADR is the home of ADR-0008's forward-pointer about the CI-privilege-assumption-as-accepted-risk.
- [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md) — Phase 0 spike. Empirical validation of the elevated-user privilege baseline; the source of the deferred CI validation that this ADR carries.
- [docs/engineering-notes.md](../engineering-notes.md) — Session 10 conventions used in this ADR's framing (three-role Deciders, "amends in part" edge notation).
- [SPEC-005](../specs/SPEC-005-agent-process-telemetry-windows-etw.md) — First consumer of this ADR. Specifies the acceptance criterion that validates clean-fail-on-insufficient-privilege, and is the first-use validation of the CI privilege assumption.
- Packaging SPEC (forthcoming, unnumbered until proposed) — Future supersession scope for §Decision part 2 (installation posture). Will introduce Windows Service registration, installer, service lifecycle, log rotation / Event Log integration, and auto-update.
