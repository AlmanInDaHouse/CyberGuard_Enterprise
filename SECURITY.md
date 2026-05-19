# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in CyberGuard, please report it privately. **Do not open a public GitHub issue.**

Send the report to **security@\<TBD\>** with:

- A description of the vulnerability.
- Steps to reproduce.
- The affected component (server service, agent, dashboard).
- Your assessment of the severity, if available.

We commit to:

- Acknowledge receipt within **5 business days**.
- Provide an initial impact assessment within 10 business days of acknowledgement.
- Coordinate a fix and a coordinated disclosure timeline with you.

Please do **not** disclose the vulnerability publicly until a fix is available and a disclosure timeline has been agreed.

## Supported Versions

| Version | Supported |
|---|---|
| _no releases yet_ | _—_ |

This table will be populated as releases are cut.

## Scope

In-scope components for this policy:

- CyberGuard Server (all services under `services/`).
- CyberGuard Agent (all crates under `agent/`).
- CyberGuard Dashboard (`dashboard/`).
- Deployment manifests under `deploy/`.

Out of scope:

- Third-party dependencies (please report directly to their maintainers).
- Issues in development branches that have not been released.

## Acknowledgement

Researchers who follow this policy will be acknowledged in the release notes of the fix, unless they prefer to remain anonymous.
