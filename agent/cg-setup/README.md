# cg-setup

The setup's elevated code, `cg-setup.exe` ([SPEC-024](../../docs/specs/SPEC-024-agent-installer-service.md) Amendment 2026-11-09, item 1), built from this workspace member with the workspace's existing Windows binding and no new third-party crate. It enables no privilege and does not adjust its token.

## What it holds today

- `inputs`: the bootstrap file's five admitted keys and their validation (item 2, §Data contracts §3).
- `staged`: the configuration and trust anchor the setup stages, generated with the fixed installed paths, the host name and a UUIDv7 placeholder, and checked by the service's own adoption checks (items 3 and 6.4).
- `record`: `setup-attempt.json`, its four keys and the checks of its form (SPEC-024 Amendment 2026-11-10, item 5).
- `staging`: `<P>` and the data root held and checked, what an earlier attempt left removed, the record, the staged anchor and configuration in that order, their removal in item 5's order, and the reversal, `reverse` (items 5 and 7).
- `service` and `scm`: the product service's configuration compared with the product's (26), `service_running`, the stop proved within 75 seconds (31), the failure actions with their flag set to false, and the start with its arguments (41), each taking the service's name (SPEC-024 Amendment 2026-11-10, items 3, 6 and 8). Their tests against real services run only in rust-ci's Windows job, with the test-only `scm-tests` feature.
- `observer`: configure's observer (item 8's outcome): the log's position recorded before the start, the agent's rotating log read after it without following a reparse point and without keeping rotation from renaming, and the service's state for 60 seconds from the start request; operational once it is `RUNNING` and `agent identity ready` and then `signed heartbeat sent` have appeared, 42 when it stops, 43 when it is still pending, 44 when it runs without both lines. Its tests against real services run with the SCM client's.
- `version`: the installed binary's version resource, the downgrade (29), and configure's check that it is this package's, 45 with a rename onto it held for the next restart and 40 otherwise (items 4 and 8).
- `preflight`: the host's state, read by `cg_agent::service::layout::classify` with the service's directory policy, the route each state and request allow, and the codes those checks reach, 22 to 25, 27, 28, 30, and 40 when the host cannot be read (items 3 and 7); `platform`: codes 20 and 21; `codes`: every exit code of item 7.

The wiring of preflight, configure and `reverse` into the executable and its verbs lands next (SPEC-024 Amendment 2026-11-10, items 8 and 9). Its tests that run it against the host run only in `package-ci`, never on a development host.

## Test

```sh
cargo test -p cg-setup
```
