# Handoff — Session 40

Session 40 opened on 2026-10-15. Manuel set its order: a live debt register,
debt #88 first, the sweep in handoff-39's order, then roadmap §F's SPEC when
the sweep allowed. He delegated again every decision that waits on him, an
ADR text and the elevated gate excepted.

This record was started early because the SPEC amendments for #83 and #79
cite the debts it records (#102 to #105) and Manuel's words of this session.
Later commits of the session complete it.

## Start

- `main` = `origin/main` = `9ad6ca6`, tree clean, no stash; no branch, no
  open PR; CI of the tip `markdown-lint → success`, the only workflow
  triggered.
- Catalogs ADR 21 / SPEC 21; Known CI debt zero; handoff-39 states 67 open
  debts.
- `logman query -ets` listed no `CGAgent` session. Docker Desktop was stopped;
  Claude Code started it, an operation CLAUDE.md allows (Manuel had asked to
  be asked before a restart, and told when it is stopped). `C:` had 22 GB
  free.

Manuel's decisions this session, in chat:

- 2026-10-15, opening the session, the order: a register of the open debts
  in `docs/debts.md`, kept by every session's close (CLAUDE.md *At the
  close*); #88 first, "Primero una enmienda de SPEC-004 (reemitirlo al
  arrancar antes de que caduque), después el código en una rama con test en
  rojo antes del arreglo, y después el gate elevado, que corro yo"; then the
  sweep in handoff-39's order; then roadmap §F's SPEC.
- 2026-10-15, the same message, the delegation: "Lo que dependa de mí,
  decídelo tú en beneficio del proyecto, como en las sesiones 38 y 39: para
  un comprador empresarial, registrando cada decisión como delegada. Un ADR
  sigue necesitando mi "sí" explícito, y el gate elevado lo sigo corriendo
  yo." It is a second statement, made at the opening of Session 40; the
  2026-10-15 words SPEC-008, SPEC-009 and the threat model quote from
  Session 39 ("todo lo que dependa de mi elige tu en beneficio del
  proyecto") are the first.

## Progress

### The debt register

`1b68435` (docs, straight to `main`): `docs/debts.md`, with the 66 open debts
and the 35 closed, each row pointing to the handoff entry that records it;
CLAUDE.md's *At the start* step 5 and *At the close* step 2 keep it. The
register's review found #70 closed by `25add5b` (ADR-0015 Amendment
2026-10-14) before handoff-38 recorded it as new, so the open count at the
start was 66, not handoff-39's 67.

```text
SHA 1b68435: markdown-lint -> success
```

### #88: the server certificate's renewal

`db35f8e` (docs, straight to `main`): SPEC-004 Amendment 2026-10-15 and its
catalog line, after four passes of the fixed SPEC prompt.

```text
SHA db35f8e: markdown-lint -> success
```

Branch `fix/s40-debt-88-server-cert` (PR #18), one commit, rebuilt three
times to fold the code reviews' fixes, pushed with `--force-with-lease`, CI
on each:

```text
ba08ff1: ts-ci -> failure (a test's race, below)
4d833cf: ts-ci -> success
41a8b2b: ts-ci -> success
ebb0394: ts-ci -> success
```

`ebb0394` waits for the elevated gate (`C:\tmp\s40-gate1.ps1`), which
Manuel runs.

### #83, #97 and #79: the contracts

This record's first form lands with SPEC-008's and SPEC-009's second
amendments of 2026-10-15, after four passes of the fixed SPEC prompt on
their drafts.

## Debts

New this session:

- **#102 — The api's login deletes no prior session.** SPEC-008 §Operational
  §2 rotates the session "on every login": a fresh token, and the prior one
  deleted. `login` in `services/api/src/auth/service.ts` creates a session
  and never deletes one the caller held, and `POST /v1/auth/login` in
  `services/api/src/auth/routes.ts` reads no cookie. The dashboard's login
  compensates by revoking the browser's session first (SPEC-009 Amendment
  2026-10-15, *the dashboard's login and logout*, item 3); a direct api client
  keeps both sessions. Surfaced by the review of that amendment.
  Destination: a later SPEC-008 change, the login deleting the session its
  cookie names.
- **#103 — Without a proxy in front of the dashboard, a client chooses the
  address the api counts.** Next keeps a request's `X-Forwarded-For` and fills
  it only when absent. So when the dashboard is listed in
  `API_TRUSTED_PROXIES` and no proxy that appends or sets the header stands
  in front of it, as on a developer's host, any client that reaches the
  dashboard chooses the address the api counts and records: it can evade the
  per-IP login limit, or spend a chosen address's attempts (SPEC-008
  Amendment 2026-10-15, *the source address behind a proxy*, item 4; SPEC-009,
  *the dashboard's login and logout*, item 5). The per-account limit is
  unaffected. Destination: roadmap §F, which serves the dashboard only
  behind a TLS proxy that appends or sets the header.
- **#104 — The dashboard sends no security headers.** No Content Security
  Policy and no HSTS (threat model §9). Destination: roadmap §F.
- **#105 — A certificate and key path that name one file lose the key at a
  reissue.** If `INGEST_SERVER_CERT_PATH` and `INGEST_SERVER_KEY_PATH` name
  the same file and it holds a self-issued pair, `writePair`
  (`services/ingest/src/cert.ts`, on the #88 branch) writes the key's
  temporary file, then the certificate's over it; the first rename puts the
  new certificate alone over the file, and the second fails. The next start
  is fatal, and logged. Before #88 such a file served until it expired. It
  needs someone to have merged the server's own pair into one file. Surfaced
  by the fourth code review of #88. Destination: the next change to
  `services/ingest/src/cert.ts`: refuse the reissue when the two paths
  resolve to one file.

Changed this session:

- **#79**: its destination is SPEC-008's second Amendment 2026-10-15,
  `API_TRUSTED_PROXIES`; it closes with that amendment's code, and roadmap
  §F's SPEC lists the addresses for its compose.
- **#83**: SPEC-009's second Amendment 2026-10-15 settles the dashboard's
  login and logout; its code narrows #83 to the TOTP enrollment and the user
  management in the dashboard, with #98's invitation. Still required by
  roadmap §F.
- **#97**: the same amendment's item 6; its code narrows #97 to the transport
  between the dashboard and the api, roadmap §F's.
