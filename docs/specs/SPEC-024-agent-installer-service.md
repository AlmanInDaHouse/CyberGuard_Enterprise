# SPEC-024: Windows agent installer, service lifecycle and key custody

- **ID:** SPEC-024
- **Title:** Windows agent installer, service lifecycle and key custody
- **Status:** Accepted
- **Depends on:** ADR-0008 (the startup reclaim of the ETW session), ADR-0010 (the packaging SPEC expressly supersedes its installation posture), ADR-0022 (F2, key custody and release protection), SPEC-001, SPEC-002, SPEC-003, SPEC-017, SPEC-019, SPEC-020 and SPEC-023.
- **Authors:** Manuel (project owner), Claude Code (design and implementation)

## Context

At `f07fa39`, `cg-agent` is an operator-launched process. `main.rs` reads
`--config`, logs to stdout and waits for Ctrl+C. No installer, SCM dispatcher
or service registration exists. `identity.rs` writes identity files and then
sets a DACL admitting the enrolling user's SID and SYSTEM. Machine-scope
DPAPI does not prevent that user's unelevated processes decrypting a readable
blob (#87, threat-model R1). The enrollment request and both heartbeat paths
use Cargo's version, independently of the release tag (#131).

This is F2's implementation contract, not evidence that these gaps are fixed.
ADR-0010 §Decision part 2 and §Compliance explicitly delegate the production
installation posture to a packaging SPEC: LocalSystem, MSI, service recovery
and logging. This SPEC exercises that provision without editing the ADR or
changing its elevated privilege baseline. Its accompanying SPEC amendments
identify the older clauses it supersedes. The service still runs the existing
secure capture and delivery path; it adds no sensor or remote command channel.

## Scope

### In scope

- Windows x86-64 installation, repair, upgrade and uninstall; one setup `.exe`
  containing an MSI, with an interactive and unattended route.
- A LocalSystem Windows service, automatic startup, bounded stop and failure
  recovery, local diagnostics and private rotated JSON logs.
- SYSTEM custody from creation of the installed identity, its parent directory
  and temporary files; explicit handling of legacy manual installations.
- One release version across installer, binary, enrollment and heartbeats.
- Executable lifecycle/security tests, the Windows validation matrix and the
  endpoint portion of the installation/upgrade runbooks.

### Out of scope

- Certificate renewal, revocation and server enrollment hardening (#88/#89)
  remain F4. No automatic re-enrollment on an expired or broken identity.
- Dashboard pages, invitations and the enrollment-token UI remain F3. Before
  those pages exist, use SPEC-023's administrative token-issuance command.
- Automatic product updates, remote execution, a new telemetry class, a disk
  event buffer, multiple agents on one host, non-Windows production agents,
  tamper resistance against an elevated administrator or SYSTEM.
- Authenticode purchase/signing remains ADR-0022 Open question 1, to reopen
  before publication. No release tag or publication is authorized by this SPEC.

## Data contracts

### 1. Artifacts and version (#131)

The product tag remains `vX.Y.Z`. In a release build the exact `X.Y.Z`, without
`v`, is the agent's version. A build-only input `CG_RELEASE_VERSION` supplies
it; this is not an operator/runtime environment variable. A build with that
input malformed or empty fails. A release job must supply it from the tag,
require an exact match and reject an absent input before packaging. A build
without the input reports `<Cargo package version>-dev`; it must not silently
masquerade as a product release. Rebuilding after changing or removing the
input must refresh the embedded value, including in the same target directory.

The same embedded value appears in `cg-agent --version`, enrollment's
`agent_version`, both heartbeat paths' `agent.agent_version` and lifecycle
version fields. The envelope and CGES schema versions remain independent.
`--version` requires no config, network, identity, service or elevation.

The existing raw executable asset
`cg-agent-<version>-x86_64-pc-windows-msvc.exe` remains. Add
`cyberguard-agent-<version>-x64-setup.exe` and
`cyberguard-agent-<version>-x64.msi`. The setup contains that MSI and the MSI
contains that exact raw executable; no endpoint toolchain or network download
of installer components is required. Publish SHA-256 entries and provenance
for all three, tied to the same source revision. Verify the outer artifacts
and the installed binary against those digests. These additions extend
SPEC-023's release asset list; they do not replace the server bundle.

MSI `ProductVersion` is `X.Y.Z`; PE numeric file versions use `X.Y.Z.0`, with
the product string `X.Y.Z`. Packaging rejects versions outside MSI's limits
(major/minor 0–255, patch 0–65535), rather than truncating, wrapping or
inventing a second product version. Development builds are not publishable
release packages. The MSI upgrade family is stable across releases; a higher
version replaces the installed product, and a lower one is refused.

### 2. Installed paths, identity and configuration

Resolve Windows' machine directories through the OS, not a caller-controlled
environment variable. Default locations:

| Object | Location | Access policy |
|---|---|---|
| Binary | `%ProgramFiles%\CyberGuard\Agent\cg-agent.exe` | SYSTEM and elevated Administrators may write; ordinary users may read/execute |
| Configuration | `%ProgramData%\CyberGuard\agent\agent.toml` | SYSTEM owner; protected DACL, SYSTEM and Administrators only |
| Server trust anchor | `%ProgramData%\CyberGuard\agent\server-ca.pem` | Same as configuration; only the product uses this trust store |
| Identity | `%ProgramData%\CyberGuard\agent\identity\{cert.pem,key.dat,identity.json}` | SYSTEM owner and SYSTEM-only protected DACL |
| JSON logs | `%ProgramData%\CyberGuard\agent\logs\agent.jsonl` and archives | SYSTEM owner; SYSTEM writes, elevated Administrators may read |

The private data root and its parents within `CyberGuard` must prohibit
unprivileged creation, replacement, deletion and ACL/owner changes. Identity
directories admit SYSTEM only; configuration/staging directories admit SYSTEM
and Administrators only. No individual enrolling-user ACE is installed. A
filtered administrator token has no access through the Administrators ACE,
and must not gain owner rights through the enrolling user's SID. Elevated
administrative maintenance remains possible; it is not a threat boundary.

The config uses the existing TOML fields and defaults. Installed mode requires
`[enrollment]`, explicit HTTPS enrollment and heartbeat URLs, an explicit
server trust-anchor file, and the fixed absolute identity paths above. It
rejects a plain/test path, relative paths or a path escaping the private
data tree. Installation takes the hostname from Windows; the server-issued
UUID remains authoritative. Until enrollment, the existing required
`agent.id` field is a generated UUIDv7 placeholder, never a claimed enrolled
identity. No private key or identity comes from the installer package.

### 3. Operator interface and service state

The setup wizard elevates before writing machine state. It takes the two
URLs, the server trust-anchor PEM and its expected SHA-256 fingerprints,
obtained independently through SPEC-023's CA distribution procedure, and a
masked enrollment token. A mismatched fingerprint is a failure, not a prompt
to trust the network certificate. It does not add roots to Windows' global
trust store or modify firewall, audit policy or antivirus settings.

The unattended route accepts a bootstrap TOML file path (the existing config
fields plus `setup.trust_anchor_sha256`, a nonempty array of `sha256:` followed
by 64 lowercase hexadecimal digits), never a token
in argv, MSI properties or environment. It requires a regular local file
whose owner/DACL exclude unprivileged access, including the enrolling user's
filtered token; creation instructions must establish protection before
writing the token. Read it without echo, copy into the protected destination
without an exposure window, and leave deletion of the operator's original to
the documented operator procedure. Do not silently delete arbitrary inputs.
Each fingerprint is over one certificate's DER, as in SPEC-023's export;
the list must match exactly the distinct certificates in the trust-anchor
file, with no extra unverified root. A trailing export fingerprint line is
not a substitute for the independently obtained list. Setup supplies the
fixed installed paths, hostname and placeholder ID rather than accepting a
bootstrap override of them. The installed TOML contains no `setup` table. UI and unattended
inputs follow the same validation and secret-handling rules.

The service name is `CyberGuardAgent`, display name `CyberGuard Agent`,
account `LocalSystem` (SID `S-1-5-18`), own process, automatic start without
an interactive desktop. Its command line is the quoted absolute binary path,
`--service`, and `--config` with the quoted fixed absolute config path. No
token or secret appears in the service configuration. Only SYSTEM and
elevated Administrators may change, start, stop or delete the service;
ordinary users may query status. The ordinary console CLI remains available
for development and regression tests; `--service` requires a real SCM launch
and cannot silently fall back to console mode.

## Operational

### 1. Installation and enrollment

Validate platform, elevation, input protection, versions, paths and existing
installation state before changing the host. Refuse a conflicting service,
untrusted/pre-existing data directory, unsupported platform or reparse-point
destination without following it or overwriting it. A legitimate existing
installation is managed through the repair/upgrade route below. Resolve and
check paths with handles so a path swap between checking and writing cannot
redirect privileged writes; reject hard-linked destination files too.

The MSI installs machine-owned files and service metadata transactionally.
Enrollment happens after package commit, in the service as SYSTEM, not in an
MSI action or the administrator's process. On enrollment failure the package
remains installed with an explicit pending/failed configuration state; do not
report operational success or roll back a server-side enrollment as though it
were a local transaction. Retrying setup/configuration must not create a second
identity when a complete valid identity already exists.

SCM `RUNNING` means local initialization succeeded and the supervisor can
receive controls. It does not prove connectivity, enrollment or capture.
The wizard distinguishes installed, enrolling, operational and failed; it
reports operational only after identity persistence, capture startup and an
acknowledged secure heartbeat. With a reachable nominal server and valid
inputs this occurs within 60 seconds after configuration is committed. A
timeout shows the pending/failure reason and a safe retry procedure, without
claiming that F3's not-yet-built dashboard has displayed anything.

### 2. Key custody and failure atomicity (#87)

Keep Ed25519, zeroized key buffers and machine-scope DPAPI. The protection
against an ordinary local user is the filesystem access boundary, not DPAPI
alone. Create a SYSTEM-owned protected directory before writing any sensitive
byte. Every new identity file, token-bearing config, replacement, staging file
and backup has its final restrictive access at creation. Tightening an ACL
after a broad file write is not sufficient. Check existing ownership, DACLs,
type and containment before reading secrets or modifying files; invalid
custody fails closed before enrollment or network delivery.

Identity persistence commits a complete, mutually consistent set of cert,
DPAPI key and metadata, or leaves a detectable incomplete state. Check that
the certificate public key matches the private key and metadata fingerprint,
and that its agent identity matches the stored `agent_id`. A partially written,
corrupted or mismatched identity never triggers automatic fresh enrollment,
never overwrites a valid identity and never reaches heartbeat/capture. The
failure identifies the repair action without exposing keys or tokens. A
consumed token plus lost enrollment response can still require a new token
and server-side cleanup; F2 does not invent an idempotent enrollment protocol.

Token removal remains SPEC-002 FR-014's best-effort operation after successful
persistence. A failure leaves only an already-consumed token in protected
storage and emits a redacted warning. The atomic rewrite must retain the
same owner/DACL; startup ignores a stale token when identity is valid.

For an existing manual installation, default setup refuses to adopt or erase
its identity. A documented, explicit elevated migration on the same host may
stop the manual agent, verify its three identity files and trust configuration,
and transfer them through private staging into SYSTEM custody before starting
the service. It preserves agent ID and key fingerprint, and verifies old
product-owned copies are removed with the operator's explicit migration
choice. Failure preserves a recoverable original and leaves the service
stopped. Copied keys on another host are not a migration mechanism.

Migration cannot recall copies previously read by the enrolling user's
processes. Disclose that residual R1 exposure and the need for F4's credential
replacement/revocation work when compromise is suspected. #87's implementation
evidence must distinguish fresh installation from legacy migration; neither
the contract nor a narrower ACL proves that an old key was never stolen.

### 3. Service lifecycle and logging

The SCM entry point invokes the same secure runtime as the console path,
without requesting additional privileges or changing security context at
runtime. It reports start/stop pending progress with bounded wait hints,
accepts STOP and SHUTDOWN once running, and reports terminal status accurately.
Control handlers signal shutdown promptly; they do not block on HTTP or capture
teardown. No pause/continue support is advertised. Failure before capture is
visible in local diagnostics, not a permanently pending service.

STOP uses SPEC-017/SPEC-020's ordering: stop logon subscription and ETW capture,
then one best-effort final `going_offline` send and log undelivered events.
Service shutdown has a 60-second total deadline, including the final request;
if the normal request timeout would exceed the remaining budget, bound it by
that budget. On expiry log the incomplete stop and report failure, never a
clean drain. Windows may terminate earlier during OS shutdown; no lossless
shutdown or persistent queue is promised. Normal stop leaves no agent session
or subscription, and a second start succeeds. Deliberate stop/uninstall does
not trigger automatic recovery.

Unexpected process death restarts after 60 seconds, then 120, then 300 seconds
for subsequent failures, resetting the failure count after a day without
failure. Transient initial enrollment/network failure retries without a tight
loop (at least 60 seconds between exhausted enrollment rounds), and stop stays
responsive while waiting. Invalid config, rejected tokens, corrupted identity,
failed custody or trust and missing capture privilege require operator action;
they do not trigger endless enrollment, key replacement or silent plain HTTP.
The existing console exit codes remain; service diagnostics expose the same
cause through SCM status and the Windows Application Event Log.

Only one production capture instance may run on a host. Service and console
startup must reject a second live instance before it can reclaim that
instance's fixed ETW session. Crash recovery may reclaim an orphan after
establishing no live product instance owns it. Run elevated marquees with the
installed service stopped, or on a separate machine, never by killing an
unidentified ETW session on a development host.

Service logs are UTF-8 JSON lines: one active file limited to 10 MiB and five
archives of at most 10 MiB each, rotated without broadening access. An oversized
record is replaced by a bounded diagnostic, not split into invalid JSON.
Register source `CyberGuardAgent` in Windows' Application log for lifecycle and
fatal diagnostics, including failures before file logging. Secrets, full
config, private-key bytes, token values and raw telemetry payloads are excluded
at every log level, including bootstrapper/MSI verbose logs. File logging
failure at startup prevents operational success; persistent runtime write
failure emits a bounded Event Log diagnostic and stops with failure, without
silently continuing indefinitely. Windows controls Event Log retention; local
rotation is a space bound, not the server's year-long retention policy.

### 4. Repair, upgrade, uninstall and rollback

Same-version repair restores package-owned binary/service metadata and checks
custody without resetting configuration, anchor, agent ID or keys. Upgrade
verifies new artifacts first, stops the service and proves it stopped before
replacing files. It preserves those same data and rolls back binaries and
service metadata if the package transaction fails. Validate rollback of each
supported adjacent version pair before shipping an upgrade; a same-version
test is not that evidence. Refuse downgrade and incompatible data formats
before stopping a healthy service; no automatic downgrade or auto-update.

A failure to restart after a committed upgrade is reported as installed but
unhealthy, with a recovery procedure; it is not proof that an older binary
can read data already changed. This phase introduces no identity format
migration. Interrupted maintenance must be recoverable without re-enrollment
and must never leave the service pointing into temporary or user-writable
locations. Do not restart the computer automatically; surface a required
reboot separately from successful service startup.

Default uninstall stops and removes the service and binaries, preserving the
private data/config/logs with their ACLs for a reinstall. Explicit purge is a
separate elevated, deliberate operator choice: remove only validated
product-owned paths, never follow junctions or remove arbitrary configured
paths. It removes local identity, not the server's agent or retained events,
and is not secure erasure or certificate revocation. Reinstall with preserved
valid state loads the same identity without using a token. Document the
network-data-loss window during maintenance and the lack of offline buffering.

### 5. Release and validation boundaries

The package sources belong under `deploy/windows/`. Build tooling must be
pinned, licence-reviewed and recorded before use; this contract adds no agent
dependency and does not authorize a paid tool or certificate. Prefer the
existing Win32 binding for SCM and filesystem calls over another elevated
runtime dependency. Local tests/dry runs publish nothing. The release workflow
must check version agreement and package contents before publication, with
the existing licence gate and least-scoped permissions preserved.

The endpoint runbook must include read-only inspection of the effective
logon audit policy and the host's existing crash/memory-dump configuration,
including centrally managed settings where applicable. A working Security-log
subscription does not establish that logons are audited. Record absent or
unknown audit coverage and the possibility of secrets in externally produced
dumps; do not report either as a verified protection. No product dump
generator/uploader or host-policy mutation is introduced here. The inherited
architecture questions about automatic audit-policy reporting and dump policy
remain explicitly reopened below, not silently treated as resolved.

Keep the active release-tag freeze. F2 completion requires its code, tests,
runbooks and endpoint evidence; first public release additionally waits for
F3/F4, #130 and Manuel's explicit publication approval. Positive published
provenance and the timed <30-minute install remain first-release evidence.

## Acceptance criteria

Each `install_ac_*` is a required outcome, not an assertion that a test already
exists. A contract-only change closes neither #87 nor #131.

| ID | Observable acceptance condition |
|---|---|
| install_ac_001 | Setup/MSI install per-machine on Windows x64; one automatic LocalSystem service points at the quoted protected binary/config; an ordinary user cannot change its control/configuration or binary. |
| install_ac_002 | Interactive and unattended routes validate the same inputs; wrong anchor fingerprint, HTTP, unsafe paths and unsafe bootstrap ACLs fail before enrollment. No token appears in process argv, environment, MSI properties or logs. |
| install_ac_003 | With a real ingest and valid token, fresh service installation enrolls once, persists identity under SYSTEM and acknowledges its first secure heartbeat with capture active within 60 seconds of configuration commit. No interactive session is required. |
| install_ac_004 | A different standard user and the enrolling administrator's filtered token cannot read/decrypt, replace, delete, take ownership through owner rights, or change the ACL of identity/config/staging files; SYSTEM can use the identity. Check effective access as well as full owner SIDs/DACLs. |
| install_ac_005 | Observing file creation/replacement and injecting persistence/ACL failures exposes no broadly readable intermediate secret; partial/mismatched state fails closed, sends no heartbeat and does not re-enroll or destroy an existing valid identity. |
| install_ac_006 | Reboot and service restart preserve agent ID/key fingerprint; the service runs without the installer user's session. Token hygiene and a failed hygiene rewrite preserve restrictive access and disclose no token. |
| install_ac_007 | STOP/SHUTDOWN during normal delivery, initial enrollment and retry waits is responsive, follows capture teardown/final-send ordering and terminates within the 60-second deadline; a stalled final request yields truthful failure/loss diagnostics. Normal stop leaves no agent ETW session and permits restart. |
| install_ac_008 | In an isolated Windows machine, crash recovery follows 60/120/300-second delays and reset policy; intentional stop does not restart. Permanent configuration/identity/trust failures do not loop or re-enroll. |
| install_ac_009 | A concurrent console/service instance fails before reclaiming the live instance's ETW session; an orphan after a crash is recoverable without affecting another application's sessions. |
| install_ac_010 | Rotation limits the JSON files to one active plus five archives of 10 MiB each, with valid bounded JSON and unchanged protection; disk/write failures and pre-logger failures have redacted Application-log diagnostics and truthful service status. |
| install_ac_011 | Repair and a tested adjacent-version upgrade preserve config, CA, ID and key; injected transaction failure restores the previous binary/service. Downgrade, unsafe pre-existing paths and incompatible state are refused without corrupting a healthy installation. |
| install_ac_012 | Default uninstall preserves protected data and reinstall reuses it without enrollment; explicit purge removes only owned data and never follows a reparse point. Neither claims server revocation or remote deletion. |
| install_ac_013 | Legacy installation is refused by default; explicit same-host migration preserves ID/key and restricts all remaining product-owned copies. Failure leaves recoverable original state and a stopped service. Document prior-copy exposure. |
| install_ac_014 | With a fixture release version different from Cargo's, CLI, enrollment, plain and secure heartbeat and lifecycle fields all match that version. Without the build input they all report the documented dev value. Rebuild changing/removing the input proves it is not cached stale. |
| install_ac_015 | Release packaging rejects absent/invalid/mismatched/out-of-range versions; setup, MSI and installed/raw binary have the same product version and contained agent-payload digest (outer package hashes differ). All required asset checksums/provenance subjects are present; a modified or unattested artifact fails verification. |
| install_ac_016 | Endpoint runbook commands execute against the packaged artifact, including failure/recovery paths and read-only audit/dump-policy inspection. Record absent/unknown audit coverage and external dump exposure, OS/build, privileges, versions, package/binary SHA-256, source/tree, results and cleanup. Windows 11 evidence does not substitute for Windows 10 or Server 2019 (#59). |

## Test scenarios

- Unit/portable integration: config and version validation, lifecycle state
  transitions, injected shutdown deadlines, log rotation and secret redaction
  (001/002/007/010/014/015). Exercise behavior, not text-matching this SPEC.
- Windows integration as real SYSTEM and restricted user tokens: actual SCM,
  machine DPAPI, NTFS access/ownership, installation transactions and path-race
  fixtures (001–002, 004–013). Tests may use isolated temporary roots; that does
  not enable an operator to redirect the production service's private paths.
- Real Windows plus ingest/Docker: enrollment, all three existing capture
  classes, acknowledgements, stop and reinstall/upgrade continuity
  (003/006/007/011/013). Run the existing full elevated capture/detection gate
  as well; a mock-only SCM test is insufficient. Resolve #4/#37's marquee
  fixture evidence/drain issues with the first capture/gate implementation.
- Disposable Windows VM: reboot, killed service, interrupted MSI, rollback
  and disk-failure experiments (006–013). Manuel runs the elevated experiments;
  do not reboot the development host or change its Event Log service for #50.
  A dead Security-log subscription experiment remains #50 until actually run.
- Packaging CI: build/install/extract/repair/uninstall and verification failures
  with fixture versions (011/012/014/015). Real published verification remains
  separate from a local checksum or negative unattested fixture.
- Compatibility record: exercise the above endpoint paths on Windows 11,
  Windows 10 and Windows Server 2019, recording exact editions/builds. Do not
  claim all newer OS releases tested, or vendor support from product testing.
  #59 remains open until the two missing platforms have execution evidence.

Before each implementation, write acceptance tests from these criteria and
observe the missing-behavior failures locally. Push only green commits, with
the complete applicable workspace/package gate, fresh-context review and
CLAUDE.md's elevated evidence where required. #46's unified evidence rule
applies; documentation of a future gate is not gate evidence.

## Risks

- LocalSystem compromise still defeats endpoint custody. This adopts the
  production posture expressly anticipated by ADR-0010, not privilege isolation
  from a malicious administrator. No new risk is accepted by this text.
- An ordinary user who previously copied a legacy DPAPI blob may already have
  the key. Migration and restrictive new ACLs cannot undo that exposure.
- Consumed enrollment tokens, interrupted persistence and absent F4 renewal
  require explicit recovery; retries must not manufacture another identity.
- Unsigned setup triggers platform warnings. Checksums alone authenticate no
  publisher; use provenance and the independently obtained release reference.
- Reboot/crash experiments can disrupt capture and lose buffered events; only
  isolated machines provide the destructive fault-test evidence.

## Open questions

1. **Authenticode** remains ADR-0022 Open question 1: Manuel decides purchase
   before the first public release. The release freeze stays active meanwhile.
2. **External Windows evidence** (#59/#50): reopen when Manuel provides the
   isolated Windows 10/Server 2019 environments and the fault-test window.
   The implementation may progress with available Windows 11/CI evidence;
   no missing-platform claim is waived.
3. **Packaging tool pin/licence**: decide and record before the package build
   change. A paid choice or one requiring an ADR waits for Manuel's approval;
   no tool installation is implied by accepting this contract.
4. **Automatic audit-policy reporting**, ADR-0019 Open question 1, carried here
   by SPEC-023: reopened by F2. ADR-0019 still excludes managing/reporting that
   policy. Read-only operator preflight is required above; it does not answer
   whether the agent should inspect/report it at startup. Before F2 endpoint
   validation is called complete, propose its disposition; any change to the
   ADR's boundary needs Manuel's explicit yes before implementation.
5. **Endpoint crash/memory-dump policy**, ADR-0020 Open question 4, likewise
   reopened by F2. Inventory existing host policy and disclose exposure; no
   automatic generation/upload is added by this SPEC. Before F2 endpoint
   validation is called complete, put the keep/send/access/retention policy
   to an explicit disposition, with Manuel's ratification if an ADR changes.
   Do not treat the absence of a product dump writer as absence of host dumps.

## Decision record

Decisions taken by Claude Code under Manuel's Session 49 delegation,
2026-10-27; ADR approval and elevated execution remain Manuel's:

- Use the LocalSystem/MSI posture already named in ADR-0010, with an offline
  setup `.exe` wrapping the MSI to meet the Blueprint's single-executable
  installation route. Reject a scheduled task and a shell-only installer.
- Retain machine-scope DPAPI and establish SYSTEM ownership/access before any
  write. Reject making the enrolling user's SID the installed key's owner;
  reject treating DPAPI as an account-level access control.
- Keep network enrollment outside MSI rollback and distinguish package,
  supervisor and telemetry readiness. Reject reporting SCM RUNNING as proof
  of a working server connection.
- Preserve identity on upgrade/repair/default uninstall; migration and purge
  are explicit. Reject silent key regeneration or assumed remote revocation.
- Embed the release version once at build time, with a visibly distinct dev
  default and checked MSI limits. Reject relying on unchanged Cargo metadata
  or truncating the product version to fit the package.
- Bound shutdown/log storage and test isolated failure recovery. Reject
  lossless-shutdown claims or using the development host as a crash fixture.
- Implementation follows this contract in separate changes; F3/F4 and the
  publication guard retain their current destinations. No ADR is amended.

## Amendment 2026-10-31: the release version's form

**What surfaced it.** The review of #131's implementation. §Data contracts §1
takes the tag's exact `X.Y.Z` as the version, and §Scope asks for one release
version across installer, binary, enrollment and heartbeats, but neither says
whether a component may carry a leading zero. The tag filter of `release.yml`
(`v[0-9]+.[0-9]+.[0-9]+`) admits `v01.2.3`: the executable would report
`01.2.3` while Windows Installer compares the `ProductVersion` as `1.2.3`,
two spellings of one product version.

**Amendment.** Each of X, Y and Z is a SemVer numeric identifier: `0`, or
ASCII digits without a leading zero. A build given any other input, an
empty one included, fails; `<Cargo package version>-dev` is reported only when
the input is absent. MSI's numeric limits stay with packaging (§1, last
paragraph; install_ac_015).

**Effect.** It narrows §Data contracts §1's "exact `X.Y.Z`" and "malformed",
and the §Decision record's version bullet, to that form; install_ac_014 and
install_ac_015 are unchanged, and the build's refusal of a malformed input is
to be exercised with install_ac_014's version tests. The release job's supply
of the tag's version to the agent build lands with the version implementation;
then a tag such as `v01.2.3` makes that build fail, so no GitHub Release is
created, while the images and bundle jobs, which do not wait for the agent's,
can still push and attest the images and attest the bundle under that tag.
Checking the tag's form before any publication belongs to install_ac_015's
release-workflow change, and the active release-tag freeze stops every `v*`
tag meanwhile. No release or version tag exists, so nothing published is
affected. The narrowed decision is this SPEC's own delegated one, and the
amendment adds no setting a deployment or an operator chooses. Taken by
Claude Code under Manuel's delegation (#131). Status stays **Accepted**.

## Amendment 2026-11-01: packaging tool, version resource, release check, test venue and coverage inventory

**What surfaced it.** Session 50 left three points for Manuel: Open question
3 with the way the executable's version resource is written; the place of
the tests that need the real service control manager (SCM) or SYSTEM; and
the disposition of Open questions 4 and 5, which bears on what the product
tells an operator about its own coverage. On 2026-11-01 he delegated the
three in chat: "esas opciones la decida claude code en beneficio del
proyecto". ADR changes, Authenticode, the approval of a publication, any
payment or recurring cost and accepting a EULA in his name stay his.
Debt #139 showed that no criterion places the release's version check
before the images' push.

**Amendment.**

1. **Packaging tool (Open question 3).** WiX Toolset v7 builds the MSI and
   the setup bootstrapper. Its `wix` tool and every extension the package
   uses are pinned in the repository at one exact release, and the .NET
   SDK that tool needs is pinned too; the .NET CLI runs with its telemetry
   off, locally and in CI. WiX's source is MS-RL (Microsoft Reciprocal
   License). Its binary releases are licensed to their user, the project,
   under the Open Source Maintenance Fee EULA v1.1 (EULA ID `wix7`), which
   sets the fee for their use: a fee to the WiX maintainers, under a heading that
   calls it monthly, for revenue-generating use by a user with annual gross
   revenue of US$10,000 or more, and none below (its §1 and §2). The same
   EULA lets the binaries be redistributed as MS-RL allows (its §4), so the
   buyer receives the WiX binaries the setup carries under MS-RL. The choice of the tool is taken under the
   delegation; as Open question 3 requires of a choice that can bring a
   fee, Manuel approved the fee: on 2026-11-01, asked in chat whether he
   accepted that EULA and its fee for the project, he chose "Acepto EULA y
   cuota futura". WiX and its extensions come from nuget.org, a channel
   outside CLAUDE.md's *Package manager preference*, and the SDK by
   winget, a machine-wide install that changes the system `PATH`; the
   first use of each is confirmed in chat. The EULA is accepted in the
   repository's build files and workflows only by the packaging change,
   which carries the pins. This
   amendment's commit adds both tools to CLAUDE.md's *Approved local
   toolchain*, as that table asks of the SPEC that introduces a dependency.
   The WiX binaries that the setup or the MSI carries (Burn's engine, an
   extension's custom action) run elevated on the buyer's endpoints. They
   ship unmodified, under MS-RL, whose reciprocity reaches each distributed
   file of WiX's own and not the agent: for each, the release carries
   MS-RL's text in its third-party notices (#130) and, as one release
   asset, an archive of those files' source from the pinned release's tag,
   in `SHA256SUMS` and attested like the setup, the MSI and the raw
   executable. By scope this amends SPEC-023's Amendment 2026-10-27, whose
   `SHA256SUMS` then covers six payload assets. The licence allowlist
   (`.github/scripts/licences-allowed.json`, under the policy of
   `docs/security/dependency-review.md`), which admits permissive licences
   and a few reviewed packages by name, gains those binaries by name under
   MS-RL, the licence of their source and of their redistribution, as a
   reviewed change with the packaging change. The licence check reads the
   nuget pins of what the setup or the MSI carries and admits them by name,
   since a nuget package declares its licence as
   a file, the EULA, not as an SPDX expression. Each nuget package is
   restored by its pinned version and checked against its recorded content
   hash before use, by a mechanism the packaging change provides, and an
   advisory check of those pins joins `dependency-audit` with it. Rejected: WiX v5.0.2 (no maintenance fee, but past its consumer security fixes' end date,
   2026-02-05, while its Burn engine would run inside the setup); WiX v6
   (its consumer security fixes end on 2027-02-05, and it carries an
   earlier maintenance-fee EULA, OSMF v1.0); NSIS and Inno Setup (no MSI); an
   MSI written through `msi.dll` by the project's own code (more installer
   code to audit, no bootstrapper).
2. **The executable's version resource.** The agent's own build writes the
   version resource of `cg-agent.exe` from the input of §Data contracts §1,
   with no new crate, build dependency or external tool, and refreshes it
   when that input changes or is removed. Its numeric file and product
   versions are `X.Y.Z.0`. Its `FileVersion` and `ProductVersion` strings
   are the version that `cg-agent --version` prints after the program's
   name: `X.Y.Z` in a release build, and `<Cargo package version>-dev` in a
   development build, whose numeric versions are Cargo's `X.Y.Z.0`. A
   component above
   65535, which a PE version field cannot hold, fails the Windows build;
   as for the other refusals of the build (Amendment 2026-10-31),
   install_ac_014's version tests exercise it and the resource's values in
   both builds, on Windows. Rejected: a
   resource crate (a new build dependency of the elevated agent, Manuel's
   decision); the Windows SDK's `rc.exe` called by the build (a Windows
   SDK tool the build would have to find, while the linker the MSVC target
   already uses takes a compiled resource file, which the build can write
   itself); stamping the file after the
   build (a new tool).
3. **The release's version check (#139).** One release job checks the tag
   before every other release job: the form of the Amendment 2026-10-31 and
   MSI's limits of §Data contracts §1, with `contents: read` alone. Every
   job that pushes, attests or
   publishes depends on it, directly or through another job; a tag that
   fails it ends the workflow with nothing pushed, attested or published.
   install_ac_015's rejection of invalid and out-of-range versions includes
   that order. SPEC-023's Amendment 2026-10-27 already requires that release
   jobs "reject version mismatches before publication"; this point makes
   the images' push and every attestation wait for the check too, and so
   publishes less. By scope it amends SPEC-023 §Operational §12 and its
   first Amendment 2026-10-26, item 4: the release runs the check ahead of
   the licence check, and §12's list of jobs and their permissions gains
   it. Its Amendment 2026-10-27's check of the payloads' agreement stays
   before publication; this tag check comes before any push or attestation.
   SPEC-023 classes that ordering as packaging, on Manuel's list; he
   approved this point in chat on 2026-11-01 ("OK, compruébalo antes").
4. **Where `cg-agent`'s service and SYSTEM tests run.** A Cargo test of
   `cg-agent` that creates, starts, stops or deletes a Windows service, runs
   code as SYSTEM or creates a local account runs only in rust-ci's Windows
   job, whose hosted runner runs its steps as the built-in Administrator and
   is discarded after the job. Each such test creates its own uniquely named
   service or account and removes it on every exit path. It fails without
   acting when a service named `CyberGuardAgent` exists, or outside a
   GitHub-hosted runner (`GITHUB_ACTIONS` and `RUNNER_ENVIRONMENT` say
   so), and never stops or changes a service it did not create. Only test targets require the
   test-only Cargo feature they need, and that feature changes no code of
   the executable. rust-ci's clippy steps, which enable every feature,
   compile these targets in both jobs (on Linux as empty files, the tests
   being Windows-only); `cargo test --all` and the elevated gate's
   `--ignored` run neither build nor run them. They run one at a time. The gate's commands stay as
   they are, and no service or account is created on the development host.
   On the runner, which is discarded, a test may use the fixed paths of
   §Data contracts §2.
   - **Evidence.** Their run in that job is their evidence. Compensating
     control §1 of CLAUDE.md asks to see them fail locally against the tree
     without the implementation, which this venue rules out. Manuel
     approved, in chat on 2026-11-01 ("Sí, PR sonda en CI"), a run in the
     same job instead: a step that cannot fail the job, on a probe pull
     request closed without landing, the probe pattern of SPEC-017's second
     Amendment 2026-10-19. That step's log is the red evidence. On the branch
     that lands, the step fails the job when a test fails. This is an
     exception to control §1 and to §Test scenarios' "observe the
     missing-behavior failures locally". Before the probe and before any
     such test lands, CLAUDE.md's control §1 states the exception and its
     *Local pre-commit gate* names these tests among those only CI runs,
     through Manuel's owner review (*Integration path* §4).
   - **What the job does not give.** The job runs the real SCM and SYSTEM
     against the test's own enrollment and heartbeat server. It gives no
     real ingest (that runner offered no container runtime for the
     backends' Linux images, ADR-0010's Amendments 2026-05-29 and
     2026-10-19), no reboot and
     no OS shutdown, so no real `SHUTDOWN` control. The service against a
     real ingest (install_ac_003, and
     §Test scenarios' "Real Windows plus ingest/Docker" line for the
     service) and the "Disposable Windows VM" experiments run on the
     isolated Windows machine where Manuel runs those experiments, once he
     provides it (like the Windows 10 and Server 2019 environments of Open
     question 2; #50, #59), never as a service on the
     development host; F2's completion therefore waits for that machine. On
     the development host the elevated gate keeps running the console
     agent. By scope this amends SPEC-023 deploy_ac_021: once
     `docs/operations/install.md`'s endpoint steps are the setup and the
     service, they run on that isolated machine with install_ac_016, not in
     the elevated gate; until then they stay the gate's console steps.
   - **The package's tests**, which install the MSI and its fixed-name
     service, keep §Test scenarios' "Packaging CI" line.

   Rejected: `#[ignore]`d tests left out of the gate by a name filter (a
   change to the gate's command that a misnamed test would pass); Manuel's
   machine (a service on the development host); a mocked SCM (insufficient
   by §Test scenarios).
5. **Open questions 4 and 5.** Neither changes an ADR. The agent does not
   read, manage or report its host's audit policy (ADR-0019 §Out of scope
   excludes managing and reporting it). The product writes, collects and
   sends no crash or memory dump, which no ADR adds (ADR-0020 Open question
   4 leaves the matter open). The endpoint runbook of install_ac_016 reads,
   with administrator rights and without changing anything, and records
   with the installation evidence:
   - the effective success and failure auditing of the Logon subcategory
     and of the Account Lockout subcategory, which logs the failures of
     locked-out accounts, queried by their GUIDs
     `{0CCE9215-69AE-11D9-BED3-505054503030}` and
     `{0CCE9217-69AE-11D9-BED3-505054503030}`, since their names are
     localized; the effective policy includes one that Group Policy
     applied;
   - whether any account has a per-user audit policy, which can exclude
     its logons from auditing, and how many do, without their names;
   - the kernel dump settings under
     `HKLM\SYSTEM\CurrentControlSet\Control\CrashControl`, and the page
     file's and hibernation's settings, since those files can also hold
     the agent's memory;
   - Windows Error Reporting's `LocalDumps` settings, global and for
     `cg-agent.exe`, and its consent and upload policy, which decides
     whether reports and their dumps leave the host.

   It records each subcategory's success and failure auditing apart: one
   not enabled is that coverage absent (Logon's success, 4624; Logon's and
   Account Lockout's failure, 4625), and one it could not read is unknown. It states that a dump of the agent's
   process can hold its decrypted private key and the events it holds, and
   a kernel dump any memory of the host, and that who reads such files and
   how long they are kept follow the host's own policy, which it records.
   This SPEC's Open questions 4 and 5 are resolved for F2 by this
   disposition, which supersedes §Operational §5's "remain explicitly
   reopened below". The agent's reporting of its host's audit coverage
   reopens with F3's agents page or a buyer's requirement, and the dump
   policy when a buyer asks the product to collect dumps; any change to either
   ADR's boundary needs Manuel's explicit yes. Rejected: a startup
   audit-policy check in the agent (it would change ADR-0019's boundary);
   turning dumps off from the product (a change to host policy, which
   §Operational §5 excludes).

**Effect.** Open question 3 is resolved but for the pin's value: WiX v7,
with its EULA and fee accepted by Manuel. For WiX v7 alone, his approval of
its fee takes the place of §Operational §5's "does not authorize a paid
tool". The packaging change names the
exact WiX release, its extensions' and the .NET SDK's versions in the
change itself, before WiX is restored or run, and checks that the pinned
release's EULA ID is `wix7`, the EULA Manuel accepted; for the pin's value
alone, this supersedes Open question 3's "decide and record before the
package build change". Open questions 4 and 5 are resolved for F2, and
ADR-0019 Open question 1 and ADR-0020 Open question 4 stay open in their
ADRs. §Test scenarios' local red run gains point 4's exception, which
CLAUDE.md's control §1 and its *Local pre-commit gate* are to state,
through owner review, before the probe and before the first such test
lands. install_ac_015 gains the order of point 3; install_ac_014's version
tests gain point 2's refusal and the resource's values; install_ac_016's
inspection is named.
When the test-only feature lands, CLAUDE.md's *Environment facts* gain
these tests, which only rust-ci's Windows job runs. No ADR is changed, and no
dependency of `cg-agent` or setting a deployment or an operator chooses is
added. Third-party pieces enter under the delegation and are listed for
Manuel's audit: at build time the .NET SDK, a runtime, and WiX's tool, its
MSBuild SDK and its extensions; on the buyer's endpoints, where they run
elevated, the WiX binaries the setup or the MSI carries (Burn's engine, an
extension's custom action). Points 1,
2, 4 and 5 are taken by Claude Code under Manuel's delegation of
2026-11-01, with his approvals quoted above; point 3 is his. Status stays
**Accepted**.

## Amendment 2026-11-01 (second): the service's outcomes

**What surfaced it.** Implementing §Operational §3 (install_ac_007,
install_ac_009 and install_ac_010) needs outcomes it names without fixing:
the exit codes of a second live instance, of `--service` without the SCM,
of an incomplete stop and of a failed service log; the states the SCM
reports; the entries of the Application log.

**Amendment.**

1. **The instance lock.** On the secure path, before the identity step and
   so before the capture start reclaims the agent's fixed ETW session
   (ADR-0008 §Decision part 2; SPEC-017 Amendment 2026-10-27), the console
   and the service take one lock per host; the service takes it before it
   opens its file log, and so before it reports `RUNNING`. It lasts only as long as the process that took it,
   and only an elevated administrator's token or SYSTEM, the principals of
   ADR-0010 §Decision part 1, can create it, so an ordinary user can
   neither take nor hold it. Only a start that holds the lock reclaims
   (ADR-0008 Amendment 2026-11-02). A platform without capture takes no lock.
   - **Held.** When another live process holds it, the agent exits with
     code **10** and
     `cg-agent: another CyberGuard agent instance is running on this host`
     on stderr, and in its log for the console; the service has no file log
     when it takes the lock, so it reports the cause through `STOPPED` with
     that code and entry 1002 (item 4). It exits before it enrolls and
     without stopping any ETW session.
   - **Without the lock, for privilege.** When its token is an ordinary
     user's or an administrator's unelevated one, and cannot control trace
     sessions (the next bullet is checked first), the agent goes on
     without the lock, through the identity step, to an ETW start without
     the reclaim. That start fails for privilege, or because a session of
     the agent's name already exists, which such a token may not reclaim.
     Both are ADR-0010's insufficient privilege (§Decision part 1,
     §Compliance; its Amendment 2026-11-02, item 2): code 9
     with SPEC-005 AC-002's line. After a failed start it stops the
     session of its name only when the start failed for privilege, the
     case where its own start left that session (SPEC-017 Amendment
     2026-10-13). It stops no session it did not start, so it never stops
     a live instance's capture (ADR-0008 Amendment 2026-11-02).
   - **ETW control outside ADR-0010's model.** A token that cannot create
     the lock but can control trace sessions (a member of Performance Log
     Users, LocalService, NetworkService, or a holder of
     `SeSystemProfilePrivilege`) could stop or keep a live instance's
     session. The agent does not start with it: code 1 and
     `cg-agent: instance lock failed: this token controls ETW sessions but is not an elevated administrator's or SYSTEM's`,
     before the identity step. ADR-0010 names those memberships as enough
     to open the provider (§Context) and requires an elevated local
     administrator (§Decision part 1), whose detection it places at the
     first ETW open (§Compliance). For these tokens §Decision part 1
     is amended by ADR-0010's Amendment 2026-11-02, item 1: they are refused
     before the ETW open, with code 1 and that line. A principal that a host administrator grants
     trace-session control through ETW's own security descriptors is not
     detected, and is outside that model.
   - **Any other failure** to take the lock ends the start with code 1 and
     `cg-agent: instance lock failed: <step>: Win32 error <code>`, since the
     agent cannot establish that no live instance holds the lock.

   After a crash the lock is free again, and the next elevated start takes
   it and reclaims the orphaned session of the agent's name; no session of
   another name is stopped. A session of the agent's name left behind for
   any other reason waits for that start too. The plain development path,
   which has no capture, takes no lock. Rejected: the lock where the
   capture starts (a second elevated instance would enroll first, using up
   its token and leaving an agent on the server); the reclaim without the
   lock (a token Windows let stop sessions could stop a live instance's).
2. **`--service`.** It runs only when the SCM started the process.
   Otherwise, and on a platform without an SCM, the agent exits with code
   **11** and
   `cg-agent: --service requires a start by the Windows service control manager`,
   without running the console path; this is checked before the
   configuration. In service mode a `--config` that is
   not an absolute path is a configuration error, code 2, with the line
   `cg-agent: invalid config: --config must be an absolute path in service mode`.
3. **The SCM states.**
   - `START_PENDING` lasts until the configuration is loaded and valid,
     the lock is taken and logging has started: for `RUNNING`, that is the
     local initialization of §Operational §1. Then `RUNNING`, accepting
     `STOP` and `SHUTDOWN` and no other control, before the identity step,
     enrollment and capture, which may wait.
   - After `STOP` or `SHUTDOWN`, `STOP_PENDING` lasts until the stop ends
     within the 60-second deadline of §Operational §3. A `STOP` or
     `SHUTDOWN` during enrollment, its retries or the wait between
     enrollment rounds (SPEC-002 Amendment 2026-10-27) ends it promptly as
     a clean stop; a token whose enrollment request was in flight may
     already be consumed (§Operational §2).
   - In both pending states the checkpoint advances at least every 10
     seconds, with a wait hint of at most 30 seconds.
   - `STOPPED` carries `dwWin32ExitCode` 0 after a clean stop. Otherwise it
     carries `ERROR_SERVICE_SPECIFIC_ERROR` (1066), and
     `dwServiceSpecificExitCode` is the code of the cause: the console's
     exit code for a cause the console shares (1 to 10); **12** when the
     stop deadline expired; **13** when the service's file log cannot be
     opened at startup or keeps failing to write (§Operational §3), with
     the line `cg-agent: the service log cannot be written: <reason>`.
   - Whether the SCM restarts a service that reported `STOPPED` with a
     failure code is the installer's setting, decided with the package
     (install_ac_008); it must not restart a deliberate stop, code 12
     included, nor a failure that §Operational §3 says requires operator
     action (invalid config, rejected tokens, corrupted identity, failed
     custody or trust, missing capture privilege) (§Operational §3). Until it restarts or an operator acts, the endpoint
     is not covered; entry 1002 below is what monitoring watches for.
4. **The Application log.** In service mode the agent writes these entries
   under the source `CyberGuardAgent`, before and after file logging works:
   - 1000, information: it reports `RUNNING`, with its version;
   - 1001, information: it stopped cleanly;
   - 1002, error: it stopped on a failure other than the expired deadline,
     with its code and the cause's line, as below;
   - 1003, error: the stop deadline expired, with the work still pending
     and the number of events not delivered.

   The cause's line is the one the console path writes to stderr, or the
   service's own line above, except that no entry carries a secret (a
   token, a key) or a line of the configuration file. A configuration
   error's line can quote the file, so for it entry 1002 names the file,
   and the line and column of the error when the parser gives them; the
   operator reads the parser's message with the file in hand. The Application log is readable by the host's
   interactive users, and any of them can write entries under any source:
   these entries are diagnostics, not evidence. Until the installer
   registers the source, Windows records them and their strings without
   message text. The console path writes none. Console Ctrl+C keeps its
   semantics (SPEC-017 Amendment 2026-10-27), so codes 12 and 13 are the
   service's alone.

**Effect.** Codes 10, 11, 12 and 13 are new, and every existing code keeps
its meaning. Code 9 gains one cause (an unlocked ETW start refused because
a session of the agent's name exists), code 1 two (the refusal of a token
that controls trace sessions outside ADR-0010's model, and any other
failure to take the lock), and code 2 one (a `--config` that is not an
absolute path, in service mode). By scope:

- SPEC-005 §Failure modes, SPEC-017 §Operational §1 (code 9 for Win32
  errors 5 and 1314 alone, code 1 for any other start failure) and SPEC-017
  capture_ac_001: for a token without the lock, an ETW start refused
  because a session of the agent's name exists is code 9 with AC-002's
  line, where it was code 1, or code 9 when the token could reclaim a
  session it had left itself; such a session now stays for the next start
  that holds the lock. A token that holds the lock keeps code 1 for that
  refusal.
- SPEC-017's Amendment 2026-10-13: without the lock, the stop after a
  failed start follows only a privilege failure.
- ADR-0010 §Decision part 1: its detection is unchanged for its principals
  and for ordinary or unelevated tokens that cannot control trace sessions.
  For a token that controls trace sessions outside that model, part 1 is
  amended by ADR-0010's Amendment 2026-11-02, item 1: it no longer reaches
  the ETW open, and ends with code 1 before the identity step.
- §Operational §1's "local initialization" is, for `RUNNING`, the
  configuration, the lock and logging.

The exit-code table of the installation runbook
(`docs/operations/install.md`) gains the new codes with the code that
introduces them. install_ac_007, install_ac_009 and the Application-log
half of install_ac_010 are tested against these outcomes. No setting a
deployment or an operator chooses is added. ADR-0010 calls the service's
lifecycle and its Event Log integration packaging, on Manuel's list:
Manuel approved this amendment in chat on 2026-11-01 ("OK a la segunda
enmienda"), and the refusal of tokens that control trace sessions outside
ADR-0010's model ("Rechazarlos al arrancar"). The two ADR changes it needs
landed first, ratified by him on 2026-11-02 ("sí a las dos enmiendas,
aterrízalas"): ADR-0008 Amendment 2026-11-02 (the reclaim, and the stop after
a failed start) and ADR-0010 Amendment 2026-11-02 (items 1 and 2 above).
Status stays **Accepted**.

## Amendment 2026-11-04: the service's paths, custody checks and identity writes

**What surfaced it.** Debt #142: the service accepts any identity and
trust-anchor paths (a relative one resolves against `System32`) and checks
no custody before it reads the configuration, the trust anchor or the
identity. It writes the identity with `std::fs::write` into a directory
that `create_dir_all` made under the data root, so each file holds the data
root's inherited Administrators entry until it is hardened after the write
(#87's write-before-hardening window). And the buffer that machine-scope
DPAPI returns is freed without being zeroed. Implementing §Data contracts
§2 and §Operational §2 for the service (install_ac_002, install_ac_004 and
install_ac_005) needs what those sections name without fixing: when the
agent is in installed mode, how it compares a configured path with a fixed
one, what a custody check accepts, the order and exit code of each refusal,
and the order in which an identity is written.

**Amendment.**

1. **Installed mode is the service.** `--service` (the second Amendment
   2026-11-01, item 2) is installed mode; the console keeps its paths and
   checks (SPEC-002 Amendment 2026-10-27: console and test paths are a
   development surface), and items 2 to 6 do not apply to it. In installed
   mode the agent derives its paths from `%ProgramData%` as the OS reports
   it (`SHGetKnownFolderPath`, after item 2's check of a relative
   `--config`; a failure is code 1, before the configuration is read): `<ProgramData>\CyberGuard`, the data root
   `<ProgramData>\CyberGuard\agent`, and in the data root `agent.toml`,
   `server-ca.pem` and `identity\` with `cert.pem`, `key.dat` and
   `identity.json`. §Data contracts §2 gives these as default locations
   and §Data contracts §3 has setup supply them without an override; in
   installed mode they are the only ones accepted, a narrowing of §2's
   "default". Setup does not create `identity\`; the service does (item 4).
2. **The configured paths are the fixed ones.** The service refuses a
   `--config` other than the fixed `agent.toml`, before any other check of
   item 3. After parsing the configuration, it refuses one without
   `[enrollment]` or `server.trust_anchor_path` (the plain path, as
   before), then one whose `server.trust_anchor_path`,
   `enrollment.cert_path`, `enrollment.key_path` or
   `enrollment.identity_path` is not its fixed path, then one without
   `server.heartbeat_url` (§Data contracts §2's "explicit HTTPS enrollment
   and heartbeat URLs"; the fallback to `server.url` of SPEC-003's
   Amendment 2026-05-22, "separate enroll and heartbeat URLs", stays the
   console's). A path is its fixed path when the two strings are equal but
   for ASCII case; any other spelling (forward slashes, `..`, a `\\?\` or
   UNC prefix, a short name, a case variant outside ASCII) is refused, not
   resolved. Each refusal is code 2, before `RUNNING`: a path other than
   its fixed one with the line
   `cg-agent: invalid config <file>: <key> must be <fixed path> in service mode`
   (`<key>` is `--config` for the first), the missing heartbeat URL with
   `cg-agent: invalid config <file>: the service requires server.heartbeat_url`.
   Where the second Amendment 2026-11-01 refused only a `--config` that is
   not absolute, the service now refuses every `--config` but the fixed
   one; that amendment's line stays for a relative one, which is checked
   first.
3. **The custody check.** The service opens each object it checks without
   following a reparse point and checks it through that handle: a
   directory where one is expected, otherwise a regular file with one
   link; not a reparse point; and a DACL, not null, with allow entries for
   the object's principals and no one else (an object need not have an
   entry for each). Deny entries may name anyone; an entry of another type
   is refused. Per object:
   - `<ProgramData>\CyberGuard` and the data root: owned by SYSTEM or
     Administrators; allow entries for SYSTEM and Administrators only. This
     is the check the service's file log has made of its directories since
     it landed, which this item states; a refusal is code 13, the code of a
     file log that cannot be opened (the second Amendment 2026-11-01, item
     3), with its line, `cg-agent: the service log cannot be written: <reason>`.
   - `agent.toml` and `server-ca.pem`: owned by SYSTEM; a protected DACL;
     allow entries for SYSTEM and Administrators only.
   - `identity\` and its three files: owned by SYSTEM; a protected DACL;
     allow entries for SYSTEM only (§Data contracts §2). Another file in
     `identity\` is neither checked nor read. These checks run before item
     4's check that the set is complete.

   Once an object and its parents pass, an ordinary user can neither
   replace nor change them, so the service may read them again by path.
   The order and the codes:
   - Before the configuration is read, after item 2's check of `--config`:
     `<ProgramData>\CyberGuard`, then the data root, each when it exists
     (code 13 as above; when either is absent, so is `agent.toml`, code 2);
     then `agent.toml`, which the service reads from the checked handle:
     code 2, `cg-agent: invalid config <file>: <reason>`. Then item 2's
     checks after parsing, the instance lock, and the file log, which checks
     its directories again as it opens them.
   - After `RUNNING`, at each start's identity step, whether or not it
     enrolls, once per start and before the first enrollment round:
     `server-ca.pem`, code 6,
     `cg-agent: tls: client configuration failed: <reason>`; then
     `identity\` when it exists and each of its three files that exists,
     code 5, `cg-agent: identity error: <reason>`.

   The reasons name the path and the failed condition, never a file's
   content.
4. **Writing the identity.** After enrollment the service creates
   `identity\` with its final security: owner SYSTEM and a protected DACL
   whose only entry, inherited by its files, is SYSTEM's. Then it seals the
   key and writes `cert.pem`, then `key.dat`, then `identity.json`, each
   created new (never over an existing file) with its final security, owner
   SYSTEM and a protected DACL for SYSTEM alone, and flushed before the
   next. In installed mode the identity set is absent only when `identity\`
   is: an `identity\` with some of the three files but not all, none
   included, is incomplete, and an `identity.json` that does not parse is
   corrupt. Either is code 5 at the next start, without enrollment,
   heartbeat or change to what is there, and the line names the repair
   (restore the identity, or remove `identity\` and enroll with a new
   token). So any write that stops after `identity\` is created leaves a
   detectable state. The one that leaves nothing, a failure to create
   `identity\`, is §Operational §2's consumed token with its enrollment
   response lost: nothing was persisted, and the next start enrolls again
   with that token (code 3). No byte of the identity is written under
   another security, and no file is restricted after it is written.
5. **Mutual consistency.** In installed mode an identity is consistent
   when its certificate's Ed25519 public key is the key's, its certificate's
   subject has one common name, `identity.json`'s `agent_id`, and the
   fingerprint in `identity.json` is that key's. The agent reads those two
   fields of the certificate with a DER reader of its own, with no new
   dependency, and validates nothing else of it there (its signature and
   validity stay with TLS, SPEC-003). At enrollment, a certificate in the
   server's answer that fails either of the first two is a malformed
   answer, code 3, and nothing is written. At the identity step, a loaded set that fails
   them is code 5, with item 4's repair. The console keeps FR-008's
   fingerprint check alone.
6. **Token hygiene.** In installed mode FR-014's rewrite of `agent.toml`
   creates its temporary file, `agent.toml.hygiene-tmp` in the data root,
   new, with the owner and DACL read from the original through its handle,
   as a protected DACL, and replaces the original with it; a leftover file
   of that name is deleted first, which deletes no reparse point's target.
   A failure stays FR-014's best-effort warning. The console's rewrite is
   unchanged.
7. **Decrypted key bytes.** In every mode, the buffer that machine-scope
   DPAPI returns is zeroed before it is freed.

**Effect.** install_ac_002 is met for the service's refusals: plain HTTP,
paths other than the fixed ones, and a configuration or trust anchor that
others can reach or change. Its "no token in logs" holds for the service
as for the console (the redacted configuration, and entry 1002 quoting no
line of the file). Its setup half (the two routes, the anchor's
fingerprints, the bootstrap file and its ACLs, no token in argv,
environment, MSI properties or MSI logs) stays with the package's setup and
MSI. install_ac_004 and install_ac_005 are tested against
items 3 to 6 for a fresh service installation: by unelevated tests of the
checks, the order of writes and the consistency, and by the service and
SYSTEM tests in rust-ci's Windows job (Amendment 2026-11-01, point 4),
which on the runner use the fixed paths and probe effective access under a
standard user created for the test and a filtered administrator's token.
That runner runs as the built-in Administrator, which has no linked
filtered token, so the test builds one with `CreateRestrictedToken`
(Administrators for deny only, `DISABLE_MAX_PRIVILEGE`, which disables
every privilege but `SeChangeNotifyPrivilege` without deleting them). It
differs from a UAC filtered token of an enrolling administrator: its user
is the built-in Administrator, its integrity level stays high, and it
enables fewer privileges; the evidence says so. The
service tests that landed with install_ac_007, install_ac_009 and
install_ac_010 move to the fixed paths too; §Test scenarios' isolated
temporary roots stay for every other test. Item 7 has no test: a freed
buffer cannot be observed, and the change is read in review. Debt #142 is
closed by the implementation. #87 is not: its legacy migration
(install_ac_013), the package, and §Operational §2's disclosure of earlier
copies remain; the console runbook's layout, with the identity files
directly in the data root and forward-slash paths, is not installed mode's,
and moving a host from it is that migration's. A host the console runbook
set up keeps the threat model's R17 outcome, code 13, since its data
directories fail the check before its configuration is read. By scope:
§Operational §2's "a detectable incomplete state" and install_ac_005's
"does not re-enroll" for the failure to create `identity\` (item 4), the
default locations of §Data contracts §2 (item 1), SPEC-003's Amendment
2026-05-22 "separate enroll and heartbeat URLs" (its fallback is the
console's), SPEC-002 FR-001, FR-007, FR-008 and FR-014 for installed mode
(as its Amendment 2026-10-27 already announces), and the second Amendment
2026-11-01, item 2. Codes 1, 2, 3, 5 and 6 gain the causes above and keep
their meanings; code 13 keeps its causes and is reached earlier for the
data directories; no code is added. The installation runbook's exit-code
table gains these causes, and the note that an editor that replaces
`agent.toml` instead of rewriting it gives it a new owner and DACL that the
next start refuses, with the change that implements them. No setting a
deployment or an operator chooses is added, no dependency of `cg-agent`
and no privilege. Like the second Amendment 2026-11-01, whose outcomes it
extends, this amendment is Manuel's to approve: he approved it in chat on
2026-11-04 ("Sí, apruebo"). Status stays **Accepted**.

## Amendment 2026-11-05: the legacy migration

**What surfaced it.** One remaining part of debt #87, install_ac_013.
§Operational §2 asks for "a documented, explicit elevated migration on the
same host" that verifies the manual installation's identity and trust
configuration and transfers them "through private staging into SYSTEM
custody before starting the service", and install_ac_013 for a default
refusal, the restriction of every remaining product-owned copy and a
recoverable original on failure. Neither says who runs the migration, how
it is asked for, what it accepts, in what order it changes the host or what
each failure reports. The manual installation is the one the console
runbook sets up (`docs/operations/install.md`): an elevated `New-Item`
creates `C:\ProgramData\CyberGuard` and its `agent` directory with the ACL
they inherit from `%ProgramData%`, which lets ordinary users create files
and directories in them; an elevated administrator puts `server-ca.pem` and
`agent.toml` there, with no ACL of CyberGuard's; and the console agent
writes `cert.pem`, `key.dat` and `identity.json` directly there, each with
the protected DACL it sets, SYSTEM and the account that ran it, owned by
that account's default owner. The service's data root is that same
directory (Amendment 2026-11-04, item 1), and the service started there
reports code 13 (the threat model's R17).

**Amendment.** Below, `<P>` is `<ProgramData>\CyberGuard`, the data root is
`<P>\agent`, the staging directory is `<P>\agent.migrating`, the legacy
directory is `<P>\agent.legacy`, and the enrolling account is the SID, if
any, that the legacy identity files' DACL allows besides SYSTEM or, once
item 2.4 has restricted them, the one this migration's record holds. A console
layout is a data root that holds `cert.pem`, `key.dat` or `identity.json`
directly. Every object below is opened without following a reparse point,
and checked through that handle; an open, a security change, a rename or
a deletion that Windows makes wait on another process (an opportunistic
lock) is cancelled after 30 seconds and fails as one
the object's sharing refuses; a security change applies to the object it
names and propagates to no child; a reason names a path and the failed
condition, never a file's content or a SID.

1. **The service migrates, asked by its start arguments.** The service
   performs the migration, as SYSTEM, while it reports `START_PENDING`,
   advancing its checkpoint as the second Amendment 2026-11-01, item 3,
   requires; no other process writes the migrated objects, and no
   privilege is enabled (ADR-0010 §Compliance). A migration starts only on
   a start whose SCM start arguments after the service's name, which only
   the holders of the service's start right (SYSTEM and elevated
   Administrators, §Data contracts §3) can pass and which the SCM does not
   keep, are exactly:
   - `--migrate-legacy=remove` or `--migrate-legacy=keep`, once (item 5);
   - `--trust-anchor-sha256=sha256:<64 lowercase hexadecimal digits>`, one
     or more, with the form and the exact-set rule of
     `setup.trust_anchor_sha256` (§Data contracts §3), obtained
     independently through SPEC-023's CA distribution procedure.

   Any other start argument, either kind without the other, migration
   arguments on a host that is neither a console layout nor a migration
   in progress (item 4), or arguments other than those a migration in
   progress recorded, are code 2 with
   `cg-agent: invalid migration arguments: <reason>`, before any change;
   the line never repeats an argument's value. Start arguments carry no
   secret. The package's setup is the operator's route: it asks for the
   choice and the fingerprints, installs without creating or changing
   anything under `<P>`, and starts the service with them once the MSI
   transaction has committed, not from an MSI action. Until the package
   exists the migration runs only in the tests and on the isolated machine
   of the Amendment 2026-11-01, point 4; the operator runbook documents it
   with the package.
2. **The route.** On a migrating start the service checks `--config` as
   the Amendment 2026-11-04, item 2, does before parsing, reads the names
   under `<P>` (item 4), takes the instance lock (code 10 while an elevated
   console agent runs) and then goes through the steps below in order. On
   every start that finds a migration in progress the lock is likewise
   taken before any rename or deletion of items 4 and 5.
   1. **Closes the directories.** `<P>` and the data root must be
      directories, not reparse points, owned by SYSTEM or Administrators,
      whose DACL lets no one else change it or take their ownership (code
      13, the file log's reason, as R17; a directory owned by another
      account, or whose DACL lets one, is refused with code 13 and item
      2.2's repair). The service gives each
      owner SYSTEM and a protected DACL that allows SYSTEM and
      Administrators, or stops with code 13 when it cannot. From here on no ordinary user can create an entry in
      them, through a handle opened before or after. This change is not
      undone: reopening them would reopen R17, and an elevated console
      agent keeps working under them.
   2. **Checks what is there** (code 5 unless shown): `<P>` holds no entry
      but the data root, and, on a continued migration, the staging
      directory item 4 accepted; the data root, with case-sensitive lookup
      off, holds `agent.toml`, `server-ca.pem`, `cert.pem`, `key.dat`,
      `identity.json` and, optionally, `agent.toml.hygiene-tmp`, and
      nothing else (another entry is named, neither read nor deleted, and
      refused with the repair: an administrator who has checked that it is
      not the agent's removes it); each
      of those files is a regular file with one link, owned by SYSTEM or
      Administrators, checked before any file is changed; the leftover
      temporary is never read or adopted, and item 2.4 and item 5 treat it
      as a copy (one that fails these checks is refused with the repair: an
      administrator deletes it, since it holds at most a consumed token); a
      file with another link is refused with the repair: an administrator
      gives it item 2.4's owner and DACL, so that no one else can link it
      again, then removes its other names;
      each identity file's DACL allows SYSTEM alone, or SYSTEM and either
      one other SID, the same in each file that has one, or Administrators,
      as item 2.4 leaves it. A
      directory or file owned by another account, as an unelevated console
      run, or a host whose policy makes an administrator's account the
      owner of what it creates, leaves it, is refused with the repair: after a restart, before
      that account signs in again, an elevated administrator of another
      account who has checked that the owner is the account that set up or
      enrolled the agent makes Administrators
      its owner and, for a directory, gives it item 2.1's DACL, then starts
      the migration again; an owner the
      administrator cannot so confirm could read and change it, and the
      host enrolls anew instead (item 6).
   3. **Reads each file once.** Each but the leftover temporary is opened for reading, sharing read
      access alone, so the open fails while any process holds it open for
      writing or deletion, or without sharing read access (code 5, the
      file named), and read whole into
      memory through that handle; everything after is checked and written
      from those bytes. `agent.toml` is a valid configuration with
      `[enrollment]`, HTTPS enrollment and heartbeat URLs, the four paths
      of the console layout, each the data root's file of its name,
      spelled with either slash and equal but for ASCII case, and no
      `setup` table (code 2; a host that relied on SPEC-003's fallback to
      `server.url` adds `server.heartbeat_url` first). The distinct
      certificates of `server-ca.pem` are exactly those of the fingerprints
      given (code 6,
      `cg-agent: tls: client configuration failed: migration: <reason>`).
      `key.dat` unseals with the machine's DPAPI key to an Ed25519 seed
      whose public key's fingerprint is `identity.json`'s and, on a
      continued migration, the recorded one, with the recorded
      `agent_id`; `cert.pem` holds one
      certificate, consistent with that key and `agent_id` (Amendment
      2026-11-04, item 5); `identity.json`'s `expires_at` is in the future
      (code 5).
   4. **Restricts the console installation in place.** Each file gets
      owner SYSTEM and a protected DACL that allows SYSTEM and
      Administrators, through a handle whose file ID is the one checked.
      The enrolling account's unelevated processes, and ordinary users,
      can no longer open them; an elevated console agent still can, so a
      refusal leaves a console installation that runs.
      Each file's link count is read through its handle before the change
      and again after it; a file the service cannot restrict, or that has
      more than one link either time, refuses the migration (code 5, with
      item 2.2's repair for a link).
   5. **Records and stages.** It creates the staging directory, new (on
      a continued migration it keeps the staging directory and its record,
      clearing any reversal mark, and replaces the staged files), owned by SYSTEM with a protected DACL that allows SYSTEM and
      Administrators, and in it `migration.json`, owned by SYSTEM with
      `agent.toml`'s policy and flushed: the choice, the fingerprints given, `agent_id`,
      the key's fingerprint, the enrolling account's SID, read in item 2.2
      before item 2.4 changes the DACLs, and the checked files' IDs (on a
      continued migration, this attempt's). A crash between items 2.4 and
      2.5 loses that SID, which the record then gives as unknown. Then, each object new, with its final security (the
      Amendment 2026-11-04, item 3, for each) and flushed: `agent.toml`,
      generated from the legacy configuration with the fixed paths, the
      legacy `agent.id`, the hostname Windows reports and no
      `enrollment.token`; `server-ca.pem`, from the verified certificates;
      `identity\` with `cert.pem`, `key.dat`, its sealed bytes unchanged,
      and `identity.json`, in the order of the Amendment 2026-11-04,
      item 4. It checks the staged tree as a start checks the data root,
      and that the staged identity loads with the legacy `agent_id` and
      key fingerprint.
   6. **Commits.** With every handle of its own under the data root
      closed, it renames the data root to the legacy directory, then
      the staging directory to the data root; the second rename is the
      commit. A failed first rename stops the start (code 5), the console
      installation in place. Before reversing a failed second rename it
      records the reversal in `migration.json`, renames the legacy
      directory back, clears the mark, and stops the start (code 5); when
      the rename back fails, the next start retries it (item 4).

   Each refusal before the commit says the console installation is in
   place, and which of its objects item 2.1 or 2.4 restricted, except after
   a failed rename back, whose line says that the data root is at the
   legacy directory's name and that the next start, with or without
   arguments, puts it back (item 4). After the
   commit the start goes on as any start of the service, from the checks
   of the Amendment 2026-11-04, item 3, to `RUNNING`, the identity step,
   which loads the migrated identity without enrolling, and capture.
3. **The record.** `migration.json` holds the operator's request and
   what item 2.5 lists, a reversal pending (item 2.6) and the copies not
   reached (item 5); which step a start has reached is otherwise read from
   which directories exist (item 4). It is created, and flushed, before the staged files,
   rewritten as the Amendment 2026-11-04, item 6, rewrites `agent.toml`,
   keeping its owner and DACL, and moves with the staging directory into
   the data root, where, after the commit, it passes `agent.toml`'s
   policy too, or the start refuses it with code 2 like a configuration
   file.
4. **Interruptions and their completion.** On every start, after the
   checks of `--config` before parsing and before the data directories'
   check, the service reads which of the data root, the staging and the
   legacy directory exist. A staging or legacy directory counts only when
   it is a directory, not a reparse point, SYSTEM owns it and its DACL
   passes `agent.toml`'s policy; one that does
   not is not used, read or deleted, and the start goes on as if it were absent, so a name an
   ordinary user created changes no outcome of a start without migration
   arguments (item 2.2 refuses it on the migrating route). A staging
   directory that counts holds a migration in progress when its
   `migration.json` passes that policy and parses; without one, beside a
   console-layout data root, it is the service's own creation
   interrupted, and a start with migration arguments deletes it, without
   following a reparse point, and migrates, while a start without them is
   code 5, which names it. Then:
   - a data root alone, or none of the three: no migration in progress; a
     start with migration arguments on a console layout migrates (item 2),
     and any other start is the Amendment 2026-11-04's (this amendment's
     item 6 for a console layout);
   - a staging directory beside a console-layout data root, an
     interruption before the first rename or after a reversal: a start
     with the recorded arguments continues from item 2.1; a start without
     them is code 5, which names the interrupted migration;
   - no data root, a staging and a legacy directory, an interruption
     between the renames: when `migration.json` records a reversal, the
     next start, with or without arguments, renames the legacy directory
     back, clears the mark and stops with code 5; otherwise it completes the commit once
     the staged tree passes item 2.5's checks, or stops with code 5 and
     names both directories;
   - a data root that is not a console layout, with a legacy directory
     and the data root's `migration.json`: a migration committed; the
     start runs, and item 5 continues;
   - any other combination: code 5, which names the directories, and
     nothing is changed.

   The start that the SCM's automatic start or its recovery runs carries
   no arguments; from the first rename on, the arguments the operator gave
   are the ones `migration.json` recorded, and no second request is
   needed.
5. **The old copies.** After the commit, each start, after it reports
   `RUNNING`, until no copy is left with `remove`, and always with
   `keep`, opens each object of the
   legacy directory without following a reparse point, matches its file
   ID with the record and checks it has one link, and gives it owner SYSTEM
   and a protected DACL that
   allows SYSTEM alone for an identity file and for the legacy directory,
   which holds them (§Data contracts §2's identity directory), SYSTEM and
   Administrators for the others; with `remove` it then deletes it, and the legacy directory
   once empty. A copy the service cannot open, restrict or delete, for
   instance one whose owner and DACL a handle held from before item 2.4
   changed, or one with another link, does not stop the start: it is logged as a warning in the file log, named in
   `migration.json` and retried at the next start. Deletion is not secure
   erasure. The legacy executable outside `%ProgramData%` holds no secret
   and is the package's. §Operational §4's purge removes the legacy
   directory and `migration.json` with the product's paths.
6. **The default refusal.** A start without migration arguments never
   starts a migration: on a console layout with no migration in progress
   it keeps the outcome of the Amendment 2026-11-04's checks (item 3),
   code 13 at the data directories (R17) or code 2 at `agent.toml`, and
   its reason adds
   `the data directory holds a console installation's identity files: migrate them before removing anything`.
   It completes a migration only from the first rename on (item 4). The
   package's default setup refuses a console layout before it changes the
   host (§Operational §1). A console installation the migration refuses
   for a cause item 2 gives no repair for (an identity that is expired,
   does not unseal or is inconsistent, an identity file whose DACL allows
   more, an owner the administrator cannot confirm) has one route: an administrator keeps its files aside, removes
   `<P>`, and installs anew with a new token, the old agent removed on
   the server.
7. **Evidence and disclosure.** After a migration the service's file log
   says so, with `agent_id`, the key's fingerprint, the choice and the
   copies' result; no log line or Application-log entry carries a token,
   key bytes or a SID; the enrolling account's SID is recorded in
   `migration.json` alone, readable by SYSTEM and elevated
   Administrators. The runbook that documents the migration discloses,
   for §Operational §2's "prior-copy exposure": the enrolling account's
   processes could read and decrypt `key.dat`, and replace the identity
   files, since enrollment (R1), and the migration cannot tell an identity
   they resealed on this host from the one enrolled here; every local user
   could read the identity files until the console agent restricted them,
   and `agent.toml`, with its token until the token was removed, until the
   migration; backups, shadow copies, crash dumps, the page file,
   antivirus quarantine and the console's command history (the token) may
   keep copies; a clone of the machine's disk unseals the same key; a
   handle opened before item 2.4 keeps the access it was granted, which
   for the enrolling account includes taking the copy's ownership, out of
   the service's reach; the certificate stays valid until it expires,
   there being no revocation before F4. When a compromise is suspected the
   remedy is a new enrollment and the removal of the old agent on the
   server, not the migration.

**Tests.** Unelevated tests over temporary roots, with the current user in
SYSTEM's place as the Amendment 2026-11-04's tests do and a failure injected
at each step without a Cargo feature, assert items 1 to 7: each refusal and
its code, with the console installation's bytes and place as item 2 leaves
them; a file held open for writing refused; a staging or legacy directory
not owned by the service ignored and left; the generated configuration and
anchor; each interrupted state completed, reversed or refused as item 4
says; the copies under both choices, and one the service cannot restrict
reported without stopping the start; no secret or SID in any line. A target
of the service and SYSTEM tests (the Amendment 2026-11-01, point 4),
`install_ac_013_migration`, enrolls a real console agent elevated, against
the test's own enrollment and heartbeat server, into a console-layout
fixture on rust-ci's runner and asserts: the default refusal
unchanged; item 2's refusals with the console installation in place and an
elevated console agent still starting from it; with each choice, a service
that reaches `RUNNING` and sends a heartbeat with the legacy `agent_id` and
client key without enrolling, the identity's owner and DACL, and effective
access to the migrated identity and the remaining copies denied to a
standard user and to the restricted administrator token of the Amendment
2026-11-04's tests; a restart without arguments. Its red run is a probe pull
request, as point 4 provides. A real ingest and reboots stay with the
isolated machine of point 4.

**Effect.** install_ac_013 is tested against items 1 to 7; its setup half
(the default refusal, the route that asks for the migration, which starts
the service with its arguments from the setup's own elevated code, and an
MSI that leaves `<P>` alone on it) stays with the package (install_ac_001,
install_ac_011), which also decides the migration codes'
place among those the SCM does not restart (install_ac_008). By scope:
§Operational §1's refusal of a pre-existing data directory does not apply to
the migration route on a console layout; §Operational §2's "may stop the
manual agent" becomes the refusal while it runs (item 2), its "before
starting the service" becomes "during the service's `START_PENDING`, before
`RUNNING`", its "Failure preserves a recoverable original" is item 2's
console installation in place and restricted, and its "verifies old
product-owned copies are removed with the operator's explicit migration
choice" becomes item 2.4's restriction of every copy and item 5's
restriction and removal, and §Operational §4's purge removes the staging
and legacy directories and `migration.json`; §Data contracts §2's layout,
and the Amendment 2026-11-04, item 1's, gain the staging and legacy
directories under `<P>` and `migration.json` in the data root; §Data
contracts §3's operator inputs gain the start arguments, which carry no
secret; the second Amendment 2026-11-01, item 3 (`START_PENDING` lasts
through a migration); the Amendment 2026-11-04, item 3 (on every start, the
read of item 4 precedes the data directories' check, and on the migration
route the lock precedes the configuration's checks, which apply to the
generated configuration), item 4 (the migration is a second writer of
`identity\`, with the same rules) and item 5, whose consistency check runs
on the legacy identity. Codes 2, 5, 6 and 13 gain the causes above, within
their meanings; no code or Application-log entry is added. The installation
runbook's code-13 repair says not to remove a console installation's
identity files before migrating them, or before keeping them aside when the
migration refuses them (item 6). Debt #87 closes with the implementation and the package's
half. The threat model's R1 and R17 are amended with it (its Amendment
2026-11-05). No ADR changes, no dependency of `cg-agent` and no privilege is
added; `migration.json` records the enrolling account's SID, a datum about a
person that stays on the host. Manuel took the design's four choices in chat
on 2026-11-05 (the service as SYSTEM asked by start arguments; `remove` or
`keep` required; completion forward without arguments from the commit; the
threat model in the same review); the completion, without arguments, of a
commit interrupted by a crash between the two renames extends the third.
Like the amendments it extends, this one is Manuel's to approve: asked in
chat on 2026-11-05 with the compare URL, the points of his list it touches
(those amendments, the start arguments, the security posture, the threat
model, the enrolling account's SID, that extension of the third choice and
the codes' new causes), he answered "Sí, apruebo". Status stays
**Accepted**.

## References

- [ADR-0008](../adr/0008-etw-crate-selection.md) — the startup reclaim of the agent's ETW session; its Amendment 2026-11-02 makes it depend on the instance lock.
- [ADR-0010](../adr/0010-agent-privilege-model-mvp.md) — packaging supersession and privilege baseline; its Amendment 2026-11-02 refuses tokens that control ETW outside its model.
- [ADR-0022](../adr/0022-single-node-production-deployment.md) — deployment, F2 and release limits.
- [SPEC-002](SPEC-002-agent-enrollment.md) — identity and accompanying custody amendment.
- [SPEC-017](SPEC-017-agent-capture-normal-run-path.md) — existing secure runtime and shutdown ordering.
- [SPEC-023](SPEC-023-production-deployment.md) — server TLS, trust distribution and release.
- [Release protection](../security/release-protection.md) — active version-tag freeze.
- [FireGiant: About the WiX Toolset](https://docs.firegiant.com/wix/) — the lifecycle table: consumer security fixes for v5 end on 2026-02-05 and for v6 on 2027-02-05.
- [FireGiant: Open Source Maintenance Fee](https://docs.firegiant.com/wix/osmf/) — OSMF EULA v1.1, EULA ID `wix7`, the US$10,000 threshold and how acceptance is given.
- [nuget.org: the `wix` 7.0.1 package's licence](https://www.nuget.org/packages/wix/7.0.1/License) — the OSMF EULA v1.1's text: §1 the fee, §2 its payment, §3 the source under MS-RL, §4 the Binary Release and its redistribution under MS-RL.
- [Microsoft: ProductVersion](https://learn.microsoft.com/en-us/windows/win32/msi/productversion) — three fields and numeric limits.
- [Microsoft: service state transitions](https://learn.microsoft.com/en-us/windows/win32/services/service-status-transitions) — SCM status protocol.
- [Microsoft: machine-scope DPAPI](https://learn.microsoft.com/en-us/windows/win32/seccrypto/example-c-program-using-cryptprotectdata) — a readable blob is decryptable by other local users.
