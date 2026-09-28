# SPEC-016: Detection rule set v1 — MVP criterion 1 (process_creation)

- **ID:** SPEC-016
- **Title:** Detection rule set v1 — MVP criterion 1 (process_creation)
- **Status:** Accepted
- **Depends on:**
  - SPEC-015 — amends its rule-document contract (§Data contracts "Rule document", `:47` at `8a0ed9f`) **by scope** with the loader contract of §Data contracts §1; the evaluator subset of its §Scope is unchanged. Resolves its Open question 1: multi-hop lineage stays out of the MVP.
  - SPEC-006 — realises its §Out of scope "Full detection bar" (`:42` at `8a0ed9f`) **by scope** and closes its Open questions 1–2 (`:248-249`); amends §Operational §2 and NFR-006-003 / NFR-006-004 (`:158-169`, `:197-198`) **by scope** with the parent resolution of §Operational §1. Its MVP rule, SC001 / SC010 and the `detect_ac_*` ACs are unchanged, except that detect_ac_001 asserts on its own rule's alert (§Operational §3).
  - SPEC-007 — the reopen condition of its Open question 2 (`:213` at `8a0ed9f`, "when the rule count grows") is met here; the decision is recorded in §Open questions.
  - ADR-0005 — §Harness obligation: every rule has a paired scenario, run in CI.
  - ADR-0012 — §Compliance: every rule declares `cg_detection_source: "rule"` (`:225`), and no Go toolchain before the firehose ADR (`:229`), so the scenario runner is a TypeScript test.
  - `docs/product/roadmap.md` — §C, the phase this SPEC is the contract for; §G and §B2, destinations named below.
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

MVP criterion 1 asks for ten operational detection rules, all with harness tests (blueprint §18). SPEC-006 shipped one rule; SPEC-015 generalized the evaluator over `Image` and `ParentImage`. Four facts, observed at `8a0ed9f`, shape this SPEC:

1. **Wiring.** No code loads `rules/tests/*.test.json` or `harness/scenarios/*/scenario.json`, and `ts-ci` does not trigger on `rules/**` or `harness/**`, so the rule and scenario conventions of `rules/README.md` and ADR-0005 are not enforced.
2. **Parent resolution.** `resolveParents` (`services/ingest/src/detect/read-model.ts`) looks back over the 300 s correlation window (`CORRELATION_WINDOW_SECONDS_DEFAULT`, fixed by `buildDetectConfig`) and resolves per batch, bounded by the batch's newest child, where SPEC-006 §Operational §2 resolves per child. A parent launched more than about five minutes before its child resolves to `null`, so a lineage rule misses a long-lived parent.
3. **Path form.** The agent copies ETW's `ImageName` into `image_file_name` unchanged (`emit_process_activity_with_cache`, `agent/cg-agent/src/cges/emit.rs`); the device-path → Win32 translation of SPEC-005 §Operational §3 does not exist. No emitted value has been recorded, so the exact form is inferred, not observed.
4. **Loader.** `parseRule` accepts unknown `cg:` keys (dropped in silence), any `level`, any tactic string, malformed technique ids, any `logsource.product`, a `condition` that holds when no block matches, and `\\` (an escaped single backslash in Sigma, matched literally here).

Capture itself runs only on the agent's test-mode path today (roadmap §G). This SPEC is server-side and does not depend on it: its ACs run on synthetic events, plus the developer-local marquee, which uses that path.

## Scope

### In scope

- **The rule set:** the SPEC-006 MVP rule plus nine rules in `rules/windows/` (§Data contracts §2).
- **The loader contract:** §Data contracts §1, enforced by `parseRule` and `loadRules`.
- **Wired rule fixtures:** one `rules/tests/<name>.test.json` per rule (§Data contracts §3).
- **Wired scenarios:** at least one positive scenario per rule in `harness/scenarios/`, each run in CI against the whole rule set (§Data contracts §4).
- **Parent resolution:** per child, with its own 24 h look-back and a termination check (§Operational §1).
- **The detect_ac_001 marquee** asserts on its own rule's alert (§Operational §3).
- **CI:** `ts-ci` runs on changes to `rules/**` and `harness/**` (§Operational §4).

