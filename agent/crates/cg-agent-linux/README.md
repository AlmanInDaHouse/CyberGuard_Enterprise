# cg-agent-linux

Linux-specific platform support for the CyberGuard agent.

Populated by SPEC-XXX-cg-agent-linux. Until then this folder is a placeholder.

Expected responsibilities:

- Process telemetry via auditd and `/proc`.
- Network telemetry via eBPF or NFLOG (decision deferred to SPEC).
- File telemetry via fanotify.
- Private key custody via Linux keyring.
- systemd unit installer hooks.

The MVP narrative targets the Windows agent; the Linux crate is staged for a post-MVP iteration.
