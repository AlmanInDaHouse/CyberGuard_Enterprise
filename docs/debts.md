# Debt register

The live list of the project's debts. Each row points to the entry that
records the debt in full, in a session handoff, and names where the debt is
to be paid. The handoffs are records (Class H) and are not edited: when a
debt changes, its row here changes and points to the handoff that records
the change.

Paths in the *Entry* column are relative to `docs/`; paths elsewhere are
from the repository root. In *Open debts*, a second path in *Entry* is a
later handoff that restated, widened or re-routed the debt; in *Closed*,
*Entry* is the record of the closure.

## How the register is kept

At its close, each session (CLAUDE.md *Session protocol*, §At the close):

- adds a row for each debt it opens, pointing to its handoff's entry; a
  debt opened and closed in the same session goes straight to *Closed*;
- moves each debt it closes to *Closed*, with the commit that closed it, or
  the reason when no commit did;
- updates the *Destination* of each debt whose destination it changed;
- sets *Open* below to the number its handoff states, and the two agree
  with the rows of *Open debts*.

**Open: 52** at Session 47's close, the count `docs/handoff-session-47.md` states.

"Sweep" in a destination is the debt sweep of the latest handoff's *How
Session N resumes* (at Session 47's close, `docs/handoff-session-47.md:601`);
"parked" is roadmap §B2, which waits for a SPEC-021 text.

## Open debts

| # | Debt | Entry | Destination |
|---|---|---|---|
| 4 | `detect_ac_001` "0 events" trap: a stale binary and a dirty cursor look the same | `handoff-session-26.md:158` | Assert a captured event count above zero. |
| 5 | Go named where the server is TypeScript | `handoff-session-26.md:161` | Roadmap Part (b) §1. `services/README.md` follows ADR-0002 Amendment 2026-10-14; `docs/product/blueprint.md:393` still names Go. |
| 7 | JCS canonicalization duplicated (`canonical.ts`, `jcs.ts`) | `handoff-session-26.md:167` | A shared package, which may be a new component: Manuel's, ask first (`docs/handoff-session-38.md:899`). |
| 21 | `NotImplementedError` is dead code; the `detect_ac_*` headers still describe the red phase | `handoff-session-30.md:118`; `handoff-session-31.md:143` | A future sweep, not ordered. |
| 24 | No per-org configurable detection window | `handoff-session-31.md:155`; `handoff-session-44.md:337` | The SPEC that adds a second org: out of roadmap §F (SPEC-023 §Scope, a delegated decision). |
| 25 | The detection cycle opens a client per call | `handoff-session-31.md:158`; `handoff-session-34.md:295` | None recorded. |
| 26 | An incident's MITRE techniques from its first alert; a severity raise sends no email | `handoff-session-31.md:163`; `handoff-session-34.md:303` | Recorded: roadmap §E, which is DONE (`docs/product/roadmap.md` §E) without it. None since. |
| 29 | The Terminate carries no path | `handoff-session-32.md:190` | The next agent phase that touches the created-time cache. |
| 31 | An event's `time` not bounded at ingest; widened to 1007's `event_id` and pids | `handoff-session-33.md:157`; `handoff-session-35.md:565`; `handoff-session-41.md:765`; `handoff-session-42.md:339` | Its shape half done (`90ee7e7`). No bound behind arrival in the MVP (Manuel, SPEC-017 Amendment 2026-10-19, the first); a bound ahead of arrival once a measurement shows what an honest agent's times do across a correction of its host's clock (a host change, which waits for his OK). |
| 32 | A 500 that is not connectivity is resent without bound | `handoff-session-33.md:160`; `handoff-session-41.md:761` | None recorded; since `90ee7e7` a malformed time, id, pid or exit code no longer makes one, but times over more than 100 days or a lone surrogate still do. |
| 35 | The driver's drain ceiling | `handoff-session-33.md:172` | None recorded. |
| 37 | `detect_ac_001` runs a single cycle | `handoff-session-33.md:178` | Drain until a cycle returns fewer rows than the limit. |
| 38 | The forensic evidence unit carries no `class_uid` | `handoff-session-34.md:310` | A precondition of any rule that cites an event other than a process event (ADR-0018 §9, SPEC-019 §Out of scope). |
| 39 | The CGES examples' members differ from the wire's (their `time` aligned in S41) | `handoff-session-34.md:317`; `handoff-session-41.md:758` | Sweep. |
| 42 | A POST per batch interval when the server shares the agent's host | `handoff-session-35.md:569` | Connection reuse in the agent's TLS client. |
| 46 | Two rules on the validity of a marquee run clash | `handoff-session-35.md:586` | None recorded. |
| 50 | A dead subscription that answered 4317 would not be found | `handoff-session-35.md:604` | None: measuring it restarts a host service (SPEC-020 §Risks). |
| 51 | The S4U success measured only on a local account in a workgroup | `handoff-session-35.md:607` | Watch on any other developer machine. |
| 52 | The process callback writes a `trace` line per record | `handoff-session-36.md:289` | The next change under `agent/cg-agent/src/etw/` (B2's branch, parked). |
| 53 | The AC-006 line lacks `activity_id` and `event_id` | `handoff-session-36.md:300` | With #52. |
| 54 | Stale "roadmap §D" / "B2" destinations in the engine and `rules/windows/README.md` | `handoff-session-36.md:168` | B2's implementation branch (parked). |
| 56 | `alert.json` says the agent's `event_id`s are UUIDv4 | `handoff-session-36.md:323` | B2's implementation branch (parked). |
| 58 | The schemas require `schema_version`; the wire carries none | `handoff-session-37.md:469` | B2's implementation branch (parked). |
| 59 | No environment runs the agent on Windows 10 or Server 2019 | `handoff-session-37.md:480`; `handoff-session-38.md:525` | Manuel: a machine of each; matters for B2's second session (parked). |
| 61 | The sessions buffer in the mode Microsoft advises against at high rates | `handoff-session-37.md:502` | A load measurement before roadmap §F. |
| 62 | SPEC-005 names the wrong call for exit 9 | `handoff-session-37.md:511` | SPEC-021 (parked). |
| 64 | ADR-0008's helper names differ from the code's | `handoff-session-38.md:1051`; `handoff-session-38.md:1197` | The code takes ADR-0008's names at the next change to `agent/cg-agent/src/etw/`, under the elevated gate. |
| 65 | An orphaned marquee agent can still leave its session | `handoff-session-38.md:1063` | Reopen if a gate leaves a session; a `try`/`finally` in each marquee. |
| 66 | Gate output noise (`logman` text, a stop line with nothing stopped) | `handoff-session-38.md:1070` | The next change to either file. |
| 73 | ADR-0021 §2 says `ajv-cli` reads JSON | `handoff-session-38.md:1127` | An ADR-0021 amendment, for Manuel's "sí". |
| 83 | The dashboard enrolls no TOTP and manages no user | `handoff-session-39.md:642`; `handoff-session-40.md:1136` | The TOTP enrollment and the user management in the dashboard, with #98's invitation. Required by roadmap §F. |
| 87 | The agent's private key readable by the enrolling user's processes | `handoff-session-39.md:676`; `handoff-session-43.md:524`; `handoff-session-44.md:337` | Roadmap §F's agent-installation SPEC (F2), whose service account decides the key's owner; ADR-0022, Accepted on 2026-10-23. #111's agent change under gate 1 went without it. |
| 88 | Agent certificates expire at 90 days, with no renewal and no revocation list | `handoff-session-39.md:683`; `handoff-session-40.md:1127` | The agents' renewal, before roadmap §F's installation is called done. The server's half closed by `b293500`. |
| 89 | Enrollment hardening (clear-text token, unchecked scope, no rate limit) | `handoff-session-39.md:694`; `handoff-session-45.md:356` | F4, roadmap §F's renewal SPEC (SPEC-023 §Out of scope); the TLS half landed in `63ea972`. |
| 95 | Logon telemetry is personal data with no asset entry | `handoff-session-39.md:724`; `handoff-session-45.md:361`; `handoff-session-46.md:450` | F1's data-protection document (`docs/operations/`), and the DPO's record of processing; the retention landed in `047eaf5`, the ClickHouse grants in `b794f52`. |
| 97 | The transport between the dashboard and the api is plain HTTP | `handoff-session-39.md:733`; `handoff-session-40.md:1140` | Roadmap §F. |
| 98 | Admins set users' passwords and receive their TOTP secret | `handoff-session-39.md:737`; `handoff-session-44.md:337` | Roadmap §F's dashboard SPEC (F3), with the user pages: an invitation the user completes (ADR-0022, Accepted 2026-10-23). |
| 99 | One process holds the internet-facing listeners and the CA's key | `handoff-session-39.md:741` | An ADR separating the CA, for Manuel's "sí". |
| 102 | The api's login deletes no prior session | `handoff-session-40.md:1024`; `handoff-session-44.md:337` | Roadmap §F's dashboard SPEC (F3), an api change it makes with its pages: the login deletes the session its cookie names (ADR-0022, Accepted 2026-10-23). |
| 110 | The development dependencies carry advisories: since S43, braces, katex and tailwindcss 3's postcss-selector-parser | `handoff-session-40.md:1086`; `handoff-session-42.md:610`; `handoff-session-43.md:517` | tailwindcss's next major (postcss-selector-parser; braces through its chokidar and fast-glob); braces and katex in markdownlint-cli2 when a fixed release falls within their parents' ranges. CI audits every dependency for a critical advisory since `0e26dd7`. |
| 113 | The workflows pin their actions by major tag, not by commit | `handoff-session-41.md:750` | A later sweep: each action pinned by commit SHA. |
| 114 | A heartbeat whose own `body.sent_at` cannot be converted, or whose `sequence_number` or `uptime_seconds` is 1e21 or more, is answered `503` and resent without bound | `handoff-session-41.md:728` | With #32. |
| 119 | The enrollment contract's texts differ from the code where SPEC-023 does not touch them (ADR-0004, SPEC-002, SPEC-003, SPEC-004) | `handoff-session-43.md:541` | F4, the renewal SPEC of roadmap §F, which rewrites the enrollment contract. |
| 124 | A compromised ingest can still rewrite the evidence the forensic key signs (its ClickHouse inserts, the newest row kept; its alerts) | `handoff-session-46.md:457` | Manuel's (threat model R7). |
| 125 | A session that raises `log_parameter_max_length_on_error` for itself logs a passphrase parameter | `handoff-session-46.md:462` | Manuel's (threat model R10). |
| 127 | #122's remainder: a one-shot's statement queues, up to the 5-min bound, for a running service's table lock, whose writes wait behind it; an owner role can take the other service's key, and the other one-shot fails after the bound | `handoff-session-46.md:467`; `handoff-session-47.md:529` | F1's upgrade document: ingest and the api stopped before the one-shots run. |
| 130 | Third-party notices beyond the Node packages: Caddy's Go modules' texts, the agent's crates, the base images' GPL sources, Next's vendored packages | `handoff-session-47.md:542` | Before the first release (F4). |
| 131 | The agent reports Cargo's version, not the release's | `handoff-session-47.md:553` | F2's SPEC, the installer. |
| 132 | A locally built image's `documentation` label points to `tree/unknown` | `handoff-session-47.md:557` | A sweep. |
| 133 | The dashboard image ships `typescript`, carried by Next's output tracing | `handoff-session-47.md:560` | A sweep (`outputFileTracingExcludes`). |
| 134 | The proxy refuses a handshake for a name its certificate does not cover, or for none, which no SPEC states | `handoff-session-47.md:563` | The next SPEC-023 amendment, with `docs/operations/`. |
| 135 | Nothing technical keeps a `v*.*.*` tag from publishing before F2 to F4 and Manuel's OK | `handoff-session-47.md:568` | Manuel's: a protected `release` environment or a tag ruleset. |

## Closed

| # | Debt | Closed by | Entry |
|---|---|---|---|
| 1 | Node 20 in CI | `67c4a7c`, `64ca6f7` (S25) | `handoff-session-26.md:148` |
| 2 | markdownlint version skew | No skew found (S36) | `handoff-session-36.md:163` |
| 3 | Hand-numbered agent ids across the ingest tests | `ecd0fd4` | `handoff-session-39.md:585` |
| 6 | The api harness's copy of the `cges_events` DDL | `b10bc3d` | `handoff-session-39.md:584` |
| 8 | The throwaway-database backfill pattern not extracted | `a9e4ef0` | `handoff-session-26.md:169` |
| 9 | `buildReport` resolved the incident twice | `3f15192` | `handoff-session-41.md:781` |
| 10 | The drill answered 404 when ClickHouse was down | `ff20923`, `4a6e981` | `handoff-session-38.md:1183` |
| 11 | ADR amendments summarized in the catalog row or in §Dependencies, inconsistently | `194c628` | `handoff-session-26.md:177` |
| 12 | Compose basename collision (root stub and `infra/dev/`) | `275288d` | `handoff-session-47.md:518` |
| 13 | Stale `path:line` citations after code moved | `5ddec66`, after the elevated gate (S42) | `handoff-session-27.md:145`; `handoff-session-28.md:153`; `handoff-session-31.md:137`; `handoff-session-41.md:772` |
| 14 | MVP-criteria altitude grouping | No live instance left (S36) | `handoff-session-36.md:166` |
| 15 | The api image not buildable | `569f92d` | `handoff-session-29.md:116` |
| 16 | `ts-ci` built neither ingest nor api | `faac8b3` | `handoff-session-29.md:118` |
| 17 | `INGEST_DETECT_*` absent from SPEC-004 §Configuration | `0cfa4df` (SPEC-023's configuration surface, checked at start) | `handoff-session-45.md:343` |
| 18 | Dev ports on all host interfaces | `6841aa5` | `handoff-session-38.md:1178` |
| 19 | Fail-loud left the listeners open | `67f2055`, `eddadd7` | `handoff-session-38.md:1173` |
| 20 | No SPEC consolidates the api's configuration surface; no `/health` | `0cfa4df` (SPEC-023's configuration surface; the api's `/health`) | `handoff-session-45.md:343` |
| 22 | No device-path to Win32 translation | `db41d33` | `handoff-session-32.md:182` |
| 23 | Capture only in test mode | `8c7ee35` | `handoff-session-32.md:183` |
| 27 | `ac-001-marquee` flake | `d6790b1` | `handoff-session-31.md:167`; `handoff-session-32.md:185` |
| 28 | `.github/workflows/README.md` stale | `f37b243` | `handoff-session-36.md:155` |
| 30 | Real-ETW tests not tried on the hosted Windows runner | `acb0654`, after SPEC-017's second Amendment 2026-10-19 (`0bfc238`) | `handoff-session-32.md:196` |
| 33 | The driver's default interval | `ff20923`, `8ea922e` | `handoff-session-38.md:1187` |
| 34 | An incident's `cg_mitre` from its first alert | Retired, folded into #26 | `handoff-session-34.md:309` |
| 36 | The ClickHouse image tag floated | `d94ab7b` | `handoff-session-38.md:1176` |
| 40 | `event.json`'s `time` had no minimum length | `00ca365` | `handoff-session-41.md:783` |
| 41 | `read-model.test.ts` flake | `f8a2d4a` | `handoff-session-38.md:1191` |
| 43 | Launch and connect out of order | Premise does not hold (S37) | `handoff-session-37.md:544` |
| 44 | SPEC-017 §Operational §6's locks | `cf54945` | `handoff-session-36.md:155` |
| 45 | The local gate missed CI's build steps | `a293c27` | `handoff-session-36.md:155` |
| 47 | No retention for the classes other than 3002 | `047eaf5` | `handoff-session-45.md:349` |
| 48 | ADR-0012:240 said 4001 and 3002 are schema-only | `be69547` | `handoff-session-36.md:155` |
| 49 | ADR-0019's `windows-sys` feature wording | No defect (S36) | `handoff-session-36.md:157` |
| 55 | The threat model described the Phase-0 plan | Manuel's audit, "Auditado, conforme" (2026-10-26), no commit | `handoff-session-47.md:519` |
| 57 | CLAUDE.md on a SPEC header's `Last updated` | `ac80734` | `handoff-session-38.md:1181` |
| 60 | An unelevated agent left its ETW session | `abde0a5` | `handoff-session-37.md:536` |
| 63 | The marquees left the last session running | `a9452b0..928cfbf` | `handoff-session-38.md:1168` |
| 67 | The dashboard's default API address | `bbdf1e3` | `handoff-session-38.md:1189` |
| 68 | No workflow checked the dev compose | `524cb5a` | `handoff-session-39.md:583` |
| 69 | An unreachable Redis held an api request | `aa70bb4` | `handoff-session-39.md:590` |
| 70 | ADR-0015 said a ClickHouse outage fails only the drill | `25add5b`, ADR-0015 Amendment 2026-10-14 (S38; recorded open until S40) | `handoff-session-38.md:282` |
| 71 | The api's Postgres client had no query timeout | `aa70bb4` | `handoff-session-39.md:590` |
| 72 | An unknown role answered 500 | `0c3ca78` | `handoff-session-39.md:591` |
| 74 | SMTP credentials in clear text without TLS | `0e398e6` | `handoff-session-39.md:588` |
| 75 | nodemailer's private fields on an unpinned release | `b560665` | `handoff-session-39.md:587` |
| 76 | Two untested statements of SPEC-022 | `a8deda8` | `handoff-session-39.md:586` |
| 77 | The bootstrap CLI takes the password on its command line | `7ad2396` | `handoff-session-45.md:346` |
| 78 | No operator setting trusts a relay's private CA | `f397129` | `handoff-session-45.md:352` |
| 79 | Behind a reverse proxy, the api reads the proxy's address | `c9dad1c` | `handoff-session-40.md:1108` |
| 80 | A request refused for want of a session wrote no audit row | Its `403` half `8bd5bc9` (S40); its `401` half by SPEC-008 Amendment 2026-10-19, Manuel's decision: not an authentication event, no row (S42) | `handoff-session-42.md:335` |
| 81 | The TOTP confirmation defeated the second factor | `ee5235a` | `handoff-session-39.md:593` |
| 82 | A session lasted 8 h from its login, not 8 h idle within 24 h | `33a56ba` | `handoff-session-41.md:787` |
| 84 | Six places cited ADR-0002 Rule 4 for Zod schemas | Corrected in place (S41) | `handoff-session-41.md:785` |
| 90 | The forensic export and report: any role, no audit row | `8d3c61d` | `handoff-session-40.md:1112` |
| 91 | Detection rules re-read from disk on every cycle | `3da53e3` | `handoff-session-40.md:1122` |
| 93 | No dependency audit in CI | `f876753` to `6d1f393` (PR #23), and the runbook (S41) | `handoff-session-41.md:789` |
| 96 | The dev stack ran NATS without authentication and MinIO with dev root credentials | `cd9b3c8`, after ADR-0003 Amendment 2026-10-16 (`b6bce33`) | `handoff-session-42.md:60` |
| 100 | The password change counts no failure and leaves its TOTP code usable | `8bd5bc9` | `handoff-session-40.md:1116` |
| 101 | The login limiter reads its counters, then counts the failure | `8bd5bc9` | `handoff-session-40.md:1119` |
| 105 | A certificate and key path that name one file lose the key at a reissue | `0cfa4df`, `09b2f87` | `handoff-session-45.md:345` |
| 106 | The dashboard's cookies carry no `__Host-` prefix | `84a4aad` | `handoff-session-45.md:351` |
| 107 | The dashboard's logout answered the api's `403` with `403` after clearing the cookies | `fe59102`, `1697e60` (opened and closed in S40) | `handoff-session-40.md:1097` |
| 111 | `cargo audit` let rustls-pemfile pass, unmaintained, in the agent's runtime | `12f8db4` (PR #37, after gate 1 and `s44-gate1`) | `handoff-session-44.md:331` |
| 112 | No SBOM is produced, and the licence check does not run in the release | `e58ca2b`; the check `867ac68` | `handoff-session-47.md:521` |
| 115 | ADR-0010 read the runner's ETW-open privilege as unvalidated in CI | `0f2d6e9` (ADR-0010's and ADR-0008's Amendments 2026-10-19, ratified) | `handoff-session-44.md:333` |
| 116 | apiauth_ac_014's Postgres case fails when the Docker VM's clock falls behind the host's | `57845d4`, opened and closed in S42 | `handoff-session-42.md:577` |
| 117 | Sentences in ADRs and SPECs the code has outgrown, which #13's sweep left | No commit: classified, each a record (Class H-inline) or superseded by an amendment (S43) | `handoff-session-43.md:272` |
| 118 | The SPEC-005 marquee's budget log differs from SPEC-005 (`within_budget`, a clock started before enrollment) | `63ea972` | `handoff-session-45.md:350` |
| 120 | RUSTSEC-2026-0382 (rustls-webpki) and RUSTSEC-2026-0381 (webpki-roots) in the agent's TLS stack | `621bf4f`, opened and closed in S44 | `handoff-session-44.md:334` |
| 121 | The dev compose's key-value store runs as root in its container (`sh -c` skips the entrypoint's drop to `valkey`) | `5d6a564` | `handoff-session-45.md:348` |
| 85 | A compromised ingest can forge an api session (shared Redis) | `b794f52` | `handoff-session-46.md:438` |
| 86 | Both services act as the Postgres superuser | `b794f52` | `handoff-session-46.md:439` |
| 92 | The containers run as root; a TLS key file ingest wrote before `b293500` keeps its mode | `7385f32` | `handoff-session-46.md:436` |
| 94 | Encryption passphrases travel as SQL parameters | `b794f52` | `handoff-session-46.md:440` |
| 103 | Without a proxy in front of the dashboard, a client chooses the address the api counts | `b794f52` | `handoff-session-46.md:442` |
| 104 | The dashboard sends no security headers (CSP, HSTS) | `b794f52`, `7385f32` | `handoff-session-46.md:444` |
| 108 | Ingest and the api share one ClickHouse credential, which can write the events the forensic key signs | `b794f52` | `handoff-session-46.md:445` |
| 109 | Whoever can write the rules directory and restart ingest changes detection | `7385f32` | `handoff-session-46.md:437` |
| 122 | The migrate one-shot waits for its advisory lock without bound, and any role can take another service's key | `2217725` | `handoff-session-46.md:434` |
| 123 | SPEC-023 §Operational §6 cites `agent/cg-agent/src/tls.rs:83`, now another line | `9c06fd2`, restored by `03b7976` | `handoff-session-46.md:435` |
| 126 | GHSA-mrhx-c7xc-m3w5 in zod below 4.6.6 (memory exhaustion validating oversized collections) | `aa56b49`, opened and closed in S46 | `handoff-session-46.md:465` |
| 128 | The proxy image keeps Caddy's title, description, URL and vendor labels beside CyberGuard's four | `2f63baa`, `696329a` | `handoff-session-47.md:523` |
| 129 | The dashboard image ships sharp's libvips (LGPL-3.0-or-later) and `caniuse-lite` (CC-BY-4.0) without their notices | `6fb5568`, `0386700`, `b70ed79` | `handoff-session-47.md:524` |