### Out of scope

Each item has its destination in brackets.

- Rules over `CommandLine` or `User` [B2].
- Rules over non-process classes: network, authentication, registry, file [D and later SPECs].
- Multi-hop lineage (a grandparent or deeper); this resolves SPEC-015 Open question 1 [post-MVP; the ancestry would come from the agent, roadmap §B2].
- Parents that started before the agent's ETW session: their children keep `ParentImage = null` (SPEC-006 §Operational §2, unchanged) [roadmap §B2: an initial process enumeration and a parent stamped by the agent].
- Capture on the agent's normal run path, and the device-path → Win32 translation [roadmap §G].
- Third-party rule content. The Detection Rule License 1.1 that SigmaHQ rules carry requires the messages based on a rule's matches to keep its author and link, when the rule supplies them, and the loader drops `author` and `references` [a future SPEC, if a rule is ever imported].
- Operator-configurable windows: the "per-org configurable" of ADR-0012 §8 and NFR-006-003 is not implemented for any window [roadmap §F].
- Incident grouping changes [SPEC-007 Open question 2, reopened at roadmap §E].
- Suppression or allow-listing of alerts [post-MVP].
- The Go `cg-harness` runner [ADR-0002 Rule 3; blocked by ADR-0012 §Compliance until the firehose ADR].

## Data contracts

### 1. Rule document — the loader contract

Amends SPEC-015 §Data contracts "Rule document" by scope. A rule is standard Sigma plus the `cg:` block, in a `*.yml` or `*.yaml` file directly under `rules/windows/` (unchanged). On top of everything SPEC-015 rejects, the loader rejects at load, with `UnsupportedRuleError` naming the construct:

- an `id` that does not match `^rule\.[a-z0-9_]+$`; in `loadRules`, an `id` other than `rule.<file name without extension>`, or an `id` already loaded;
- a `logsource.product` other than `windows`;
- a `level` outside `informational`, `low`, `medium`, `high` and `critical`, or a `cg.severity_id` other than its OCSF value: 1, 2, 3, 4 and 5 respectively (`schemas/cges/v0.1/common/ocsf_severity.json`);
- a missing `cg.cg_detection_source`, or any value but `rule`: `ml` and `hybrid` need a model pairing that no rule can carry (ADR-0005), and the `alerts` table requires a `model_id` for both;
- a key in `cg` other than `heuristic_score`, `severity_id`, `cg_detection_source` and `cg_mitre`, or in `cg_mitre` other than `tactics` and `techniques`;
- a tactic outside the fourteen MITRE ATT&CK Enterprise tactics, in the kebab-case names `common/cg_mitre.json` describes (`reconnaissance`, `resource-development`, `initial-access`, `execution`, `persistence`, `privilege-escalation`, `defense-evasion`, `credential-access`, `discovery`, `lateral-movement`, `collection`, `command-and-control`, `exfiltration`, `impact`); a technique that does not match `^T[0-9]{4}(\.[0-9]{3})?$` (`common/cg_mitre.json`); or a repeated tactic or technique;
- a value containing `\\`: Sigma reads it as one escaped backslash, and the evaluator matches values literally;
- a `condition` that is true when every block is false. A rule must need at least one block to match: `not filter`, or `selection or not filter`, would fire on almost every process.

`cg.cg_detection_source` is the key that ADR-0012 §Compliance and the CGES alert use; ADR-0005 §Compliance and `rules/README.md` name the same concept `detection_source`. The tactic vocabulary is a loader constraint only; the CGES schema is unchanged.

### 2. The rule set

