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

Manuel ran the elevated gate on `ebb0394` (`C:\tmp\s40-gate1.ps1`,
2026-10-15, 20:51 to 20:56). Its logs, literal:

```text
01-elev    net session: No hay entradas en la lista.
02-head    ebb0394 / fix/s40-debt-88-server-cert
03-status  status lines: 0
05-build   Finished `release` profile [optimized] target(s) in 10.99s
06-cargo   test auth_ac_012_logons_on_the_real_log ... ok
           test capture_ac_006_elevated_shutdown_leaves_no_session ... ok
           test net_ac_008_a_real_connection_is_reported_with_its_probe ... ok
           test net_ac_009_an_accepted_connection_and_the_filter ... ok
           test ac_004_cache_hit_terminate_matches_launch_byte_for_byte ... ok
           test ac_007_parent_pid_only_when_parent_dead_at_child_launch ... ok
           test ac_009_events_lost_under_deliberate_etw_buffer_pressure ... ok
08-vitest  ✓ test/servercert-ac-renewal.test.ts (19 tests | 1 skipped) 9077ms
           ✓ test/detect-ac-001-marquee.test.ts (1 test) 46787ms
           ✓ test/spec-005-marquee.test.ts (1 test) 40473ms
           ✓ test/net-ac-010-marquee.test.ts (1 test) 25676ms
           ✓ test/auth-ac-013-marquee.test.ts (1 test) 25544ms
           ✓ test/ac-001-marquee.test.ts (1 test) 4570ms
           detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-IjQGgj\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
           spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
           Test Files  73 passed (73)
                Tests  292 passed | 1 skipped (293)
10-agent   logman query CGAgent-KernelProcess -ets: No se encontró el Conjunto de recopiladores de datos.
```

The skipped test is `servercert_ac_008`, which runs only off Windows: it
sets the key file's mode, which Windows does not keep. The cargo log holds
no `refused_attempt_reported` line of `net_ac_008` nor `auth_ac_012`'s
redacted lines, which CLAUDE.md's procedure says they print: libtest keeps
the output of a test that passes unless it runs with `--nocapture`, and the
gate does not pass it.

The branch was rebased onto `6aaea20` (docs only) as `44d2ac4`; `git diff
--quiet ebb0394 44d2ac4 -- services/` returned 0, so the gate covers its
code. `44d2ac4: ts-ci -> success`. It landed on `main` as `b293500` by
`git merge --squash`, behind `git diff --cached --quiet <branch>` and `git
diff --quiet <branch> HEAD`, each stopping the push on a mismatch; PR #18
was closed and the branch deleted, local and on origin.

```text
SHA b293500: ts-ci -> success
```

### #83, #97 and #79: the contracts

This record's first form lands with SPEC-008's and SPEC-009's second
amendments of 2026-10-15, after four passes of the fixed SPEC prompt on
their drafts.

### Two corrections to the contracts, before their code

The code reviews of PR #19 (below) found two places where the amendments
and the code differed. Both were corrected in place in this commit, before
any code that relies on them landed:

- SPEC-008's second Amendment 2026-10-15, item 1 and apiauth_ac_018: the
  code refuses more ranges than `/0`. Item 1 now names them: a prefix past
  its family's length, and any IPv6 range that holds the IPv4-mapped space
  (`::ffff:0:0/96`, `::/80`), which a mapped range matches for IPv4 peers.
  Each entry is checked alone. Item 2's unset case reads the socket's
  address as the socket gives it.
- SPEC-009's second Amendment 2026-10-15, item 2 and dash_ac_005: the hosts
  are compared without regard to case. An empty first `X-Forwarded-Host`
  entry reads `Host`, as Next's check does. The `Origin`'s host is read as
  its URL gives it, and port 80 or 443 is removed from the host addressed
  only, so `http://host:443` no longer passes. Next 15.5.19 sets
  `X-Forwarded-Host` under `next start`. The criterion names an `Origin`
  that matches `Host` while the first `X-Forwarded-Host` entry names
  another host. Item 5 asks the proxy for the browser's `Host` with its
  port, and roadmap §F carries that half of the requirement too.
- SPEC-008's second Amendment, item 4, two sentences on `main` since
  `6aaea20`, which the second pass found false. The first said the walk
  reaches the client only if the hop in front of the outermost listed one
  appends or sets the header, which is false when that hop is a proxy that
  appends. The second said the dashboard neither appends nor sets the
  header, but its server fills the header when a request carries none.

