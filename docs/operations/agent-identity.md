# Agent identities

This document is for the operator of the server. It covers what the server holds of each enrolled agent, how ingest's log shows that it refuses one, an agent's revocation and what undoing one means, an agent's identity copied to another endpoint, a restore's effect on the agents, and the compromise of ingest's CA (SPEC-025). A command runs as root in the bundle's directory ([README.md](README.md)).

## What the server holds of an agent

An enrollment ([install.md](install.md#enroll-the-first-agent)) gives the endpoint a new agent: an id the server assigns, a key the endpoint generates and keeps, and a certificate that ingest's CA signs for that key, which lasts 90 days, or less when the CA ends sooner. Ingest keeps the agent's row in Postgres:

- its id, its org, its public key, its certificate and the certificate's end;
- the token that enrolled it, by the identifier `issue-token` printed ([install.md](install.md#on-the-server-the-trust-anchor-and-a-token));
- the host name, platform and agent version the endpoint last sent, which the endpoint chooses ([data-protection.md](data-protection.md#postgres));
- a pending certificate: one ingest issued at a renewal and the agent has not yet used;
- its revocation, with its time and reason, once it is revoked;
- its copy flag ([A copied identity](#a-copied-identity)), and the start of its process as its last heartbeats gave it.

Ingest accepts an agent's request only under the agent's certificate or its pending one, each compared with the row byte for byte. A certificate the CA signed that the row does not hold is refused, an older one of the agent's included. The first accepted request under the pending certificate makes it the agent's, and ingest logs `agent certificate promoted` with both certificates' serial numbers. A renewal is a request of the agent's, under its key and the new one, in the last 45 days of its certificate. This release's agent does not yet ask for one: its certificate lasts the 90 days of its enrollment ([install.md](install.md#check-that-the-agent-reports)).

## Reading a refusal

Ingest logs each heartbeat it refuses as `heartbeat rejected`, and each renewal it refuses as `renewal rejected`, with the `reason`, the `response_status` and, once the request is read, the `agent_id` it named. Three reasons concern the agent itself:

| `reason` | Status | What it means |
|---|---|---|
| `unknown_agent` | 401 | No agent of that id, or a client certificate that names another. An endpoint enrolled after the backup a restored server came from gives it ([A restore of the server](#a-restore-of-the-server)). |
| `agent_revoked` | 403 | The agent is revoked ([Revoking an agent](#revoking-an-agent)). |
| `agent_superseded` | 403 | The request presents a certificate of the agent's that is neither its certificate nor its pending one: a copy of the endpoint ([A copied identity](#a-copied-identity)), an endpoint or a server restored from a backup taken before a renewal, or a certificate signed with the CA's key outside a renewal ([The compromise of ingest's CA](#the-compromise-of-ingests-ca)). |

The other reasons are a malformed request, the envelope's own checks of its time, its nonce and its signature, a pending certificate's promotion the database did not record, and a renewal's own refusals: a renewal not yet due, a second one within 10 minutes, and a new key's signature that does not verify. Ingest logs a refused enrollment as `enroll rejected`, with its reason, `rate_limited` among them ([install.md](install.md#on-the-server-the-trust-anchor-and-a-token)), and a store it cannot reach as `backend unavailable`. On the endpoint, the agent logs a refused heartbeat as `signed envelope rejected by server`, with the status, and keeps running; after the retries it drops that batch's events ([install.md](install.md#check-that-the-agent-reports)). No refused heartbeat stops it. At its certificate's end the server refuses its connection, and its service stops with code 7 ([install.md](install.md#the-agents-exit-codes)).

This command prints ingest's lines of these refusals and of refused enrollments, of the copy signals and of the promotions that the container logs still hold:

```sh
docker compose logs --no-color ingest | grep -F -e 'heartbeat rejected' -e 'renewal rejected' -e 'enroll rejected' -e 'agent duplicate suspected' -e 'agent certificate promoted'
```

The container logs keep five files of 10 MB for each container ([data-protection.md](data-protection.md#container-logs)); a longer record is the installation's log shipping ([configuration.md](configuration.md#logs)).

## Revoking an agent

A revocation is kept on the agent's row: its time, which the database sets, and a reason of 1 to 500 characters. Ingest refuses every heartbeat and renewal of a revoked agent with `403 agent_revoked`, and stores none of its events. The agent's row and the events it delivered before stay on the server. On the endpoint the agent keeps running, logs each refusal and drops its events after the retries. It cannot renew, so it stops at its certificate's end, with code 7. A revocation refuses every endpoint that holds the agent's key, the original included.

This release has no command or route that revokes an agent. Neither running service writes a revocation. The api's route for it, with its audit record, comes with SPEC-026 (roadmap F3), and this section then gives its steps; this document gives no step in the database. Until then, an agent whose key is out of the operator's control stays accepted, and whoever holds that key and the agent's certificate can renew it. Publication waits for roadmap F2 to F4, F3 among them ([README.md](README.md)).

## Undoing a revocation

Nothing in the product undoes a revocation. Once a revocation is set, the database refuses any change to it, and neither running service has the right to write it. The database's owner role for ingest, `cg_ingest_owner`, holds the trigger that refuses the change, and could disable it; this document gives no such step. An agent revoked by mistake stays revoked. Its endpoint is purged and enrolled anew with a new token, as [install.md](install.md#the-setups-result) says after its table, which gives it a new agent. The revoked agent's row and its events stay on the server.

## A copied identity

A copy of an enrolled endpoint's disk carries the agent's key ([install.md](install.md#the-windows-endpoint)), and the server cannot tell the copy from the original. Ingest flags the agent when it sees either of two signs, and logs `agent duplicate suspected` with the agent's id and the `signal`:

- `superseded_certificate`: a request presents a certificate of the agent's that is neither its certificate nor its pending one ([Reading a refusal](#reading-a-refusal)): once either holder has renewed, the other's requests present one;
- `interleaved_process`: two processes take turns as the agent, by the start that each heartbeat gives, its time less its process's uptime.

The flag is set once, with its first signal and its time. A later signal changes neither and logs nothing, and neither running service clears it. No alert is written, no request is refused for the flag, and nothing revokes the agent. A flag is a sign, not proof: a virtual machine reverted to a snapshot, a step of the endpoint's clock and its correction of more than 2 minutes each, and an endpoint restored from its own backup or snapshot taken before a renewal raise one too, as a server restored from a backup taken before a renewal does ([A restore of the server](#a-restore-of-the-server)). This release shows the flag in the log line alone; the dashboard's agents page that SPEC-026 adds shows it on the agent.

The remedy is the revocation ([Revoking an agent](#revoking-an-agent)), which refuses every endpoint that holds the agent's key. Then each endpoint that held it is purged and enrolled anew with a new token, as [install.md](install.md#the-setups-result) says after its table, and each has an agent of its own. Until the revocation's route lands, purge and enroll anew each endpoint known to hold the key: the old agent stays accepted for any copy not purged. An endpoint is imaged, as a template or for a deployment tool, before its setup runs, never after.

## A restore of the server

A restore ([backup-and-restore.md](backup-and-restore.md#after-a-restore)) brings back the agents' rows as they were at the backup:

- An agent enrolled before the backup, whose certificate has not changed since, is accepted with no new enrollment.
- An agent that renewed its certificate after the backup has, in the restored row, its older certificate and no pending one. Ingest refuses its requests under its newer certificate with `403 agent_superseded`, and flags it as a copy; the agent cannot renew. It stays so until it is revoked ([Revoking an agent](#revoking-an-agent)) and its endpoint purged and enrolled anew. Until it is revoked, a copy of the endpoint that holds the older certificate is accepted. This release's agent does not yet renew.
- An agent enrolled after the backup has no row: ingest refuses it as `unknown_agent`, and its endpoint is enrolled anew.
- A revocation made after the backup is not in the restored database, and is to be made again. Neither is a copy flag raised after it, nor a use or a withdrawal of a token.

## The compromise of ingest's CA

Ingest's CA signs the agents' certificates and, unless the operator provides ingest's certificate and key, ingest's own server certificate. Its certificate is the agents' trust anchor, `server-ca.pem` ([install.md](install.md#on-the-server-the-trust-anchor-and-a-token)), except with an operator-provided pair, where the anchor is the CA that issued that pair ([configuration.md](configuration.md#placing-or-replacing-the-operators-pair)). The CA's private key is in Postgres, encrypted under `CG_INGEST_CA_PASSPHRASE` of `.env`, and in ingest's memory. Whoever holds a backup, or the Postgres database, together with `.env`, and whoever is root on the host or can use its Docker daemon, can hold the key.

A holder of the key can:

- sign a server certificate, for a name of `CG_AGENT_SERVER_NAMES`, that the agents anchored on `server-ca.pem` trust. From a position on their network path, it receives their enrollments' tokens, their heartbeats and their events, and answers them as the server. With an operator-provided pair, the agents trust no certificate ingest's CA signs as the server's.
- sign agent certificates, which ingest refuses: a certificate that the agent's row does not hold is `agent_superseded`, and flags the agent. The key alone takes over no enrolled agent.

The product has no rotation of its CA: ingest generates it at its first start, a restore brings back the backed-up one, and no command replaces it. A compromise of the CA's key is a compromise of the server, and its remedy is a new installation, with every endpoint enrolled anew:

1. Keep the compromised installation for the investigation. Stop it in its directory with `docker compose stop`, which removes no volume, and keep its volumes, its backups and its `.env`, apart from the new installation. Its endpoints keep running: the events they capture wait in their buffers, and the oldest are dropped once the buffers are full ([backup-and-restore.md](backup-and-restore.md#after-a-restore)).
2. Install the server anew from the release's bundle, as [install.md](install.md) does, on a host the compromise did not reach, with a `.env` that `setup.sh` fills anew, so that every secret is new and ingest generates a new CA. Do not copy the compromised installation's `.env` into it, nor restore its backups: the dump holds the CA's key, and that `.env` the passphrase that opens it.
3. Record the new installation's fingerprints, the CA's and the forensic key's, as [install.md](install.md#on-the-server-the-trust-anchor-and-a-token) and [its forensic key's section](install.md#record-the-forensic-keys-fingerprint) record them, and create its users again.
4. Enroll every endpoint anew: purge it ([install.md](install.md#the-purge)), then run the setup with the new installation's `server-ca.pem`, its fingerprints and a new token, as [install.md](install.md#the-setups-result) says after its table. An endpoint not yet purged that reaches the new installation stops: its agent refuses the new server's certificate, code 6, or the new server refuses the agent's, code 7 ([install.md](install.md#the-agents-exit-codes)).

The compromised installation's events, alerts, incidents and audit log stay in it and in its backups; nothing in the product moves them to the new one. A forensic export its key signed stays verifiable under the fingerprint recorded for it ([forensic-verification.md](forensic-verification.md)).