| # | `id` | `level` → `severity_id` | `heuristic_score` | `cg_mitre.tactics` | `cg_mitre.techniques` | Needs the parent | Scenario |
|---|---|---|---|---|---|---|---|
| 0 | `rule.office_spawns_script_host` | high → 4 | 0.9 | execution, initial-access | T1059.001, T1566.001 | yes (Office) | SC001 |
| 1 | `rule.office_spawns_lolbin` | high → 4 | 0.8 | execution, defense-evasion | T1204.002, T1218 | yes (Office) | SC011 |
| 2 | `rule.script_host_spawns_powershell` | high → 4 | 0.75 | execution, defense-evasion | T1059.001, T1059.005, T1218.005 | yes (short-lived) | SC012 |
| 3 | `rule.staged_payload_execution` | high → 4 | 0.8 | execution, initial-access | T1204.002, T1566.001 | partly | SC013 |
| 4 | `rule.deceptive_executable_name` | high → 4 | 0.85 | execution, defense-evasion | T1204.002, T1036.002, T1036.007 | no | SC014 |
| 5 | `rule.system_binary_masquerading` | high → 4 | 0.8 | defense-evasion | T1036.005 | no | SC015 |
| 6 | `rule.exec_from_suspicious_folder` | medium → 3 | 0.6 | defense-evasion | T1036 | no | SC016 |
| 7 | `rule.exec_from_startup_folder` | medium → 3 | 0.7 | persistence | T1547.001 | no | SC017 |
| 8 | `rule.psexec_like_service` | medium → 3 | 0.6 | execution, lateral-movement | T1569.002, T1021.002 | no | SC018 |
| 9 | `rule.credential_theft_tool` | critical → 5 | 0.9 | credential-access | T1003.001, T1555, T1558 | no | SC019 |

Rule 0 is the SPEC-006 MVP rule; it only gains `cg_detection_source: rule`. Each other rule lives in `rules/windows/<name>.yml` (`id` = `rule.<name>`), with `status: test`, `logsource` `windows` / `process_creation`, a `title`, a `description`, ATT&CK `references`, Sigma `tags`, and the `detection` block below. Values are written in lower case; matching is case-insensitive anyway (SPEC-015).

Authoring constraints, all met below:

- **Both path forms.** Every value matches the Win32 form (`C:\…`) and the device form (`\Device\HarddiskVolumeN\…`): no drive letter, folders matched as `\folder\` fragments, names as `\name.exe` suffixes.
- **No `not` over a `ParentImage` block.** An unknown parent would make it true (SPEC-015 §Data contracts).
- **The existing fixtures stay quiet.** No rule other than rule 0 matches the events that the existing detection, incident and notification suites insert, nor the marquee's probe.
- **The confidence follows the evidence.** It is lower where legitimate use is plausible (6, 8) and higher where it is rare (4, 9); `final_score` equals it for a rule-only alert (ADR-0012 §4).

#### Rule 1 — `rule.office_spawns_lolbin`: Office Application Spawns a LOLBin

An Office application launches a binary commonly abused to proxy execution or to fetch a payload. Outlook is not a parent here; its attachment path is rule 3's.

```yaml
detection:
  selection:
    ParentImage|endswith:
      - '\winword.exe'
      - '\excel.exe'
      - '\powerpnt.exe'
      - '\msaccess.exe'
      - '\onenote.exe'
      - '\mspub.exe'
    Image|endswith:
      - '\rundll32.exe'
      - '\regsvr32.exe'
      - '\certutil.exe'
      - '\bitsadmin.exe'
      - '\schtasks.exe'
      - '\wmic.exe'
      - '\msbuild.exe'
      - '\installutil.exe'
      - '\regasm.exe'
      - '\regsvcs.exe'
      - '\msiexec.exe'
      - '\cmstp.exe'
      - '\odbcconf.exe'
      - '\curl.exe'
      - '\forfiles.exe'
  condition: selection