The fixed SPEC prompt ran five times. The first pass found that the draft
had replaced an accurate sentence with a false one (A1): it said Next's
Server Actions check accepts either header. Next 15.5.19 calls
`parseHostHeader` without an origin (`action-handler.js:356`), so it reads
the first `X-Forwarded-Host` entry, or else `Host`. The draft went back to
the accurate sentence; the code reviewer that had prompted the change had
misread Next. The second pass found item 4's two sentences (A1, A2). The
third found that the first rewrite of item 4 still named the entry the walk
takes ambiguously (A1), and that the roadmap carried item 5's requirement
on the proxy without its `X-Forwarded-Host` half (A2); both were fixed,
with the port in the `X-Forwarded-Host` a proxy sets. The fourth found
that a listed hop which sets the header behind another listed hop
discards what that hop wrote (A1), and that item 4's summary of the walk
left out item 2's entries that are not IP addresses (A2): item 4 now asks
the listed hops behind the outermost one to append, and roadmap §F
carries it. The fifth found that this rule left out the dashboard, which
passes the header on unchanged and is safe to list behind a listed proxy
(A1), and that item 4's summary left out item 2's cases where no entry
qualifies (A2). Both were fixed with the wording the reviewer proposed;
no sixth pass ran on those two one-line changes.

A third correction, to SPEC-009's second Amendment, came from the third
code review of PR #19:

- Item 1, and dash_ac_004 and dash_ac_007: a `cgsess` or `cgcsrf` cookie
  whose value is not a token (base64url, as the api writes its tokens) is
  read as absent. The dashboard never sets one, and some values, once
  decoded, cannot go into a header.
- Item 3 and dash_ac_003: a `200` whose token is not a token is answered
  `502`.
- Item 4 and dash_ac_004: the api's `403` on a logout, a CSRF token that
  does not match, is answered `502`. Like a `503`, it clears the cookies
  without the api ending the session, so item 7 takes the browser to the
  login view with its notice. The handler's `403` is left to item 2's
  refusal, which changes no cookie.
- Item 7 and dash_ac_008: the login view, when it opens, and the sign-out,
  when it navigates, drop the router's cached views. Going back from the
  login view, after a sign-out or an expired session, on a shared machine,
  then shows no incident without asking the server.

The fixed SPEC prompt reviewed this correction. It found that the first
draft left the api's `403` signed out but on the page, with its views
cached (A1), and that the draft's reason for the token rule was false (A2):
a tab can go into a header, and a `€` cannot. A second pass found that the
api can answer `503` after it has deleted the session, when its audit row
fails (`services/api/src/auth/service.ts`, `logout`), so items 4 and 7 now
say the session may live on (A1); its wording points were taken too.

### #79, #83 and #97: the code

