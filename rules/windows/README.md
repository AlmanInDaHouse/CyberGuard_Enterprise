# Windows detection rules

Sigma-compatible rules targeting Windows event sources (Sysmon, EventLog, ETW, PowerShell ScriptBlock).

SPEC-006 (Detection MVP) lands the first rule here: `office_spawns_script_host.yml` (Office → script-host lineage — the blueprint's suspicious-PowerShell case re-planted from command-line to process lineage, because command-line is empty in CGES v0.1 per SPEC-006 §Operational §4). The full 10-rule operational bar is deferred per ADR-0012 §Out of scope; future detection SPECs populate the rest.
