# Operations

These runbooks describe the server bundle before its first release. Publication still waits for roadmap F2–F4 and Manuel's explicit approval. Positive verification of published artifacts and the timed fresh-host installation remain first-release gates; local fixtures and CI do not replace them.

The documents for running the CyberGuard Enterprise server on one Linux x86-64 host with Docker Engine, its Compose plugin and `gpg`, and its agents on Windows endpoints, from the bundle and the agent's executable a release publishes (SPEC-023). The operator who installs and runs the server reads [install.md](install.md) first, then [configuration.md](configuration.md), [backup-and-restore.md](backup-and-restore.md) and [upgrade.md](upgrade.md) as the work calls for them. The data protection officer reads [data-protection.md](data-protection.md) for the record of processing. The auditor who checks a forensic export against the fingerprint recorded at installation reads [forensic-verification.md](forensic-verification.md).

Read them at the tag of the release being installed, `v<version>`: the bundle's `README.md` gives the address of `install.md` at that tag. A command on the server runs as root, in the directory that holds the bundle's `docker-compose.yml` and `.env`, unless its document says otherwise. `setup.sh` sets `.env` to mode `0600`, and the operator keeps it owned by root, so that root alone reads it (SPEC-023 §Security considerations).

| Document | What it covers |
|---|---|
| [install.md](install.md) | The server from the published bundle, its first admin, the forensic key's fingerprint recorded outside CyberGuard, and the first agent on a Windows endpoint. |
| [configuration.md](configuration.md) | The bundle variables of `.env` with their defaults, the service variables the compose sets from them, the start-up checks that refuse a malformed value, and the project name. |
| [backup-and-restore.md](backup-and-restore.md) | `backup.sh`, which writes two encrypted files into `CG_BACKUP_DIR`, and `restore.sh`, which restores them into a stack whose stores are empty, with the backed-up installation's `.env`. |
| [upgrade.md](upgrade.md) | Moving an installation to a newer bundle: `setup.sh` adds the variables the newer bundle brings, and the operator stops ingest and the api before the one-shots run. |
| [data-protection.md](data-protection.md) | The personal data a deployment holds, where it keeps it and for how long, for the data protection officer's record of processing, and how long to keep the backups. |
| [forensic-verification.md](forensic-verification.md) | The forensic key's fingerprint and how it is recorded at installation, and checking a forensic export against it with the api image's `verify-export.js`. |
