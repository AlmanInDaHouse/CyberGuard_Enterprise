# Forensic verification

This document is for the auditor who checks a forensic export of CyberGuard Enterprise, and for the operator who records, at installation, what the auditor checks it against. An export is the evidence of one incident, signed by the deployment's forensic key. The verifier, `verify-export.js`, checks an export against the key's fingerprint recorded outside CyberGuard at installation (ADR-0016 Amendment 2026-10-19; SPEC-023 §Operational §11).

A command on the server runs as root in the bundle's directory, as in [install.md](install.md). The verifier runs on the auditor's own host, which need not be the server (*Running the verifier*).

## The forensic key and its fingerprint

- The api generates the forensic key, an Ed25519 key pair, at its first start. It keeps it in its Postgres table `forensic_key`, the private half encrypted under `CG_API_FORENSIC_PASSPHRASE`, a secret of `.env`. Each export is signed with it.
- The key's fingerprint is `sha256:` followed by the SHA-256 of the public key's 32 bytes in lowercase hexadecimal, 64 digits.
- The fingerprint recorded at installation is the anchor. The key an export carries is checked against it and is never trusted for itself: a server that signed with a key of its own would carry that key in its exports, and their signatures would verify under it.
- No route serves the key on its own, and none serves it without a session: it reaches a reader inside the exports and the reports an admin or an analyst requests, beside its fingerprint, and through `forensic-key.js` on the server (ADR-0016 Amendment 2026-10-19, item 3).
- The forensic fingerprint is not the CA's. `export-ca.js` prints the fingerprint of ingest's CA certificate, in the same `sha256:<hex>` form, over the certificate's DER. That CA is the agents' trust anchor unless the operator provides ingest's server pair ([install.md](install.md)).

## Recording the fingerprint at installation

