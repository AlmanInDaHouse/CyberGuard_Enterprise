# cg-agent-windows

Windows-specific platform support for the CyberGuard agent.

Populated by SPEC-XXX-cg-agent-windows. Until then this folder is a placeholder.

Expected responsibilities:

- Process telemetry via ETW.
- Network telemetry via Windows Filtering Platform.
- File and user telemetry via system APIs.
- Private key custody via DPAPI.
- Service / scheduled task installer hooks.
- Single `.exe` distribution per the MVP narrative.
