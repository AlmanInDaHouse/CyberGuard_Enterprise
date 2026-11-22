# Handoff — Session 94

Session 94, 2026-11-22, built the pages of F3's phase 1, the accounts (SPEC-026 §Operational
§9, item 1): the dashboard's mutation handlers (dash_ac_009), the navigation by role (dash_ac_010's
part), `/account`, `/invitation` with its QR code (dash_ac_011) and `/users` (dash_ac_012's
invitation half), with the threat model's lines the pages make true and R14 closed. Debts #83
and #98 closed. A RustSec advisory published on 2026-11-20 (RUSTSEC-2026-0592, `url` 2.5.8) turned
the PR's `dependency-audit` red; `url` moved to 2.5.9 under the elevated gate (PR #160), which
Manuel ran, and the PR's last four commits were rebased onto it. Ingest's migration
`0011_dashboard_rights` and the detection change did not start: they need the elevated gate and a
session of their own (*How Session 95 resumes*). Manuel gave no OK to R5's amendment this session;
its branch stands. No tag was pushed; the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `1e7ee72` (Session 93's close), clean, no stash; branches `main`
  and `docs/tm-r5-revocation-built` (`57d7d75`, owner review), no open PR. The tip's CI was ALL
  GREEN (`markdown-lint`). Counts: 22 ADRs / 25 SPECs, 54 open debts, no Known CI debt. The repo
  agreed with Session 93's handoff. 28 GB free on `C:`. Docker Desktop was down and was started.
- Landed on `main`, by fast-forward to the SHA each commit's CI ran, one commit per push:
  - PR #159: `9e1965e` (the mutation handlers), `1806654` (the navigation and `/account`);
  - PR #160: `cb073b4` (`url` 2.5.9, RUSTSEC-2026-0592);
  - PR #159, rebased onto `cb073b4`: `5cfcde2` (`/invitation` and the `qr` dependency; `78d4ebc`
    before the rebase), `d12e851` (`/users`; `3174f3c`), `900bc2e` (the threat model's lines;
    `00c1efb`), `a053795` (the review's test fixes; `1bab28d`). The rebased tip's tree is
    `1bab28d`'s with `cb073b4`'s `Cargo.lock`.
- Resulting `main`: those commits and this close's.
- Branches: `main`, and `docs/tm-r5-revocation-built` (`57d7d75`, owner review, pushed, no PR),
  which still waits for Manuel (*Waiting on Manuel*). PRs #159 and #160 closed, their branches
  deleted; the gate's worktree `C:\Users\manul\cg-s94-gate` removed (no junction pointed outside
  it).
- Catalogs: 22 ADRs / 25 SPECs, unchanged.

## What landed

- **The mutation handlers** (`dashboard/src/lib/auth/mutations.ts`, nine routes under
  `dashboard/app/api/`): `account/password`, `users`, `users/[id]/role`, `/status`,
  `/invitation`, `/invitation/withdraw`, `/reset`, `invitations/totp`, `invitations/complete`. In
  order: the same-origin check of SPEC-009's second Amendment 2026-10-15 item 2 (`403`); for a
  route with a session, the `__Host-cgsess` and `__Host-cgcsrf` cookies (`401` without both); the
  `[id]` as a UUID (`404`); the body, `application/json` up to 16 KiB with exactly the route's
  string fields (`400`). The api is called with `Cookie: cgsess=<token>` and `x-csrf-token`, the
  invitation's two with neither, and the request's `X-Forwarded-For`. Its `200` and `201` pass with
  their body, its `204` with none, its refusals with their status and `error` code; any other answer
  `502`, no answer `503`; every answer `no-store`. The password change's `200` clears both cookies.
  The login's helpers in `handlers.ts` are exported for it; `AuthPath` is `/v1/${string}`.
- **The navigation** (`dashboard/src/components/app-nav.tsx`, `dashboard/src/lib/signed-in.ts`):
  every signed-in page reads `GET /v1/auth/session` first, with the session alone, and shows the
  incidents and the account for every role, the users' page for an admin alone.
- **`/account`**: the password change, the new password twice; on `200` the form goes to `/login`.
- **`/invitation`** (`dashboard/src/components/invitation-form.tsx`): the invitation read from the
  fragment once, the fragment removed with `history.replaceState`, the invitation kept in the
  component's memory; the provisioning URI as a QR code and as text; the completion. A reload asks
  for nothing. The QR code is the page's own SVG elements (`dashboard/src/components/qr-code.tsx`):
  no injected markup, no image, the CSP unchanged.
