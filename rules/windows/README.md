# Windows detection rules

Sigma-compatible rules targeting Windows event sources (Sysmon, EventLog, ETW, PowerShell ScriptBlock).

Rule set v1 (SPEC-016, MVP criterion 1) is ten `process_creation` rules over process image and lineage: SPEC-006's `office_spawns_script_host.yml` (Office → script-host lineage — the blueprint's suspicious-PowerShell case re-planted from command-line to process lineage, because command-line is empty in CGES v0.1 per SPEC-006 §Operational §4) plus the nine rules of SPEC-016 §Data contracts §2. Each rule file sits directly in this folder (the loader does not recurse), meets the loader contract of SPEC-016 §Data contracts §1, and has its fixture in [`../tests/`](../tests/) and at least one positive scenario in [`harness/scenarios/`](../../harness/scenarios/). Rules over the command line or the user wait for B2; rules over other event classes wait for roadmap §D.