```

#### Rule 2 — `rule.script_host_spawns_powershell`: Script Host Spawns PowerShell

Windows Script Host or `mshta` launches PowerShell, the usual second stage of a script or HTA dropper.

```yaml
detection:
  selection:
    ParentImage|endswith:
      - '\wscript.exe'
      - '\cscript.exe'
      - '\mshta.exe'
    Image|endswith:
      - '\powershell.exe'
      - '\pwsh.exe'
  condition: selection
```

#### Rule 3 — `rule.staged_payload_execution`: Staged Payload Executed from a Document, Script or Mail Attachment

An Office application or a script host launches an executable from a user staging folder, or any executable runs from Outlook's attachment cache. The cache branch does not need the parent.

```yaml
detection:
  document_or_script_parent:
    ParentImage|endswith:
      - '\winword.exe'
      - '\excel.exe'
      - '\powerpnt.exe'
      - '\msaccess.exe'
      - '\onenote.exe'
      - '\mspub.exe'
      - '\wscript.exe'
      - '\cscript.exe'
      - '\mshta.exe'
  staging_folder:
    Image|contains:
      - '\appdata\local\temp\'
      - '\downloads\'
  outlook_attachment_cache:
    Image|contains:
      - '\content.outlook\'
  condition: (document_or_script_parent and staging_folder) or outlook_attachment_cache
```

#### Rule 4 — `rule.deceptive_executable_name`: Executable With a Deceptive Name

An executable disguised as a document or a picture: a double extension, a right-to-left override character (U+202E), or a screensaver or PIF outside the system folders.

```yaml
detection:
  double_extension:
    Image|endswith:
      - '.pdf.exe'
      - '.doc.exe'
      - '.docx.exe'
      - '.xls.exe'
      - '.xlsx.exe'
      - '.ppt.exe'
      - '.pptx.exe'
      - '.rtf.exe'
      - '.txt.exe'
      - '.jpg.exe'
      - '.jpeg.exe'
      - '.png.exe'
      - '.zip.exe'
      - '.rar.exe'
  right_to_left_override:
    Image|contains:
      - "\u202e"
  screensaver_or_pif:
    Image|endswith:
      - '.scr'
      - '.pif'
  system_folder:
    Image|contains:
      - '\windows\system32\'
      - '\windows\syswow64\'
  condition: double_extension or right_to_left_override or (screensaver_or_pif and not system_folder)
```

#### Rule 5 — `rule.system_binary_masquerading`: System Binary Name Outside the System Folders

A process named like a core Windows binary runs outside `System32`, `SysWOW64` and `WinSxS`, and not from the kernel's `\SystemRoot\` form. `pwsh.exe` and `explorer.exe` are not listed: they live elsewhere legitimately. Anchoring on folder fragments means that a nested `\windows\system32\` elsewhere evades the rule; that is accepted.

```yaml
detection:
  system_binary_name:
    Image|endswith:
      - '\svchost.exe'
      - '\lsass.exe'
      - '\csrss.exe'
      - '\smss.exe'
      - '\wininit.exe'
      - '\winlogon.exe'
      - '\services.exe'
      - '\spoolsv.exe'
      - '\taskhostw.exe'
      - '\sihost.exe'
      - '\dwm.exe'
      - '\dllhost.exe'
      - '\conhost.exe'
      - '\rundll32.exe'
      - '\regsvr32.exe'
      - '\powershell.exe'
      - '\cmd.exe'
      - '\mshta.exe'
      - '\wscript.exe'
      - '\cscript.exe'
  system_folder:
    Image|contains:
      - '\windows\system32\'
      - '\windows\syswow64\'
      - '\windows\winsxs\'
  kernel_system_root:
    Image|startswith:
      - '\systemroot\'
  condition: system_binary_name and not system_folder and not kernel_system_root