Once the first `docker compose up -d --wait` has returned, the api has started and generated the key: it generates it before it answers its health check. Print the key and its fingerprint, as the installation's step *Record the forensic key's fingerprint* does ([install.md](install.md#record-the-forensic-keys-fingerprint)):

```sh
docker compose exec -T api node dist/cli/forensic-key.js
```

It prints two lines, the public key in base64url and its fingerprint, and exits 0:

```text
<the public key in base64url, 43 characters>
sha256:<64 lowercase hexadecimal digits>
```

It reads the key from the api's database, never from an export, and needs only `API_PG_URL`, which the compose sets. When the api has not generated the key yet, it prints `cg-api forensic-key: no forensic key yet: the api generates it at its first start` and exits 1. Any other failure prints one line, `cg-api forensic-key: fatal: <reason>`, and exits 1.

Record the second line, all of it, where CyberGuard cannot write (ADR-0016 Amendment 2026-10-19, item 1):

1. In the installation's change record, signed by whoever performed the installation.
2. In a copy handed to the function that audits the deployment.

Take the fingerprint from this command alone. The fingerprint an export or a report carries is the one the server computed, and checking an export against it proves nothing about the key. Neither line is secret: the first is the key's public half, and the second is derived from it.

## What the export and the report carry

The export is a JSON document:

| Field | What it holds |
|---|---|
| `incident_id` | The incident exported. |
| `events` | The evidence: the events the incident's alerts name, as ClickHouse holds them when the export is requested, ordered by time and then by `event_id`. |
| `chain_root` | The chain head, in lowercase hexadecimal: a SHA-256 chain over the events in their order, each event in the canonical JSON form of RFC 8785 (SPEC-012 §Data contracts §2). |
| `root_signature` | The forensic key's Ed25519 signature over the chain head's 32 bytes, in base64url. |
| `forensic_pubkey` | The forensic public key, its 32 bytes in base64url. |
| `forensic_pubkey_sha256` | The key's fingerprint, `sha256:<hex>`, as the server computed it. |

Each event holds `event_id`, `agent_id`, `activity_id`, `process_pid`, `process_uid`, `process_name`, `image_file_name`, `process_parent_pid` and `event_time`. The events are process events (class 1007), the one class the api reads (SPEC-023 §Operational §4). An `image_file_name` can hold a user name inside its path, so an export holds personal data ([data-protection.md](data-protection.md)).

The report is a PDF of the incident. Its *Integrity* section prints `chain_root`, `root_signature`, `forensic_pubkey` and `forensic_pubkey_sha256`, and says that a verifier checks the key against the fingerprint recorded at installation. The PDF is not signed, and the verifier does not read it: the verifier checks an export.

`forensic_pubkey_sha256`, in the export and in the report, is a convenience that a reader compares with the record. It vouches for nothing, since the server that signed the export computed it. The verifier does not read it: it computes the fingerprint from `forensic_pubkey`.

## Obtaining an export

- The api serves the export at `GET /v1/incidents/<incident-id>/forensic-export` and the report at `GET /v1/incidents/<incident-id>/report`, to a session of a user whose role is `admin` or `analyst`. A request with no valid session answers `401`, and a `viewer` is refused with `403`. A malformed incident id, an incident that does not exist and one that belongs to another org each answer `404`.
- Before it sends either document, the api writes a row in its `audit_log`, with the action `forensic.export` or `forensic.report`, the incident, the number of events signed and the chain head, and, for a report, the PDF's SHA-256. When the row cannot be written, the api answers `503` and sends no document.
- The api publishes no port, and a browser reaches only what the dashboard serves (ADR-0022 §2). The dashboard has no page for the export or the report yet: the dashboard SPEC of roadmap §F (F3) adds them, and the first release waits for it (SPEC-023 §Scope, §Operational §12). This section names those pages once that SPEC has landed.

### The server's record of an export

The `audit_log` row is the server's own record. No route serves it, and the api's runtime role can append to `audit_log` but cannot read it. The host operator can read export rows using the api's existing schema-owner role. The command reads that role's password from the Postgres container's environment, without putting its value in the command line:

<!-- forensic: audit -->

```sh
docker compose exec -T postgres sh -c 'export PGPASSWORD="$CG_PG_API_OWNER_PASSWORD"; exec psql -h 127.0.0.1 -U cg_api_owner -d cyberguard -v ON_ERROR_STOP=1' <<'SQL'
SELECT occurred_at, user_id, target_id, detail FROM api.audit_log WHERE action = 'forensic.export' ORDER BY id;
SQL
```

`target_id` is the incident, `detail` holds the number of events signed (`events`) and the chain head (`chain_root`), and `occurred_at` is the time the database wrote the row. A report's rows have the action `forensic.report`, and their `detail` also holds the PDF's SHA-256 (`report_sha256`). The row is written by the server whose exports it records, so it vouches for an export no more than the server does (*What it does not prove*).

## Running the verifier

The auditor runs the verifier from the api's published image, on the auditor's own Linux host, as a non-root user allowed to run Docker, in no particular directory. It reads no store and needs no configuration (SPEC-023 §Operational §11); the bundle's test runs it with no network (SPEC-023 deploy_ac_010). The image is built for linux/amd64 (SPEC-023 §Operational §12).

### The image

The image's reference is the `image` of the `api` service in the published bundle's `docker-compose.yml`, the release's asset `cyberguard-deploy-<version>.tar.gz` ([install.md](install.md)):

```text
ghcr.io/almanindahouse/cyberguard-api:<version>@sha256:<digest>
```

`<version>` is the release the installation runs, and `<digest>` the digest of the image that release built, which its workflow attests (SPEC-023 §Operational §12). The digest binds the verifier to the image the installation runs. The tag alone, `ghcr.io/almanindahouse/cyberguard-api:<version>`, the form of SPEC-023 §Operational §11, names whatever image the registry holds under that tag when it is pulled.

Pull the image once; this is the one step that needs a network:

```sh
docker pull ghcr.io/almanindahouse/cyberguard-api:<version>@sha256:<digest>
```

### The export's file

Put the export in a directory owned by the auditor that holds nothing else. Keep it readable only by that account; the command below runs the verifier with the same uid/gid instead of the image's default uid 1000:

```sh
chmod 0700 <dir>
chmod 0600 <dir>/<export>.json
```

The export holds personal data ([data-protection.md](data-protection.md)). Keep it under the auditor's evidence-retention and access policy; verification does not require making it world-readable.

A directory or file this account cannot read makes the verifier answer `refused: cannot read /in/<export>.json` and exit 2.

### The command

Run the verifier with the fingerprint from the record:

```sh
docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges:true --user "$(id -u):$(id -g)" -v <dir>:/in:ro ghcr.io/almanindahouse/cyberguard-api:<version>@sha256:<digest> node dist/cli/verify-export.js --fingerprint sha256:<hex> /in/<export>.json
```

- `<dir>` is the absolute path of the directory that holds the export, and `<export>.json` the export's file name in it.
- `sha256:<hex>` is the fingerprint as recorded at installation.
- `--network none` gives the container no network, and `:ro` mounts the directory read-only. The root filesystem is read-only, all capabilities are dropped, and the verifier uses the non-root auditor's uid/gid. It needs neither a network nor a write. This invocation assumes rootful Docker without user-namespace remapping; a different Docker user-mapping setup needs an equivalent readable private mount.

The verifier also runs anywhere Node 22 runs, with the api's `dist/` and its production dependencies (SPEC-023 §Operational §11). This document gives the image's form.

## Its outputs and exit codes

The verifier checks, in this order, and stops at the first check that fails:

1. Its arguments: `--fingerprint` with a value, and one export file.
2. The fingerprint's form: `sha256:` and 64 hexadecimal digits.
3. The file: it can be read.
4. The export's shape: a JSON object with an array `events` and the strings `chain_root`, `root_signature` and `forensic_pubkey`.
5. The key: `forensic_pubkey` decodes to 32 bytes whose SHA-256 is the recorded fingerprint.
6. The chain: the head it recomputes from `events`, in their order, is `chain_root`.
7. The signature: `root_signature` verifies over that head under the key.

| Exit code | Line | Stream | Meaning |
|---|---|---|---|
| 0 | `verified: <n> events, chain head <hex>` | standard output | Every check passed. `<n>` is the number of events and `<hex>` the chain head. |
| 1 | `the export's key does not match the recorded fingerprint` | standard output | The key the export carries, `forensic_pubkey`, is not the recorded one, or is not 32 bytes. |
| 1 | `the events do not give the export's chain head` | standard output | The events do not give `chain_root`: an event, their order or their number was changed, or `chain_root` was. |
| 1 | `the signature does not verify` | standard output | `root_signature` is not a signature of the recorded key over the chain head, which is the case too when the events and `chain_root` were both changed. |
| 1 | `cg-api verify-export: fatal: <error>` | standard error | An unexpected error. |
| 2 | `refused: <reason>`, then the usage line | standard error | The command was not given what it needs. Nothing was verified. |

The usage line is `usage: verify-export --fingerprint sha256:<hex> <export.json>`. The refusals, in the order of the checks:

- `refused: --fingerprint is required`: `--fingerprint` is missing, or nothing follows it.
- `refused: one export file`: no export file was given, or more than one.
- `refused: the fingerprint is sha256: and 64 hexadecimal digits`: the fingerprint has another form. The prefix `sha256:` is in lower case; the digits may be in either case.
- `refused: cannot read <file>`: the file cannot be read: for instance, it does not exist, it is a directory, or uid 1000 cannot read it.
- `refused: the file is not a forensic export`: the file is not JSON, is not a JSON object, or lacks one of the four fields of check 4 with its type. A byte-order mark before the JSON makes the file not JSON.

## What a verified export proves

- The key that signed the export is the one whose fingerprint was recorded at installation.
- The events of the export, each field of each event, their order and their number, are those that key signed. The chain binds each event's canonical form, in order, into the head, and the signature binds the head to the key (SPEC-012 §Data contracts §2).

Since verification starts from the record, a server compromised after the record cannot substitute a key of its own unseen: an export signed by any other key fails verification (ADR-0016 Amendment 2026-10-19, item 4).

## What it does not prove

- **Which event changed.** The export carries no hash per event, so the verifier cannot name the event that changed (SPEC-023 §Operational §11).
- **What lies outside the events.** The signature covers the chain head alone, which the events give. `incident_id` and `forensic_pubkey_sha256` are outside it, and the verifier reads neither. The `forensic.export` row records the incident and the chain head of each export the api sends, but it is the server's own record (*The server's record of an export*): it does not bind `incident_id` to the events independently of the server.
- **That the evidence was true when the key signed it.** The key signs what the stores hold when the export is requested. Whoever holds a credential that writes the events, the alerts or the incidents can change them before an export, and the key then signs what they wrote (threat model, R7). In the bundle the api's credentials write none of them. Ingest's can insert a row under the id of an event an alert names, which the export then reads, insert alerts, and change which alerts an incident lists, though not change an alert it wrote. The stores' owner and administrator credentials and the Postgres superuser's, whose passwords are in `.env`, can between them change anything the stores hold.
- **That the key was not stolen.** The private key is on the server, in Postgres, encrypted under a passphrase of the same host's `.env`. Whoever takes the database, or one of its backups, and `.env` can sign as the server, and the exports signed after that are not distinguishable from earlier ones. The MVP has no hardware key store and no trusted timestamp (ADR-0016 Amendment 2026-10-19, item 4).
- **When the export was signed.** The export carries no time of signing. The audit row's `occurred_at` is the time the server's database wrote it: the server's word, which no third party vouches for.
- **That the evidence is complete, or the same at each export.** Each export signs the events the stores return at its request. An event that its retention period has deleted is absent (ADR-0022 §Consequences), and the export does not say that an alert names it. An incident that gains an alert gains its events too. Two exports of one incident, made at different times, can then carry different events and different chain heads, and each verifies. An export with no events verifies too, as `verified: 0 events, chain head e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`: the head of an empty chain is the SHA-256 of the empty input.

## After a restore

- `restore.sh`, run with the backed-up installation's `.env`, restores the forensic key with the database. `forensic-key.js` then prints the fingerprint recorded at installation (SPEC-023 deploy_ac_012). Run it after each restore and compare its second line with the record ([backup-and-restore.md](backup-and-restore.md)).
- The key's passphrase is `CG_API_FORENSIC_PASSPHRASE`, in `.env`, which `backup.sh` does not copy: the restore needs the copy of `.env` kept apart from the backups ([install.md](install.md)).
- With a `.env` whose `CG_API_FORENSIC_PASSPHRASE` is not the one the key was stored under, the api cannot decrypt the key and does not generate another: each start writes `cg-api: fatal: <error>` to its log and exits 1, so the api never becomes healthy.
- An export made before a restore verifies after it as before: the verifier reads the export and the record alone.
- The MVP does not rotate the key. When the api starts with no `forensic_key` row, as after a restore that lost it, it generates a new key, whose fingerprint differs from the record. The operator records the new fingerprint as at installation. Exports signed before stay verifiable under the earlier record (ADR-0016 Amendment 2026-10-19, item 5), and those signed after verify only under the new one: keep both records.
