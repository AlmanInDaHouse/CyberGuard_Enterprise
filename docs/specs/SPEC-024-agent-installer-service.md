# SPEC-024: Windows agent installer, service lifecycle and key custody

- **ID:** SPEC-024
- **Title:** Windows agent installer, service lifecycle and key custody
- **Status:** Accepted
- **Depends on:** ADR-0010 (the packaging SPEC expressly supersedes its installation posture), ADR-0022 (F2, key custody and release protection), SPEC-001, SPEC-002, SPEC-003, SPEC-017, SPEC-019, SPEC-020 and SPEC-023.
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

## References

- [ADR-0010](../adr/0010-agent-privilege-model-mvp.md) — packaging supersession and privilege baseline.
- [ADR-0022](../adr/0022-single-node-production-deployment.md) — deployment, F2 and release limits.
- [SPEC-002](SPEC-002-agent-enrollment.md) — identity and accompanying custody amendment.
- [SPEC-017](SPEC-017-agent-capture-normal-run-path.md) — existing secure runtime and shutdown ordering.
- [SPEC-023](SPEC-023-production-deployment.md) — server TLS, trust distribution and release.
- [Release protection](../security/release-protection.md) — active version-tag freeze.
- [Microsoft: ProductVersion](https://learn.microsoft.com/en-us/windows/win32/msi/productversion) — three fields and numeric limits.
- [Microsoft: service state transitions](https://learn.microsoft.com/en-us/windows/win32/services/service-status-transitions) — SCM status protocol.
- [Microsoft: machine-scope DPAPI](https://learn.microsoft.com/en-us/windows/win32/seccrypto/example-c-program-using-cryptprotectdata) — a readable blob is decryptable by other local users.