```

#### Rule 6 — `rule.exec_from_suspicious_folder`: Execution from a Suspicious Folder

An executable runs from a folder that legitimate software rarely runs from: the public profile, `PerfLogs`, the recycle bin, or Windows data folders. `Temp` and `ProgramData` are left out on purpose, because installers and security products run from them.

```yaml
detection:
  selection:
    Image|contains:
      - '\users\public\'
      - '\perflogs\'
      - '\$recycle.bin\'
      - '\windows\tasks\'
      - '\windows\system32\tasks\'
      - '\windows\fonts\'
      - '\windows\debug\'
      - '\windows\help\'
      - '\windows\tracing\'
      - '\windows\media\'
      - '\windows\cursors\'
      - '\windows\addins\'
      - '\windows\repair\'
  condition: selection
```

#### Rule 7 — `rule.exec_from_startup_folder`: Executable Launched from a Startup Folder

An executable runs from a Start Menu `Startup` folder, per user or common. A shortcut there runs its target from elsewhere; an executable in the folder itself is a persistence foothold.

```yaml
detection:
  selection:
    Image|contains:
      - '\start menu\programs\startup\'
  condition: selection
```

#### Rule 8 — `rule.psexec_like_service`: PsExec-like Remote Service Executed

The service binary of PsExec or of a PsExec-like tool (PAExec, RemCom, CSExec) runs: remote execution over an admin share. IT uses PsExec legitimately, hence `medium`.

```yaml
detection:
  service_binary:
    Image|endswith:
      - '\psexesvc.exe'
      - '\remcomsvc.exe'
      - '\csexecsvc.exe'
  paexec_service:
    Image|contains:
      - '\paexec-'
  condition: service_binary or paexec_service
```

#### Rule 9 — `rule.credential_theft_tool`: Credential Theft Tool Executed

A credential-theft tool runs under its public name: LSASS dumping, Kerberos ticket abuse, or extraction from password stores. Renaming the binary evades the rule; it stays for commodity use.

```yaml
detection:
  selection:
    Image|endswith:
      - '\mimikatz.exe'
      - '\safetykatz.exe'
      - '\sharpkatz.exe'
      - '\nanodump.exe'
      - '\dumpert.exe'
      - '\outflank-dumpert.exe'
      - '\rubeus.exe'
      - '\kekeo.exe'
      - '\lazagne.exe'
      - '\sharpdpapi.exe'
      - '\sharpchrome.exe'
  condition: selection
```

### 3. Rule fixture

`rules/tests/<name>.test.json`, one per rule, keeping the format of the SPEC-006 fixture (`rules/tests/README.md` is corrected to it):

```json
{
  "rule": "rule.<name>",
  "description": "what the cases pin",
  "cases": [
    {
      "name": "short case name",
      "event": { "Image": "...", "ParentImage": "... or null" },
      "expected_match": true
    }
  ]
}
```

Each case is evaluated as the Launch (`activity_id = 1`) of a process with those two fields. Each fixture has:

- `rule` equal to the `id` of `rules/windows/<name>.yml`;
- at least two positive cases, at least one of them with every path in the device form (`\Device\HarddiskVolume3\…`);
- at least two negative cases, including a near miss: the right name in the wrong place, or the right child under the wrong parent;
- for a rule whose `condition` reads `ParentImage`, a negative case with `ParentImage: null`.

The SPEC-006 fixture's `rule` field becomes `rule.office_spawns_script_host`, and the fixture gains a device-form positive.

### 4. Scenario

`harness/scenarios/SCNNN-kebab-title/scenario.json`, in the format of `harness/scenarios/README.md`, which is unchanged. A scenario lists only each event's `activity_id`, `process_name`, `image_file_name`, `process_pid` and `process_parent_pid`, parents before children. The runner derives the rest:

- `org_id` is the scenario `id`; one `agent_id` per scenario, and each `event_id`, are derived deterministically from the `id` and the event's position; `time` is a fixed base per scenario plus the position in seconds;
- the agent is enrolled in the scenario's org before the cycle (the `alerts` → `agents` foreign key);
- one `runDetectionCycle` runs over the scenario's org, with the whole of `rules/windows/`.

A scenario passes when the alerts for its agent number `expected.alert_count` and, when there is at least one, every alert has `rule_id = expected.rule_id`, `cg_detection_source = rule` and `final_score = expected.final_score`. A positive scenario therefore also proves that no other rule fires on its events.

Coverage: every rule has at least one scenario with `expected.alert: true` naming it, and every scenario's `rule_id` names an existing rule. SC001 and SC010 keep their files and their meaning; the nine new scenarios are SC011–SC019 (§Test scenarios). SC002–SC009 stay reserved for the blueprint §14 scenarios, most of which need capture beyond process creation.

## Operational

### 1. Parent resolution

Amends SPEC-006 §Operational §2 and NFR-006-003 / NFR-006-004 by scope:

```text
R = the most recent Launch row (activity_id = 1) with
      R.agent_id    = child.agent_id
      R.process_pid = child.process_parent_pid
      child.time - PARENT_LOOKBACK <= R.time <= child.time