- **`/users`** (`dashboard/app/users/page.tsx`, `dashboard/src/components/users-admin.tsx`): the
  org's users a page at a time by the api's cursor; a new user, a new invitation, its withdrawal, a
  reset (asked twice), the role and the status. The link `<origin>/invitation#<token>` of an
  invitation the page issues is held in the view's memory alone and shown once. Another role's
  request is not found and reads no user; the api's `403` is not found, a refused cursor returns to
  the first page.
- **`url` 2.5.9** (`cb073b4`): RUSTSEC-2026-0592 (2026-11-20, a stack overflow in `Url::origin()`
  on nested `blob:` URLs) covers `url` 2.5.8, which `cg-agent` links through `reqwest`;
  `cargo update -p url --precise 2.5.9` moved `url` and its sibling `form_urlencoded` (1.2.2 to
  1.2.3), and added no crate. `docs/security/dependency-review.md`, fix 1.
- **The threat model** (`900bc2e`), the Amendment 2026-11-20 (SPEC-026) executed in place: §9's
  "every mutation are route handlers", "enrolls a user's TOTP factor at the invitation's completion
  and manages users", the R14 sentence removed; R14 closed as the amendment words it.

## Decisions

### Manuel's

- **The local Rust gate's eight failures accepted as an environment debt** (#175), 2026-11-22:
  "2- a", on Claude Code's option (a).
- **R13 left until phase 3**, 2026-11-22: "3-sigo tu recomendacion".
- **The elevated gate's one failure accepted as a debt** (#174), 2026-11-23: "opcion a".

### Anticipated (Claude Code)

- **The QR library: `qr` 0.7.2** (Paul Miller, npm `qr`), licence `(MIT OR Apache-2.0)`, no
  dependency, no install script (its `allowScripts` names a devDependency of its own build),
  published 2026-09-28, pinned exactly; the lockfile's integrity matches the tarball read before the
  install (`sha512-yovYRuac...`). Reason: it encodes the matrix (`encodeQR(text, "raw")`), which the
  page draws as its own SVG elements. Not heavyweight under CLAUDE.md *Decision authority*.
  Discarded: `qrcode` (dependencies), `qrcode-generator` and `uqr` (no dependency, but less
  maintained), and an SVG string or `data:` image (markup injected into the page, or an image the
  CSP would need to admit).
- **One generic mutation handler over a table of routes.** Discarded: nine handlers, each repeating
  the checks.
- **The handler's order: origin, cookies, id, body**, the api's own order for the last three.
  Discarded: the body before the cookies, which reads a body the api would refuse.
- **The body re-serialised from the route's fields**, so the api receives what the handler checked.
  Discarded: the body passed through, which the api's strict schemas would refuse anyway.
- **The api's `error` code passed through on a refusal**, its other fields dropped; the codes reach
  only the role the api admitted.
- **A missing CSRF cookie with a session is `401`**, as no session: both are set together.
  Discarded: `503`, the logout's answer, which a mutation has no reason to give.
- **The session read first on every signed-in page**, one request more per page. Discarded: the
  navigation in a layout, which cannot hand the role to the page.
- **The users' page is not found for another role.** Discarded: a refusal page, which tells a
  viewer the page exists.
- **The issued link in the view's state alone, until "Done"**; a reset asks twice; no reset,
  status or invitation control on the admin's own row; "New invitation" only for a user without a
  confirmed factor.
