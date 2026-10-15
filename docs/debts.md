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

**Open: 68** at the commit that records #100's and #101's closure in `docs/handoff-session-40.md`. Session 40 opened with 66; it added #102 to #106 and #108, and closed #79, #90, #100 and #101; #107 was opened and closed in the session, and #80 narrowed to its `401` half. `docs/handoff-session-39.md:596` states 67: it counts #70, which ADR-0015's Amendment 2026-10-14 had closed (`25add5b`) before `docs/handoff-session-38.md:1102` recorded it as new. Session 40's review of this register found it.

"Sweep" in a destination is the debt sweep of the latest handoff's *How
Session N resumes* (at `9ad6ca6`, `docs/handoff-session-39.md:771-776`);
"parked" is roadmap §B2, which waits for a SPEC-021 text.

## Open debts

| # | Debt | Entry | Destination |
|---|---|---|---|
| 4 | `detect_ac_001` "0 events" trap: a stale binary and a dirty cursor look the same | `handoff-session-26.md:158` | Assert a captured event count above zero. |
| 5 | Go named where the server is TypeScript | `handoff-session-26.md:161` | Roadmap Part (b) §1. `services/README.md` follows ADR-0002 Amendment 2026-10-14; `docs/product/blueprint.md:393` still names Go. |
| 7 | JCS canonicalization duplicated (`canonical.ts`, `jcs.ts`) | `handoff-session-26.md:167` | A shared package, which may be a new component: Manuel's, ask first (`docs/handoff-session-38.md:899`). |
| 8 | The throwaway-database backfill pattern not extracted | `handoff-session-26.md:169` | A shared test helper; sweep. |
| 9 | `buildReport` resolves the incident twice | `handoff-session-26.md:171` | Dedupe the resolution; sweep. |
| 11 | ADR amendments summarized in the catalog row or in §Dependencies, inconsistently | `handoff-session-26.md:177` | Retrofit of the ADR catalog; sweep. |
| 12 | Compose basename collision (root stub and `infra/dev/`) | `handoff-session-27.md:140` | Roadmap §F; Part (b) §4. |
| 13 | Stale `path:line` citations after code moved | `handoff-session-27.md:145`; `handoff-session-28.md:153`; `handoff-session-31.md:137` | A Class-A pass swept by pattern, with #84's SPEC texts; sweep. |
| 17 | `INGEST_DETECT_*` absent from SPEC-004 §Configuration | `handoff-session-28.md:169` | A SPEC-004 amendment: recorded as owner-STOP under the rules of the time, Claude Code's since 2026-10-10 (CLAUDE.md *Decision authority*). |
| 20 | No SPEC consolidates the api's configuration surface; no `/health` | `handoff-session-29.md:120` | Roadmap §F's operator configuration surface. |
| 21 | `NotImplementedError` is dead code; the `detect_ac_*` headers still describe the red phase | `handoff-session-30.md:118`; `handoff-session-31.md:143` | A future sweep, not ordered. |
| 24 | No per-org configurable detection window | `handoff-session-31.md:155` | Roadmap §F. |
| 25 | The detection cycle opens a client per call | `handoff-session-31.md:158`; `handoff-session-34.md:295` | None recorded. |
| 26 | An incident's MITRE techniques from its first alert; a severity raise sends no email | `handoff-session-31.md:163`; `handoff-session-34.md:303` | Recorded: roadmap §E, which is DONE (`docs/product/roadmap.md` §E) without it. None since. |
| 27 | `ac-001-marquee` flake | `handoff-session-31.md:167`; `handoff-session-32.md:185` | Diagnosis first; sweep. |
| 29 | The Terminate carries no path | `handoff-session-32.md:190` | The next agent phase that touches the created-time cache. |
| 30 | Real-ETW tests not tried on the hosted Windows runner | `handoff-session-32.md:196` | Sweep. |
| 31 | An event's `time` not bounded at ingest; widened to 1007's `event_id` and pids | `handoff-session-33.md:157`; `handoff-session-35.md:565` | A SPEC-017 amendment, an event's time bounded against its arrival (threat model R9; `docs/handoff-session-39.md:491`); sweep. |
| 32 | A 500 that is not connectivity is resent without bound | `handoff-session-33.md:160` | None recorded; with #31 one event blocks its agent. |
| 35 | The driver's drain ceiling | `handoff-session-33.md:172` | None recorded. |
| 37 | `detect_ac_001` runs a single cycle | `handoff-session-33.md:178` | Drain until a cycle returns fewer rows than the limit. |
| 38 | The forensic evidence unit carries no `class_uid` | `handoff-session-34.md:310` | A precondition of any rule that cites an event other than a process event (ADR-0018 §9, SPEC-019 §Out of scope). |
| 39 | `time` in the CGES examples and the schema README | `handoff-session-34.md:317` | Sweep. |
| 40 | `event.json`'s `time` has no minimum length | `handoff-session-34.md:324` | Sweep. |
| 42 | A POST per batch interval when the server shares the agent's host | `handoff-session-35.md:569` | Connection reuse in the agent's TLS client. |
| 46 | Two rules on the validity of a marquee run clash | `handoff-session-35.md:586` | None recorded. |
| 47 | No retention for the classes other than 3002 | `handoff-session-35.md:591` | Roadmap §F, with #95. |
| 50 | A dead subscription that answered 4317 would not be found | `handoff-session-35.md:604` | None: measuring it restarts a host service (SPEC-020 §Risks). |
| 51 | The S4U success measured only on a local account in a workgroup | `handoff-session-35.md:607` | Watch on any other developer machine. |
| 52 | The process callback writes a `trace` line per record | `handoff-session-36.md:289` | The next change under `agent/cg-agent/src/etw/` (B2's branch, parked). |
| 53 | The AC-006 line lacks `activity_id` and `event_id` | `handoff-session-36.md:300` | With #52. |
| 54 | Stale "roadmap §D" / "B2" destinations in the engine and `rules/windows/README.md` | `handoff-session-36.md:168` | B2's implementation branch (parked). |
| 55 | The threat model described the Phase-0 plan | `handoff-session-36.md:306`; `handoff-session-38.md:527` | Its Amendment 2026-10-15 landed (`9ad6ca6`); open until Manuel's audit of it (`docs/handoff-session-39.md:570`). |
| 56 | `alert.json` says the agent's `event_id`s are UUIDv4 | `handoff-session-36.md:323` | B2's implementation branch (parked). |
| 58 | The schemas require `schema_version`; the wire carries none | `handoff-session-37.md:469` | B2's implementation branch (parked). |
| 59 | No environment runs the agent on Windows 10 or Server 2019 | `handoff-session-37.md:480`; `handoff-session-38.md:525` | Manuel: a machine of each; matters for B2's second session (parked). |
| 61 | The sessions buffer in the mode Microsoft advises against at high rates | `handoff-session-37.md:502` | A load measurement before roadmap §F. |
| 62 | SPEC-005 names the wrong call for exit 9 | `handoff-session-37.md:511` | SPEC-021 (parked). |
| 64 | ADR-0008's helper names differ from the code's | `handoff-session-38.md:1051`; `handoff-session-38.md:1197` | The code takes ADR-0008's names at the next change to `agent/cg-agent/src/etw/`, under the elevated gate. |
| 65 | An orphaned marquee agent can still leave its session | `handoff-session-38.md:1063` | Reopen if a gate leaves a session; a `try`/`finally` in each marquee. |
| 66 | Gate output noise (`logman` text, a stop line with nothing stopped) | `handoff-session-38.md:1070` | The next change to either file. |
| 73 | ADR-0021 §2 says `ajv-cli` reads JSON | `handoff-session-38.md:1127` | An ADR-0021 amendment, for Manuel's "sí". |
| 77 | The bootstrap CLI takes the password on its command line | `handoff-session-39.md:602` | Roadmap §F's SPEC. |
| 78 | No operator setting trusts a relay's private CA | `handoff-session-39.md:608` | Roadmap §F's SPEC. |
| 80 | A request refused for want of a session writes no audit row | `handoff-session-39.md:623`; `handoff-session-40.md:875` | Manuel's reading of ADR-0014 §Compliance (`adr/0014-human-authentication-model.md:98`); its `403` half closed by `8bd5bc9`. |
| 82 | A session lasts 8 h from its login, not 8 h idle within 24 h | `handoff-session-39.md:636` | Sweep: SPEC-008's NFR or the code. |
| 83 | The dashboard enrolls no TOTP and manages no user | `handoff-session-39.md:642`; `handoff-session-40.md:868` | The TOTP enrollment and the user management in the dashboard, with #98's invitation. Required by roadmap §F. |
| 84 | Six places cite ADR-0002 Rule 4 for Zod schemas | `handoff-session-39.md:653` | The SPEC texts, with #13. |
| 85 | A compromised ingest can forge an api session (shared Redis) | `handoff-session-39.md:666` | Roadmap §F: a Redis user per service. |
| 86 | Both services act as the Postgres superuser | `handoff-session-39.md:672` | Roadmap §F: a Postgres role per service. |
| 87 | The agent's private key readable by the enrolling user's processes | `handoff-session-39.md:676` | A SPEC-002 amendment, with the next agent change under the elevated gate. |
| 88 | Agent certificates expire at 90 days, with no renewal and no revocation list | `handoff-session-39.md:683`; `handoff-session-40.md:859` | The agents' renewal, before roadmap §F's installation is called done. The server's half closed by `b293500`. |
| 89 | Enrollment hardening (clear-text token, unchecked scope, no rate limit) | `handoff-session-39.md:694` | Roadmap §F and a SPEC-004 amendment. |
| 91 | Detection rules re-read from disk on every cycle | `handoff-session-39.md:705`; `handoff-session-40.md:879` | SPEC-016's Amendment 2026-10-16: the rule set loaded at the driver's start; closes with its code, after the elevated gate. |
| 92 | The containers run as root; a TLS key file ingest wrote before `b293500` keeps its mode | `handoff-session-39.md:711`; `handoff-session-40.md:864` | Roadmap §F. |
| 93 | No dependency audit in CI | `handoff-session-39.md:714` | Sweep. |
| 94 | Encryption passphrases travel as SQL parameters | `handoff-session-39.md:717` | Roadmap §F's Postgres logging. |
| 95 | Logon telemetry is personal data with no asset entry | `handoff-session-39.md:724` | Roadmap §F's grants, and the DPO's record of processing. |
| 96 | The dev stack runs NATS without authentication and MinIO with dev root credentials | `handoff-session-39.md:729` | Sweep: removed until a SPEC uses them. |
| 97 | The transport between the dashboard and the api is plain HTTP | `handoff-session-39.md:733`; `handoff-session-40.md:872` | Roadmap §F. |
| 98 | Admins set users' passwords and receive their TOTP secret | `handoff-session-39.md:737` | A later SPEC-008 change: an invitation the user completes. |
| 99 | One process holds the internet-facing listeners and the CA's key | `handoff-session-39.md:741` | An ADR separating the CA, for Manuel's "sí". |
| 102 | The api's login deletes no prior session | `handoff-session-40.md:775` | A later SPEC-008 change: the login deletes the session its cookie names. |
| 103 | Without a proxy in front of the dashboard, a client chooses the address the api counts | `handoff-session-40.md:785` | Roadmap §F: the dashboard only behind a TLS proxy that appends or sets `X-Forwarded-For`. |
| 104 | The dashboard sends no security headers (CSP, HSTS) | `handoff-session-40.md:796`; `handoff-session-40.md:882` | Roadmap §F; not `Referrer-Policy: no-referrer`, which would make SPEC-009's same-origin check refuse every sign-in. |
| 105 | A certificate and key path that name one file lose the key at a reissue | `handoff-session-40.md:798` | The next change to `services/ingest/src/cert.ts`. |
| 106 | The dashboard's cookies carry no `__Host-` prefix | `handoff-session-40.md:809` | A SPEC-009 change, with roadmap §F's deployment domain. |
| 108 | Ingest and the api share one ClickHouse credential, which can write the events the forensic key signs | `handoff-session-40.md:820` | Roadmap §F: a ClickHouse user per service, the api's read-only. |

## Closed

| # | Debt | Closed by | Entry |
|---|---|---|---|
| 1 | Node 20 in CI | `67c4a7c`, `64ca6f7` (S25) | `handoff-session-26.md:148` |
| 2 | markdownlint version skew | No skew found (S36) | `handoff-session-36.md:163` |
| 3 | Hand-numbered agent ids across the ingest tests | `ecd0fd4` | `handoff-session-39.md:585` |
| 6 | The api harness's copy of the `cges_events` DDL | `b10bc3d` | `handoff-session-39.md:584` |
| 10 | The drill answered 404 when ClickHouse was down | `ff20923`, `4a6e981` | `handoff-session-38.md:1183` |
| 14 | MVP-criteria altitude grouping | No live instance left (S36) | `handoff-session-36.md:166` |
| 15 | The api image not buildable | `569f92d` | `handoff-session-29.md:116` |
| 16 | `ts-ci` built neither ingest nor api | `faac8b3` | `handoff-session-29.md:118` |
| 18 | Dev ports on all host interfaces | `6841aa5` | `handoff-session-38.md:1178` |
| 19 | Fail-loud left the listeners open | `67f2055`, `eddadd7` | `handoff-session-38.md:1173` |
| 22 | No device-path to Win32 translation | `db41d33` | `handoff-session-32.md:182` |
| 23 | Capture only in test mode | `8c7ee35` | `handoff-session-32.md:183` |
| 28 | `.github/workflows/README.md` stale | `f37b243` | `handoff-session-36.md:155` |
| 33 | The driver's default interval | `ff20923`, `8ea922e` | `handoff-session-38.md:1187` |
| 34 | An incident's `cg_mitre` from its first alert | Retired, folded into #26 | `handoff-session-34.md:309` |
| 36 | The ClickHouse image tag floated | `d94ab7b` | `handoff-session-38.md:1176` |
| 41 | `read-model.test.ts` flake | `f8a2d4a` | `handoff-session-38.md:1191` |
| 43 | Launch and connect out of order | Premise does not hold (S37) | `handoff-session-37.md:544` |
| 44 | SPEC-017 §Operational §6's locks | `cf54945` | `handoff-session-36.md:155` |
| 45 | The local gate missed CI's build steps | `a293c27` | `handoff-session-36.md:155` |
| 48 | ADR-0012:240 said 4001 and 3002 are schema-only | `be69547` | `handoff-session-36.md:155` |
| 49 | ADR-0019's `windows-sys` feature wording | No defect (S36) | `handoff-session-36.md:157` |
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
| 79 | Behind a reverse proxy, the api reads the proxy's address | `c9dad1c` | `handoff-session-40.md:843` |
| 81 | The TOTP confirmation defeated the second factor | `ee5235a` | `handoff-session-39.md:593` |
| 90 | The forensic export and report: any role, no audit row | `8d3c61d` | `handoff-session-40.md:847` |
| 100 | The password change counts no failure and leaves its TOTP code usable | `8bd5bc9` | `handoff-session-40.md:851` |
| 101 | The login limiter reads its counters, then counts the failure | `8bd5bc9` | `handoff-session-40.md:854` |
| 107 | The dashboard's logout answered the api's `403` with `403` after clearing the cookies | `fe59102`, `1697e60` (opened and closed in S40) | `handoff-session-40.md:832` |