ParentImage(child) = R.image_file_name,
  or null when there is no such R, or when R's own Terminate
  (same agent_id and process_uid) is earlier than child.time
PARENT_LOOKBACK = 86 400 s (24 h)
```

- **Per child.** Resolution happens per child, as SPEC-006 §Operational §2 always specified; `resolveParents` resolving per batch was the deviation. `ParentImage` is the parent's `image_file_name`, as the SPEC-006 §Data contracts table states; the `process_name` wording of SPEC-006 §Operational §2 is superseded.
- **Termination check.** A candidate that terminated before the child was created cannot be its parent: the real parent's Launch is missing (ETW loss, an agent restart, a dropped batch), so the child's parent is unknown and the older process holding the same PID is not used. The check matches on `process_uid` (ADR-0011 §6); a Terminate whose `process_uid` does not match excludes nothing.
- **Its own constant.** The look-back is decoupled from the 300 s dedup bucket (NFR-006-004 keeps its value; its "share one tunable" clause no longer holds) and from the 1800 s incident window (SPEC-007). It is a constant, not an environment variable; per-org configurability stays out of scope (roadmap §F). ADR-0012 §8 is untouched: it assigns the 300 s window to the dedup bucket only.
- **Honesty (unchanged).** A parent that started before the agent's ETW session was never captured; its children keep `ParentImage = null` (SPEC-006 §Operational §2), and lineage rules miss them.
- **Cost.** The parent query spans up to 24 h instead of 5 min, filtered by org, agent and PID. At MVP volume this is expected to be acceptable; that is an estimate, not a measurement. The exit is a parent stamped by the agent (roadmap §B2).

### 2. Loading

Fail-closed at load and fail-loud at boot, unchanged: the rejections of §Data contracts §1 happen in `parseRule` and `loadRules`, so `assertRulesLoadable` refuses to start the driver on a rule outside the contract. The suites that start ingest with the driver enabled load `rules/windows/`, so a broken rule also fails suites outside detection; rules_ac_001 and rules_ac_002 name the construct.

### 3. The detect_ac_001 marquee

detect_ac_001 runs the whole rule set over about 40 s of system-wide capture on the developer's machine. It asserts:

- exactly one alert with `rule_id = rule.office_spawns_script_host` for the agent;
- the SPEC-006 §Acceptance criteria fields it did not assert before: `status = new`, a well-formed `dedup_key`, and `source_events` containing the `event_id` of the `powershell.exe` child.

Alerts from other rules on background activity are logged, not asserted. The test also logs the captured `image_file_name` of the probe's parent and child: the first recorded sample of the emitted path form (§Context, fact 3).

### 4. CI

`ts-ci` triggers on `rules/**` and `harness/**`, for `push` and `pull_request` alike, so a commit that changes only a rule, a fixture or a scenario runs the suites that load them.

## Acceptance criteria

Each maps to a test under `services/ingest/test/`, named `rules_ac_NNN_*`.

- **rules_ac_001 (loader contract).** Each rejection of §Data contracts §1 happens at load and names the construct; the ten rules load.
- **rules_ac_002 (fixtures wired).** Every rule has a fixture that meets §Data contracts §3, every case evaluates as expected, and every fixture has its rule. No `*.yml` or `*.yaml` sits in a subdirectory of `rules/windows/`: the loader does not recurse, so such a rule would be ignored in silence.
- **rules_ac_003 (scenarios wired).** Every scenario passes as §Data contracts §4 defines, against the whole rule set, and coverage holds.
- **rules_ac_004 (parent resolution).** A parent launched 2 h before its child resolves; a later reuse of the parent's PID, inside the same batch, does not replace it; a candidate whose Terminate precedes the child is not used; a parent launched more than 24 h before its child resolves to `null`.
- **rules_ac_005 (marquee).** detect_ac_001 behaves as §Operational §3 states. Developer-local and elevated, run by Manuel.
- **rules_ac_006 (regression).** The SPEC-006, SPEC-007, SPEC-011, SPEC-014 and SPEC-015 suites stay green and unchanged, except for the helper that builds raw rules (it gains `cg_detection_source`) and detect_ac_001 (§Operational §3).

## Test scenarios

Per ADR-0005 §Harness obligation, every scenario declares its `expected_detection_source`; all of them are rule-track. The paths below are in Win32 form, except SC013, which is written in device form.

| Scenario | Input (parent → child, or a single process) | Expected |
|---|---|---|
| SC001 (existing) | `winword.exe` → `powershell.exe` | 1 alert, `rule.office_spawns_script_host`, 0.9 |
| SC010 (existing) | `explorer.exe` → `powershell.exe` | 0 alerts |
| SC011 | `EXCEL.EXE` → `C:\Windows\System32\rundll32.exe` | 1 alert, `rule.office_spawns_lolbin`, 0.8 |
| SC012 | `C:\Windows\System32\wscript.exe` → `powershell.exe` | 1 alert, `rule.script_host_spawns_powershell`, 0.75 |
| SC013 | `WINWORD.EXE` → `\Users\alice\AppData\Local\Temp\invoice_viewer.exe` | 1 alert, `rule.staged_payload_execution`, 0.8 |
| SC014 | `C:\Users\alice\Downloads\invoice.pdf.exe` | 1 alert, `rule.deceptive_executable_name`, 0.85 |
| SC015 | `C:\Users\alice\AppData\Roaming\svchost.exe` | 1 alert, `rule.system_binary_masquerading`, 0.8 |
| SC016 | `C:\Users\Public\Libraries\update.exe` | 1 alert, `rule.exec_from_suspicious_folder`, 0.6 |
| SC017 | `C:\Users\alice\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\helper.exe` | 1 alert, `rule.exec_from_startup_folder`, 0.7 |
| SC018 | `C:\Windows\PSEXESVC.exe` | 1 alert, `rule.psexec_like_service`, 0.6 |
| SC019 | `C:\Users\alice\Desktop\mimikatz.exe` | 1 alert, `rule.credential_theft_tool`, 0.9 |

The Office parents live under `C:\Program Files\Microsoft Office\root\Office16\`, as in SC001; SC013 writes both paths behind `\Device\HarddiskVolume3`. A single-process scenario has a parent that was not captured.

## Risks

| Risk | Mitigation |
| --- | --- |
| Lineage rules (0–3) miss a child whose parent started before the agent, or more than 24 h before the child | Stated (§Operational §1); four of the ten rules need the parent; the exit is roadmap §B2 |
| Renaming or relocating a binary evades the name- and folder-based rules | Accepted for the MVP: the rules target commodity tradecraft; rule 5 is evaded by a nested `\windows\system32\` elsewhere |
| Legitimate use raises alerts (PsExec by IT, installers in `Users\Public`, admin scripts that call PowerShell) | `medium` level and lower confidence where plausible; triage by `status`; suppression is post-MVP |
| One attack that fires rules with different tactic sets opens several incidents, and one email each | Recorded (§Open questions 1); reopened at roadmap §E |
| The emitted path form is inferred, not observed | Every rule matches both forms (fixture requirement); the marquee records the first sample |
| A broken rule fails suites outside detection | The loader names the construct; `ts-ci` now runs on `rules/**` |

## Open questions

1. **Incident grouping.** SPEC-007 groups alerts by their canonical tactic set, and its Open question 2 reopens "when the rule count grows", which this SPEC does. With the tactic sets of §Data contracts §2, on one agent and inside one incident window, rules 0 and 3 share an incident, and so do rules 1, 2 and 4, and rules 5 and 6. **Decision: keep tactic-set grouping for the MVP; reopen at roadmap §E, where the incident becomes the unit of SOAR action.**
2. **Third-party rules.** Importing a DRL 1.1 rule needs its author and link carried into every message derived from a match: the alert, the email, the PDF. **Reopen when the first third-party rule is proposed.**
3. **Suppression and allow-listing per org.** Not in the MVP. **Reopen with the first field feedback on false positives.**

## Ratification record

Load-bearing decisions for Manuel's gate. Manuel delegated the choice explicitly ("elige tú", 2026-09-27); the advisor decided, and Manuel's ratification of this SPEC ratifies the decisions.

1. **A new SPEC**, amending SPEC-015 (the rule document) and SPEC-006 (parent resolution) by scope, and realising SPEC-006's "Full detection bar". Precedents: SPEC-015 → SPEC-006, SPEC-011 → SPEC-010.
2. **The ten rules of §Data contracts §2** — a product decision. Six of them do not need the parent. Web-server, WMI and browser lineage rules were set aside: their parents are long-lived processes that often start before the agent, or more than a day before the child.
3. **Criterion 1 is done** when each of the ten rules has a wired fixture and at least one CI scenario, scenario isolation holds, and the elevated marquee is green.
4. **Original rule content only**; no third-party rule in v1.
5. **The loader contract of §Data contracts §1**, with `cg.cg_detection_source` as the key.
6. **Parent resolution per child**, with the termination check and a 24 h look-back constant; no environment variable.
7. **Incident grouping unchanged**, reopened at roadmap §E.
8. **Multi-hop lineage out of the MVP** (SPEC-015 Open question 1).
9. **The scenario runner is a vitest suite in `services/ingest`**, and `ts-ci` triggers on `rules/**` and `harness/**`.
10. **Doc-only gate first.** The code is the next gate (a review branch, relay rule 5) and includes the elevated marquee.

## References

- [SPEC-006](SPEC-006-detection-mvp.md) — the MVP rule, SC001 / SC010, the parent self-join amended here by scope, and the "Full detection bar" realised here.
- [SPEC-015](SPEC-015-detection-evaluator-generalization.md) — the evaluator subset; its rule document is amended here by scope.
- [SPEC-007](SPEC-007-incident-grouping-mvp.md) — incident grouping (§Open questions 1).
- [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — the capture these rules read; §Operational §3, the translation not yet implemented.
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — §Harness obligation and §Compliance.
- [ADR-0011](../adr/0011-cges-process-activity-v0-1.md) — §5, the parent name the agent may emit; §6, `process_uid`.
- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — §4 scoring, §8 the 300 s window, §Compliance.
- [ADR-0002](../adr/0002-language-per-component.md) — Rule 3, the deferred Go runner.
- [roadmap](../product/roadmap.md) — §C (this SPEC), §G, §B2, §E, §F.
- `schemas/cges/v0.1/common/cg_mitre.json`, `schemas/cges/v0.1/common/ocsf_severity.json` — the MITRE and severity shapes the loader checks.
- [MITRE ATT&CK Enterprise tactics](https://attack.mitre.org/tactics/enterprise/) — the tactic vocabulary and the techniques cited.
- [Sigma specification](https://github.com/SigmaHQ/sigma-specification) — value escaping (`\\`).
- [Detection Rule License 1.1](https://github.com/SigmaHQ/Detection-Rule-License) — why third-party rules are out of scope.