- **The threat model's §9 forwarded-address clause, its "which the secrets R13 names cross too",
  and R13's restatement wait** for the pages that bring the reads that record an address (phase 2)
  and the enrollment tokens (phase 3): the amendment's Status says R13 stands as written until the
  code it describes lands, and R13's restatement names an enrollment token. Until then R13 names
  less than crosses the leg (`docs/security/threat-model.md:328`): the pages carry a password
  change's passwords and code, an invitation, its completion's password and code and a provisioning
  URI. Manuel chose to leave it (*Manuel's*).
- **`0011_dashboard_rights` moved to Session 95** (`feedback-checkpoint-over-rushing`): it needs
  the elevated gate and a review of its own.

### Reactive corrections

- **`dependency-audit` red on `78d4ebc`** (the PR's lockfile change runs every job): `cargo audit`
  found RUSTSEC-2026-0592 on `main`'s `Cargo.lock`, which no change of the session caused. Fixed by
  its own PR (#160) before the rest of #159 landed, as `dependency-review.md` orders, rather than
  accepted as debt. `cargo update -p url` moved nothing; `--precise 2.5.9` did, its MSRV (1.63)
  under the toolchain's.
- **The background CI waits and the first local Rust gate were reaped** by Claude Code under memory
  pressure; the gate was run again in the foreground with `-j 1` on Manuel's word.
- **Claude Code's terminal was elevated** (high mandatory level, Administrators enabled,
  `SeDebugPrivilege` enabled), against CLAUDE.md's *Environment facts*; eight Rust tests that expect
  a refusal fail there, with `main`'s `Cargo.lock` too (#175).
- **`dash-ac-007-pages.test.tsx`** expected one read per page; the pages read the session first. It
  now answers the session, then the page's read, and checks the cookie on both.
- **The account page repeated the user's email** that the navigation shows; it shows the role.
- **A comment said `0`** where the code returns `null` (`invitation-form.tsx`).
- **The `/users` commit was formatted after a lint run whose exit `tail` hid**
  (`feedback-lint-exit-not-tail`): `1a825bd`, never pushed, was amended into `3174f3c`.
- **The CI script wrote its file under bash's `/tmp`**, which node reads as `C:\tmp`
  (`feedback-bash-heredoc-backslashes`); its first Monitor was stopped and the script fixed.
- The review's fixes, below.

## The review (CLAUDE.md compensating control 5)

Reviewers and verifiers ran on Sonnet, in isolated worktrees, with the fixed code prompt; one
verifier per item that could hold.

| Criterion | Test | Verdict |
|---|---|---|
| dash_ac_009 | `dashboard/test/dash-ac-009-mutations.test.ts` | covered after fixes |
| dash_ac_010, phase 1's part | `dashboard/test/dash-ac-010-navigation.test.tsx` | covered (the rest with phases 2 and 3) |
| dash_ac_011 | `dashboard/test/dash-ac-011-invitation.test.tsx` | covered |
| dash_ac_012, the invitation half | `dashboard/test/dash-ac-012-users.test.tsx` | covered after fixes (the token half with phase 3) |
| §9, `/account` | `dashboard/test/account-password-change.test.tsx`, dash_ac_009's run against the api | covered |

- Red first: against stubs, the handlers answered `501` (dash_ac_009, 178 cases: the behaviour's,
  two cases that passed with no call made were tightened to assert the call); the navigation's
  entries were empty and the pages read no session; the forms and pages rendered nothing.
- Two reviewers (`1e7ee72..00c1efb`): no code defect. Items:
  - R13 and §9's "now cross too" clause not applied (both reviewers): verifier REFUTED, the
    amendment's Status and R13's restatement, which names an enrollment token, support waiting; its
    residual, R13 naming less than crosses until phase 3, went to Manuel, who chose to leave it.
  - `no-store` asserted on some answers only: verifier HOLDS (5 of 11 return paths). Fixed in
    `1bab28d` (`a053795` after the rebase), probed red by setting `private` in `respond`.
  - dash_ac_012's "nowhere else": verifier HOLDS narrowly (a cookie, a router navigation or a
    history entry carrying the token would have passed). Fixed in `1bab28d`, `a053795` after
    the rebase (`tokenNowhereElse`), probed red by writing the token to `document.cookie`.
  - The `qr` dependency unrecorded: recorded above.
  - `no-store` on the `204` unasserted: not so, the test asserts it.
- No fresh pass after the fixes: they are test assertions, no security code changed.
- **The url fix** (`1806654..cb073b4`, the defect statement of RUSTSEC-2026-0592 as the contract),
  one reviewer (`feedback-dependency-fix-needs-reviewer`): no item; only `url` and
  `form_urlencoded` moved, no crate added, `cargo audit` clean on the tip's lock; its note on the
  collateral `form_urlencoded` bump is recorded above. `cargo fetch --locked` checked the
  lockfile's checksums.
- Recorded, not changed: the reload is a remount in jsdom; a browser's back-forward cache restoring
  `/users` with the link is not tested (the page renders at request time, with Next's dynamic
  `Cache-Control`); the `401` of the password change reads "the current password or the code was
  not accepted" also when the session has ended.

## Gates and CI

- Local, on the branch's tip (`dashboard`, the *Dependencies* and the *bundle and runbooks*
  bullets):
  - build `0`, typecheck `0`, lint `0` ("Checked 72 files", no fixes);
  - "Test Files 17 passed (17)" / "Tests 370 passed (370)";
  - `pnpm install --frozen-lockfile` `0`; `pnpm audit --prod --audit-level=high` `0` ("Severity: 4
    low | 10 moderate"); `pnpm audit --audit-level=critical` `0`;
  - `licences admitted: 432 packages`; deploy tests 64, 60 pass, 0 fail, 4 skipped;
  - markdownlint on `docs/security/threat-model.md`, its exit read alone: `0`.
- The url fix (`cb073b4`), local, Claude Code's elevated terminal:
  - `cargo fmt --check` `0`; `cargo clippy --all-targets --all-features -D warnings` `0`;
  - `cargo test --all --no-fail-fast`: 641 passed, 8 failed, 11 ignored; the same eight fail with
    `main`'s `Cargo.lock` (`url` 2.5.8) and pass in rust-ci (#175):
    `install_ac_005_a_descriptor_windows_refuses_creates_nothing`,
    `install_ac_013_a_record_held_open_is_not_taken_for_absent`,
    `install_ac_013_a_file_held_open_is_code_5_by_name`,
    `package_adoption_a_data_root_that_cannot_be_listed_is_code_13_before_the_lock`,
    `package_adoption_a_listing_that_fails_stops_step_6`,
    `package_layout_directories_that_cannot_be_listed_are_untrusted`,
    `install_ac_017_prim_02_parent_grants_the_act`,
    `package_reversal_of_a_host_whose_names_cannot_be_read_is_40`;
  - "agent version matrix: all steps passed"; the release build `0`, `cg-agent.exe` at 23:04:51
    after the commit's 22:28:59;
  - `cargo audit` `0` (the two allowed unmaintained warnings);
  - deploy tests 64, 60 pass, 4 skipped; Windows package tests 86, 77 pass, 9 skipped; "pins
    verified: 4 packages at 7.0.1", with `--tool` too.
- The elevated gate, `C:\tmp\s94-gate1.ps1`, run by Manuel on `cb073b4` (tree `a424961d`) in the
  worktree `C:\Users\manul\cg-s94-gate`: `net session` exit `0`; Docker up; the release
  `cg-agent.exe` at 23:12:12; the `--ignored` tests all passed, the seven of CLAUDE.md among them;
  vitest "Test Files 1 failed | 98 passed (99)" / "Tests 1 failed | 557 passed | 2 skipped (560)",
  the failure `marquee-agent-etw.test.ts` "stopEtwSessions, against logman > stops a running
  session and reports none left" (`logman start` exit 0, then `logman query` nonzero), which
  launches no agent; "detect_ac_001 drained 25 rows in 1 cycles", `detect_marquee_elapsed_seconds`
  39.158, the captured images in Win32 form; no `CGAgent-KernelProcess` left. The failing test
  passed 3 of 3 alone in Claude Code's elevated terminal; Manuel accepted it as #174.
- No elevated gate for the pages: nothing of theirs is on its paths.
- CI, each push, the REST fallback:

```text
SHA 9e1965e (PR #159)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 1806654 (PR #159)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 78d4ebc (PR #159, before the rebase)
  deploy-ci -> success
  ts-ci -> success
  dependency-audit -> failure  https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/actions/runs/45071365831
Verdict: FAILURES (cargo audit: RUSTSEC-2026-0592 on url 2.5.8; fixed by PR #160)
SHA 9e1965e (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 1806654 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA cb073b4 (PR #160)
  dependency-audit -> success
  ts-ci -> success
  deploy-ci -> success
  rust-ci -> success
  package-ci -> success
Verdict: ALL GREEN
SHA cb073b4 (push, main)
  package-ci -> success
  dependency-audit -> success
  ts-ci -> success
  rust-ci -> success
Verdict: ALL GREEN
SHA 5cfcde2 (PR #159)
  dependency-audit -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA d12e851 (PR #159)
  deploy-ci -> success
  dependency-audit -> success
  ts-ci -> success
Verdict: ALL GREEN
SHA 900bc2e (PR #159)
  dependency-audit -> success
  deploy-ci -> success
  markdown-lint -> success
  ts-ci -> success
Verdict: ALL GREEN
SHA a053795 (PR #159)
  markdown-lint -> success
  dependency-audit -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 5cfcde2 (push, main)
  dependency-audit -> success
  ts-ci -> success
Verdict: ALL GREEN
SHA d12e851 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 900bc2e (push, main)
  markdown-lint -> success
Verdict: ALL GREEN
SHA a053795 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
```

- `3174f3c`, `00c1efb` and `1bab28d`, the pre-rebase forms of `d12e851`, `900bc2e` and `a053795`,
  were never pushed. package-ci's bundle job takes about 30 minutes, past *CI monitoring*'s 10, as
  on the PR's run; Claude Code compared both runs' steps before waiting.
- This close's push: CI on its SHA (`event=push`), reported in the turn.

## Test baselines

- `dashboard`: 370 tests, 17 files (141, 12 at Session 93's close).
- `services/api`: unchanged, 512 tests, 64 files.
- `services/ingest`: unchanged from Session 91 (unelevated 560: 553 passed, 4 failed, 3 skipped).

## Debts

- Closed: **#83**, by `5cfcde2` and `d12e851`: the dashboard enrolls a user's TOTP factor at the
  invitation's completion and manages users. **#98**, by `d12e851`, with the api's half
  (`60af68d`, `9144889`): an admin creates and resets a user by an invitation the user completes.
- Opened: **#174**, the elevated gate's `logman` probe (`marquee-agent-etw.test.ts`, "stops a
  running session") failed once under the full suite and passed alone; cause not diagnosed;
  destination: the next elevated gate, run again and diagnosed if it fails. **#175**, eight Rust
  tests that expect Windows to refuse fail in an elevated Claude Code terminal and pass in rust-ci;
  cause not diagnosed (the runner is an Administrator too); destination: the next session, run
  unelevated, and *Environment facts* corrected for whichever terminal Claude Code runs in.
- Open at close: **54**.

## Waiting on Manuel

- Unchanged since Session 87's close: the Authenticode decision and its purchase; #130's legal
  readings; the commercial licence and terms; `SECURITY.md`'s contact; the isolated machines
  (M3); a penetration test; #99, #124 and #125.
- **R5's amendment**, on `docs/tm-r5-revocation-built` (`57d7d75`), as Session 93's handoff states
  it.

## How Session 95 resumes

1. Run the start protocol:
   - `main` is this close's commit; the branches are `main` and `docs/tm-r5-revocation-built`
     (owner review, unless Manuel has answered); no PR;
   - 54 open debts; 22 ADRs / 25 SPECs;
   - whether the terminal is elevated (`whoami /groups`): unelevated, run `cargo test --all` and
     close or keep #175 by its result.
2. If Manuel has given his OK to R5's amendment, land it as Session 93's handoff, step 2, says.
3. **Ingest's `0011_dashboard_rights` with the detection change** (SPEC-026 §Data contracts §1,
   §Operational §7), through a branch and a PR, its tests seen red first:
   - the migration, as `cg_ingest_owner`, its grants only where the role exists (`0009`'s model):
     the api's column grants on `enrollment_tokens`, `agents`, `incidents`, `alerts`; `SELECT` on
     `triage_history` to `cg_incident_reader`; ingest's `UPDATE` on `incidents` narrowed to
     `(alert_ids, severity_id, cg_mitre, updated_at)`; `triage_history` with its index, its
     `SECURITY DEFINER` trigger (`search_path = ingest, pg_temp`) and its write-once triggers; the
     open incident's partial unique index in place of `incidents_grouping_key_unique`;
   - grouping's upsert (`services/ingest/src/detect/incidents.ts`) on the open incident's key, and
     the techniques accumulated in the alert's order: incident_ac_006 and 007;
   - deploy_ac_004's remaining rows (`services/api/test/deploy-ac-004-005-postgres.test.ts`), the
     history's `changed_by_role`;
   - the elevated gate (CLAUDE.md compensating control 4): its script `C:\tmp\s95-gate1.ps1` for
     Manuel, the change landing only after a green run that covers it;
   - self-review: the fixed code prompt on Sonnet, two reviewers, a verifier per item, a fresh pass
     after a security fix.
4. Then the triage routes and phase 2's pages (with §9's forwarded-address clause), then the tokens,
   agents and revocation routes with phase 3's pages, R13's restatement and the revocation's
   runbook (SPEC-026 Amendment 2026-11-22), then the audit view (phase 4), then **F4's agent
   half** after its SPEC-024 amendment.