Branch `feat/s40-dashboard-login` (PR #19), two commits. It was rebuilt
three times to fold the reviews' fixes, rebased onto `8be5e36` and then
`fe59102`, and pushed one commit per push, each with CI:

```text
75e9837 (api):        ts-ci -> success
ccf2865 (dashboard):  ts-ci -> success
b2a155c (api):        ts-ci -> success     rebuilt on 8be5e36, first fixes
988622b (dashboard):  ts-ci -> success
1fdadb4 (api):        ts-ci -> success     second fixes
c69a4fd (dashboard):  ts-ci -> success
2877169 (api):        ts-ci -> success     rebased on fe59102
e3192ea (dashboard):  ts-ci -> success     third fixes
```

Landed on `main` by cherry-pick, one push per commit, each behind `git
diff --quiet <branch-sha> HEAD`, which stops the push on a mismatch. PR #19
was closed and the branch deleted, local and on origin:

```text
SHA c9dad1c (from 2877169): ts-ci -> success
SHA 1697e60 (from e3192ea): ts-ci -> success
```

Red before green:

- apiauth_ac_018 against `11d441b`'s api: 16 of 19 failing for the missing
  list (`expected +0 to be 1`, `expected [Function] to throw an error`).
  The 3 that passed assert the prior behavior, which the amendment keeps.
  Of the tests the reviews added, the one that changes behavior failed
  against the code before its fix: a request with no peer address
  (`Cannot read properties of undefined (reading 'includes')`). The others
  strengthen coverage of behavior that was already right.
- The dashboard's tests against stubs with `main`'s behavior: handlers that
  answer 501, the pages' whole cookie header, a login view that submits
  nothing, a sign-out button that does nothing. 74 of 75 failed for the
  missing behavior. The first run of the dash_ac_003/004 file failed in its
  collection instead: its 400 cases read the api's helpers before
  `beforeAll`. It was fixed and ran again, 25 of 25 failing on `501`. The
  test that passed asserts that the login view shows no notice. Against the
  code before each round of fixes, the reviews' new tests failed where the
  behavior changes: 11 after the first round, 3 after the second, 5 after
  the third.

Local gates at the tips: `services/api` 44 files / 242 tests (it was 43 /
216); `dashboard` 10 files / 117 tests (it was 3 / 8).

Docker Desktop's engine hung during the rebuild (its API answered 500).
Manuel said to restart it ("Reinícialo tú"). Claude Code ran `docker
desktop restart`, and `docker version` then answered with 29.1.2.

#### Self-review: matrices

apiauth_ac_018 (`services/api/test/apiauth-ac-018-source-address.test.ts`):

| Criterion | Test | Verdict |
|---|---|---|
| Listed peer, XFF `203.0.113.7`: counted and `source_ip` under it | "a forwarded address is the one counted and recorded" | covered (row count before and after) |
| An IPv6 entry; `::ffff:203.0.113.9` as `203.0.113.9` | "an IPv6 entry is read in its canonical form, a mapped one as its IPv4 address" | covered (both notations) |
| Two forwarded addresses counted apart | "failures under two forwarded addresses are counted apart" | covered (429 under one, 401 under the other) |
| `198.51.100.4, <listed>` under `198.51.100.4` | "the walk passes listed entries and takes the first that is not" | covered |
| The walk starts from the right | "the walk starts from the right: ..." | covered (second review) |
| Every entry listed: the leftmost | "with every entry listed, the leftmost is taken" | covered |
| No header, or a non-IP entry: the peer | "no header, or an entry that is not an IP address, leaves the peer's address" | covered |
| A failed TOTP confirmation under the forwarded address | "a failed TOTP confirmation is counted under the forwarded address" | covered |
| Unlisted peer, and unset: counted and recorded under the peer | "...: the peer's address is counted and recorded" | covered (every kind of header, the confirmation and its row) |
| Refusals; spaces and empty entries | "the list's configuration", "the list's matching" | covered |

dash_ac_003 to 008 (`dashboard/test/`):

| Criterion | Test | Verdict |
|---|---|---|
| 003 sign-in: 200, the cookies and attributes, a `cgsess` the read API accepts, no token in the body, `no-store` | `dash-ac-003-004-login-logout.test.ts` "a user with a password and a confirmed TOTP signs in" | covered (real api) |
| 003 wrong code: 401, no cookie | "a wrong code answers 401 and sets no cookie" | covered (real api) |
| 003 400 cases, without calling the api | the `test.each` of 400 cases, "a body of exactly 16 KiB is read and sent on" | covered (exact bound, a body cut off) |
| 003 the api's 429/503; a 200 without its token; no answer | the mapping cases; `dash-ac-003-api-client.test.ts` | covered (the bound and a refused connection on a real socket) |
| 003 a login revokes the browser's session | "a login from a browser that holds a session revokes it" | covered (real api) |
| 004 sign-out: deleted, read 401, cookies cleared; `cgcsrf` is the api's token | "a signed-in session signs out...", "a login's cookies sign out..." | covered (real api, round trip) |
| 004 no longer held: 200; mismatched `cgcsrf`: 502, cookies cleared, session kept | the two cases | covered (real api) |
| 004 api 503 or no answer; `cgsess` without `cgcsrf` | the cases | covered |
| 005 refused and passed origins | `dash-ac-005-same-origin.test.ts` | covered (both handlers) |
| 006 `X-Forwarded-For` passed as received, none when none | `dash-ac-006-client-address.test.ts` | covered (both calls of a login) |
| 007 the session alone; no header without one | `dash-ac-007-pages.test.tsx` (the pages), `dash-ac-007-session-alone.test.ts` | covered (the pages themselves, second round) |
| 008 the views | `dash-ac-008-views.test.tsx` | covered |

Before the reviewers, Claude Code's own matrix had two *weak* verdicts. The
first: dash_ac_007 tested the function the pages use, not the pages. The
second: the empty entries of the list were checked in the configuration
only. Both were fixed in the first round.

#### Self-review: the reviewers

Four passes of the fixed code prompt, one reviewer per commit; the fourth
reviewed the dashboard's commit only.
In the first pass, one reviewer ran `git checkout` to `75e9837` and back;
the tree was clean, and the branch and status were checked after each pass.

First pass:

- api:
  - fixed:
    - a row count depended on the order of the test files;
    - the unset and unlisted cases covered one header only;
    - a request with no peer address threw;
    - the matching of mapped ranges and host bits had no test.
  - recorded:
    - the code refused more ranges than item 1 said. Item 1 was corrected
      instead (`8be5e36`), since the refusal fails closed.
- dashboard:
  - fixed:
    - `cgcsrf` was never shown to hold the api's token; now a round trip
      with the real api;
    - `httpAuthApi` and its bound had no test;
    - the pages were not tested;
    - the 403 did not assert cleared cookies;
    - the 16 KiB bound was tested only above;
    - a body cut off answered 500 without `no-store`;
    - the handlers took the first repeated `cgsess` and the pages the last.
  - recorded:
    - 502 for a token that is not base64url;
    - Next's own 405 and OPTIONS answers carry no `no-store`;
    - the 403 tension between items 4 and 7, which became #107.
  - wrongly reported:
    - Next's Server Actions check accepts either header (it reads the first
      `X-Forwarded-Host` entry, or else `Host`; see *Two corrections*).

Second pass:

- api:
  - fixed:
    - no test showed that the walk starts from the right; without it, an
      implementation that read from the left would pass;
    - a non-IP entry left of the one taken;
    - the unlisted peer's confirmation row;
    - an IPv6 prefix past 128;
    - the list is parsed before any client opens.
  - recorded:
    - a mapped range keeps an IPv6 prefix (`::ffff:192.0.2.0/24` is
      refused);
    - prefixes with leading zeros are accepted.
- dashboard:
  - fixed:
    - a `cgsess` that decodes to a control character broke the page's
      header; a value without a token's shape is now no session, in the
      pages and the handlers;
    - the port removal on `X-Forwarded-Host` had no test.
  - recorded:
    - the last repeated cookie wins in the dashboard, where the api takes
      the first; without a `__Host-` prefix a sibling subdomain can set one
      (#106);
    - the import of Next's internal `RequestCookies` path, which would break
      the build, not the runtime, on a move.

Third pass:

- api: nothing blocking. Recorded: the unlisted and unset cases repeat four
  header shapes rather than all; `source_ip` is asserted for the login's
  failure rows only; a prefix with leading zeros is accepted.
- dashboard:
  - fixed through the contract first (`fe59102`):
    - the token-shape rule was not in the amendment;
    - after a sign-out, going back showed the router's cached incident view
      without asking the server; the sign-out and the login view now drop
      that cache;
    - the api's `403` on a logout signed the browser out while the view
      said only that the request was refused; it is now `502`, which takes
      the view to the login with its notice. That was #107, opened and
      closed in this session.
  - recorded: the malformed-body case is tested on the login only, and a
    connection reset has no test.

Fourth pass, dashboard:

- nothing contradicted.
- recorded:
  - the tests do not assert that the refresh comes after the push, which
    the code does in the right order; a failed refresh, or Back during its
    round trip, keeps the old cache;
  - the login body has no time bound of its own beyond Node's 300 s;
  - a `Referrer-Policy: no-referrer`, which #104's headers must not set,
    makes browsers send `Origin: null` and item 2 refuse every sign-in;
  - the import of Next's internal `RequestCookies`.

#### Decisions (delegated, inside the ADRs)

Anticipated:

- **Landing order.** #79 and #83 land as two commits of one PR, since
  SPEC-009's login depends on SPEC-008's list. Alternative: two PRs.
- **The list is read once, at start.** The parsed list is held in
  `Services.trustedProxies`, and the config check uses the same parser.
  Alternative: parse per request.
- **No list keeps the socket's address.** With no list, the source address
  is the socket's, as before; with a list, it is canonical. Alternative:
  always canonical, which would change the address the audit records on a
  dual-stack bind.
- **Refusals beyond `/0`.** A range that holds every IPv4 or every IPv6
  address is refused, as is a prefix past its family's length. Item 1 was
  corrected to say so. Alternative: refuse `/0` alone.
- **Only base64url tokens become cookies.** The dashboard sets a cookie only
  from a base64url token, and otherwise answers 502; it reads a cookie value
  of another shape as no session. Alternative: take the values as they come.
- **The api's own error code passes through.** The handler passes the api's
  `error` string, falling back to the status's own code. Alternative: map
  each status to a fixed code.
- **The sign-out notice travels in the URL.** The notice reaches `/login`
  as `?notice=signed-out`, read by a server page. Alternatives: a cookie, or
  `sessionStorage`.
- **The login view clears two fields after a failure.** It clears the
  password and the code. Alternative: keep them.
- **One cookie parser.** The handlers read cookies with Next's
  `RequestCookies`, as the pages do. Alternative: the handler's own parser,
  which disagreed with the pages on a repeated cookie.

Reactive:

- **The api's logout `403` became `502`.** The view takes `502` to the
  login view with its notice, and the router's cache is dropped there
  (`fe59102`, third review). Alternatives: keep the `403` and read its
  body in the view; leave the cookies on a `403`.
- **The 400 cases became lazy.** They were built at collection time, which
  read the api's helpers before `beforeAll`.
- **`role="status"` became `<output>`.** Biome's a11y rule asked for it.
- **Item 2 matches Next for an empty entry.** An empty first
  `X-Forwarded-Host` entry reads `Host`, and port 80 or 443 is removed only
  on the host addressed, so `http://x:443` no longer passes.

### #90: the contract

SPEC-009's third Amendment 2026-10-15, *the forensic export and report, a
role and an audit row*, lands first as a document, after the fixed SPEC
prompt (see the next paragraph). It amends SPEC-008 and SPEC-013 by scope;
SPEC-012 states no role and is not amended. The forensic export and report become `admin` and `analyst`
only. Each document produced writes one audit row that identifies what the
forensic key signed (the events' count, the chain head, and the report's
SHA-256), and none is sent without it. Decided under Manuel's delegation;
alternatives in the amendment's *Decision*. For Manuel's audit: when the
code lands, the threat model's R7 narrows to the shared Postgres and
ClickHouse credentials (#86, #108), and its record of the system as built
(§3, §7) changes with it.

The first pass of the fixed SPEC prompt found six points that blocked (A):

- read_ac_006's `404` for a viewer's malformed id;
- SPEC-013's "no role gate";
- the report's row recorded nothing of what it signed;
- the second amendment's "the read routes neither count nor record one";
- R7's residual cited the wrong debts, and closing #90, which named the
  shared ClickHouse credential, would leave it without an open debt (now
  #108);
- this record did not yet list the change.

The draft fixed all six, and took the second list's points on `HEAD`
requests (which would sign and send nothing), on `500` beside `503`, and
on the catalog's edge to SPEC-008 and SPEC-013.

### #90: the code

Branch `fix/s40-debt-90-forensic` (PR #20), one commit, rebuilt twice to
fold the reviews' fixes, CI on each push:

```text
d44d66d: ts-ci -> success
9db278f: ts-ci -> success     first review's fixes
0e9fc10: ts-ci -> success     second review's test fixes
```

Landed on `main` as `8d3c61d` by `git merge --squash`, behind `git diff
--cached --quiet <branch>` and `git diff --quiet <branch> HEAD`, each
stopping the push on a mismatch; PR #20 closed, the branch deleted.

```text
SHA 8d3c61d: ts-ci -> success
```

The criteria ran red first, against `main`'s code: 13 of 17 failed for
the missing behavior. The 4 that passed assert behavior already right: the
admin's and the analyst's `200`, and no row for a `404`. That red run also
showed that a `HEAD` request then answered `200`, signing without sending.
Local gate at the tip: `services/api` 45 files / 264 tests.

Self-review matrix (`services/api/test/forensic-ac-001-003-role-audit.test.ts`):

| Criterion | Test | Verdict |
|---|---|---|
| ac_001: a viewer's `403` for its incident, an unknown id and a malformed id, no row | "a viewer's request for ..." | covered (no row of that user at all) |
| ac_001: the analyst's and the admin's `200` | "an %s's export and report answer 200" | covered |
| ac_001: `HEAD` answers `404`, no row | "the %s does not answer HEAD ..." | covered (exact `404`, after the first review) |
| ac_002: the export's row, its columns and `detail` | "an export writes one forensic.export row ..." | covered (the correlation id matched to the logged `reqId`, after the second review) |
| ac_002: the report's row with the PDF's SHA-256 | "a report writes one forensic.report row ..." | covered |
| ac_002: an upper-case `:id` recorded lower-cased | "an :id in upper case is recorded lower-cased" | covered (both routes) |
| ac_002: a `404` writes none | the malformed, unknown and other-org cases | covered |
| ac_003: no document without its row | "when the row cannot be written ..." | covered, with a control that the refusing app still reads |

Before the reviewer, Claude Code's own matrix had one *weak* verdict (the
`HEAD` status) and one *missing* (the upper-case `:id`). Both were fixed.

Two passes of the fixed code prompt:

- First pass, fixed:
  - "no row" counted the forensic actions only, and now counts every row
    of the user;
  - `HEAD`'s exact `404`;
  - the upper-case `:id`;
  - no row for a malformed or another org's `:id`;
  - read_ac_006 keeps its viewer for every other route and takes an
    analyst for the two forensic ones;
  - a comment naming `buildReport`.
- First pass, recorded: the migration `0005` comment, which records what
  the migration did then, is left as it was.
- Second pass, nothing contradicted. Fixed, in tests and a comment only:
  - the correlation id is tied to the request's logged `reqId`;
  - the upper-case `:id` is checked on the report too;
  - forensic_ac_003 gained a control that its `503` is the audit row's;
  - `audit.ts`'s `targetId` comment.

  No third pass ran on those test-only fixes.
- Second pass, recorded:
  - `source_ip` behind a listed proxy is not exercised on these routes; it
    is the helper apiauth_ac_018 tests;
  - a session-less request to the export is not tested for its `401`;
  - the requester's address in `audit_log`, whose retention no document
    states (the DPO's record of processing, #95's destination).

Decisions (delegated):

- Anticipated:
  - **The role gate reuses `makeRequireRole`.** It now takes several
    roles. Alternative: a second factory.
  - **`composeReport` returns the PDF with its seal.** `buildReport`
    stays, as a wrapper for its tests. Alternative: call the export twice,
    which would sign twice.
  - **One request context for both route sets.** `requestContext`
    replaces the auth routes' inline `ctxOf`.
- Reactive:
  - **The `HEAD` forms were removed.** Fastify 5 answers `HEAD` on every
    `GET` route, which here signed without sending (the review of the
    text).
  - **The report's row carries its seal and SHA-256.** The report signs
    too (the review of the text).

The threat model, for Manuel's audit:

- R7 is restated: the key signs what the shared credentials can write
  (#86, #108).
- §3 and §7 now record the role gate and the audit rows.

### #80, #100 and #101: the contract

SPEC-008's Amendment 2026-10-16, *refused requests, the password change, the
limiter's reservation*, lands first as a document, after two passes of the
fixed SPEC prompt.

- A role or CSRF refusal writes an `access.denied` row with its reason and
  route pattern.
- A request refused for want of a session writes none, as now. ADR-0014
  §Compliance (`:98`) asks that "all authentication events" be logged, and
  whether a missing or rejected session token is one is Manuel's reading
  of that ADR. #80 stays open for that half; see *What waits on Manuel*.
- The password change is limited on a per-user counter of its own, writes
  a denied row, and marks its code used.
- Every limited attempt reserves its slots atomically, with one Redis
  script, before the credentials are checked. A failure restarts the
  window, as before; a success gives its slots back.

It amends SPEC-009's third Amendment 2026-10-15 by scope (item 3 and
forensic_ac_001): a viewer's refused export or report writes the
`access.denied` row and no forensic row. Decided under Manuel's
delegation; the alternatives are in the amendment's *Decision*. For
Manuel's audit: when the code lands, four sentences of the threat model's
§3 change with it:

- "a request the session, role or CSRF check denies writes no row (#80)";
- "rate limits exist only on the login and the TOTP confirmation";
- "the login and the TOTP confirmation do write one for a failure";
- "A read writes none".

The first pass of the fixed SPEC prompt found six points that blocked (A):

- forensic_ac_001's test counts every row of the viewer, so it changes;
- the threat model's §3 had more sentences that change;
- #80's `401` half was dropped from its restatement;
- the first amendment's item 6 said a refused change writes no row;
- #101's destination named increment-then-check;
- the amendment did not say it is not backward-compatible.

From the first pass's second list, the draft took:

- a success no longer restarts the window;
- a counter of the password change's own;
- a counter that reaches 0 is removed;
- `503` for a store failure and `500` for any other;
- an attempt that ends in a store failure keeps its slots, which the third
  pass turned around (below).

The second pass found six more (A):

- the password change still shared the login's per-IP counter, which
  unauthenticated requests fill (now it does not);
- SPEC-009's forensic_ac_001 criterion still said "write no row";
- a present-tense claim about a test, now preceded by the document-change
  sentence;
- one listed incompatibility was not one, and the real one (attempts in
  flight count) was missing;
- the `401` half had been closed "by decision" against ADR-0014's MUST,
  which is Manuel's to read (now open, for him);
- this record's count of the threat model's sentences.

From the second pass's second list, the draft took:

- the wording of what the limit prevents;
- the edge of a window that ends in flight;
- the Decision naming the API contract;
- the criteria's wording.

The third pass found two (A):

- this record pointed at a *What waits on Manuel* section it did not yet
  have (now begun, below);
- slots kept after a store failure would have counted an outage against
  accounts, against "the window unchanged". Now such an attempt gives its
  slots back.

From its second list, the draft took:

- the code citation for item 9's give-back;
- removing a promise about windows that end in flight, which plain counters
  cannot keep;
- the code marked used before the change;
- the alternatives of items 1 and 2;
- the report's refusal and the route without its query in the criteria;
- the NFR-009-004 line.

No fourth pass ran on those changes.

The code's first review (PR #21) found two places where the text and the
code read differently, and the text was clarified before the code landed:

- A failure whose audit row then fails keeps its slots; only an attempt a
  store interrupts before its outcome gives them back. A counter that Redis
  being down puts out of reach keeps them until its window ends.
- The refused request's row: `503` when it cannot be written. A `500` for
  another failure was unreachable, since every failure of the write is the
  store's.

### #80, #100 and #101: the code

Branch `fix/s40-debts-80-100-101` (PR #21), one commit. It was rebuilt
three times to fold the reviews' fixes, rebased once onto `fac5654`, and
pushed with CI on each:

```text
632fef6: ts-ci -> success
4ef6c68: ts-ci -> success     first review's fixes, on fac5654
e9b0937: ts-ci -> success     second review's fixes
574638e: ts-ci -> success     third review's fix
```

Landed on `main` as `8bd5bc9` by `git merge --squash`, behind the two tree
guards, each stopping the push on a mismatch; PR #21 closed, the branch
deleted.

```text
SHA 8bd5bc9: ts-ci -> success
```

The criteria ran red first, against `main`'s code: 12 of 18 failed for
the missing behavior. The 6 that passed assert behavior the old code
already had: no row without a session, counters the password change did
not share, a success, a `429`, and a store failure that counted nothing.
The red run showed #101's race: twenty parallel failed logins from one
address answered twenty `401`, none `429`. Local gate at the tip:
`services/api` 46 files / 282 tests; the dashboard's tests against the
real api, 38 of 38.

Self-review matrix
(`services/api/test/apiauth-ac-019-021-refused-password-reservation.test.ts`):
every clause of apiauth_ac_019, 020 and 021 covered. Claude Code's own
matrix had none weak; the reviewers found four (below).

Three passes of the fixed code prompt:

- First pass. Fixed:
  - the row's org now comes from a session of a non-default org;
  - a kept window is asserted to be still running (`ttl > 0`);
  - a store failure starts from counters above 0.

  Clarified in the contract first (`fac5654`): a failure whose row fails
  keeps its slots; a `503` when the refused request's row cannot be
  written, the `500` branch being unreachable. Recorded: a give-back can
  take the slot of another attempt in flight (an edge the text names).
- Second pass. Fixed:
  - each outcome settles its slots before the Redis call that records it,
    so a failure stays counted and a give-back never runs twice;
  - a failure whose counter's window ended in flight counts afresh at 1;
  - the confirmations' statuses are asserted;
  - apiauth_ac_014's comment.

  Recorded:
  - a corrupt counter can leave a partial reservation, as `MULTI` could;
  - the give-back on any error, not only a store failure, which no client
    can trigger after the credential checks;
  - the confirmation now fails closed on a Redis error reply (the first
    amendment's item 14).
- Third pass. One contradiction: the success's give-back had been made to
  swallow a Redis error, answering `200` unlogged, against the first
  Amendment 2026-10-15 item 14. Fixed by letting it propagate: a `503`,
  logged, with the slots counted until their window ends. No fourth pass
  ran on that revert.

Decisions (delegated):

- Anticipated:
  - **Where the refusal's row is written.** The error handler writes it,
    once for every role or CSRF refusal. Alternative: each `preHandler`
    writes its own.
  - **The reservation is three Lua scripts.** One reserves, one records a
    failure, one gives back; each runs atomically. Alternative: `WATCH`
    and `MULTI`, which retry under contention.
  - **The password change's counter is its own `rl:pw:<id>`.**
- Reactive:
  - **The give-back after a store failure** (the third text review).
  - **The settle order** (the second code review).
  - **The failure that recreates its counter** (the second code review).
  - **The success's give-back propagates its Redis error** (the third
    code review).

Deployment note, for roadmap §F's Redis user per service (#85): the api's
login now runs `EVAL`, so its user needs the `@scripting` category.

### #91: the contract

SPEC-016's Amendment 2026-10-16, *the rule set is loaded once, at the
driver's start*, lands first as a document, after the fixed SPEC prompt.
The production driver loads `INGEST_DETECT_RULES_DIR` once, at its start,
with the checks it already ran there. A change to the directory takes
effect at the next start, and must pass the same checks. A file that fails
to load fails that start instead of every cycle. This follows the threat
model's own plan (`:99`, rules loaded only at startup; `:105`, no hot
reload).

An anticipated decision, Claude Code's inside ADR-0012; the alternatives,
both forms of hot reload, are in the amendment. Its code touches
`services/ingest/src/detect/`, so it waits for an elevated gate.

For Manuel's audit: when the code lands, the threat model's R8 narrows,
and its record of the system as built (§4, `:305`) changes with it.

The fixed SPEC prompt found three points that blocked (A), all fixed:

- the criterion's id, `rules_ac_005`, was taken (now rules_ac_007);
- SPEC-018 §Operational §3 says the rules load on every cycle (now amended
  by scope, with its catalog edge);
- the draft called "silent" a failure the driver logs.

From its second list, the draft took:

- a criterion that tells a load at start from a reload that keeps the last
  good set, by applying a valid change alone first;
- the checks the driver runs, named where they are;
- the off switch;
- the threat model's plan;
- the dormant broken file that stops ingest at the next start;
- ADR-0012's call.

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
- **#106 — The dashboard's cookies carry no `__Host-` prefix.** A sibling
  subdomain is same-site, so neither `SameSite=Strict` nor SPEC-009's
  same-origin check stops it from setting `cgsess` or `cgcsrf` with
  `Domain=<parent>`. Next's cookie parser keeps the last of a repeated
  cookie, so it can plant a valid pair of its own (a login-CSRF variant) or
  break the pair the browser holds. `__Host-cgsess` and `__Host-cgcsrf`
  would close it; the attributes the prefix requires (`Secure`, `Path=/`,
  no `Domain`) are already set. Surfaced by the second and third code
  reviews of PR #19. Destination: a SPEC-009 change, with roadmap §F's
  deployment domain.

- **#108 — Ingest and the api share one ClickHouse credential, which can
  write the events the forensic key signs.** The export signs what the drill
  reads from `cges_events` (`services/api/src/forensic/export.ts`), and the
  api's ClickHouse credential is the same one ingest writes with
  (`infra/dev/docker-compose.dev.yml`), so whoever holds it can change, before
  an export, the evidence the key then attests. Threat model R7's residual
  with #86. Surfaced by the review of SPEC-009's third Amendment 2026-10-15.
  Destination: roadmap §F, a ClickHouse user per service, the api's
  read-only.

Opened and closed this session:

- **#107 — The dashboard's logout answered the api's `403` with `403` after
  clearing both cookies** (SPEC-009's second Amendment 2026-10-15 as landed
  in `6aaea20`, items 4 and 7). The view then stayed on the page saying the
  request was refused, with the browser signed out, the session alive on
  the server and the router's cached views kept. Surfaced by the third code
  review of PR #19, before any code shipped it. Closed by `fe59102` (the
  contract: the api's `403` is answered `502`, which takes the view to the
  login with its notice) and `1697e60` (the code).

Closed this session:

- **#79**, by `c9dad1c`: the api reads the source address through
  `API_TRUSTED_PROXIES` (SPEC-008's second Amendment 2026-10-15). Roadmap
  §F's SPEC lists the addresses for its compose.

- **#90**, by `8d3c61d`: the forensic export and report answer only an admin
  or an analyst, and each document is recorded with what the key signed
  (SPEC-009's third Amendment 2026-10-15).

- **#100**, by `8bd5bc9`: the password change is limited on a per-user
  counter of its own, records a failure, and uses up its code (SPEC-008's
  Amendment 2026-10-16, item 2).
- **#101**, by `8bd5bc9`: every limited attempt reserves its slots in one Redis
  script before its credentials are checked (item 3).

Changed this session:

- **#88**: its server half closed with `b293500`: the ingest server reissues
  its self-issued certificate before it expires (SPEC-004 Amendment
  2026-10-15). It stays open for the agents' certificates: no renewal and no
  revocation list, before roadmap §F's installation is called done. The
  threat model's R5 and its record of the system as built say so.
- **#92**: since `b293500` ingest writes its TLS key file `0600`; a key file
  it wrote before keeps its mode until a reissue rewrites it. It narrows to
  that and the containers' user, roadmap §F. The threat model's R15 says
  so.
- **#83**: narrowed by `1697e60`, the dashboard's login and logout
  (SPEC-009's second Amendment 2026-10-15), to the TOTP enrollment and the
  user management in the dashboard, with #98's invitation. Still required
  by roadmap §F.
- **#97**: narrowed by `1697e60`, whose pages forward the session alone, to
  the transport between the dashboard and the api: plain HTTP by default,
  which a login's password and TOTP code now cross too. Roadmap §F.
- **#80**: narrowed by `8bd5bc9` to its `401` half, a request refused for
  want of a session, which waits for Manuel's reading of ADR-0014
  §Compliance (`docs/adr/0014-human-authentication-model.md:98`); see *What
  waits on Manuel*. Its `403` half closed with that commit.
- **#91**: its destination is SPEC-016's Amendment 2026-10-16, the rule
  set loaded at the driver's start; it closes with its code, after the
  elevated gate.
- **#104**: the headers must not include `Referrer-Policy: no-referrer`:
  under it browsers send `Origin: null` on a same-origin POST, and SPEC-009's
  same-origin check would refuse every sign-in and sign-out (the fourth
  code review of PR #19).

## What waits on Manuel

This section is begun mid-session; the close completes it.

- **The `401` half of debt #80: a reading of ADR-0014.** ADR-0014
  §Compliance (`docs/adr/0014-human-authentication-model.md:98`) says "All
  authentication events and role assignments MUST be audit-logged". The
  question is whether a request that carries no session token, or a
  rejected one, is an authentication event.
  - If it is, every such `401` writes an audit row, and the api needs a
    bound on them (a per-address limit), since anyone who reaches it can
    send them.
  - If it is not, the request log is its record, and #80 closes.

  Recommendation: it is not. An authentication event is an attempt to
  authenticate (a login, a TOTP confirmation, a password change), and each
  of those writes its row. A session check is the use of an authentication
  already made. SPEC-008's Amendment 2026-10-16 leaves it open until his
  answer.
