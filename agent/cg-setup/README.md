# cg-setup

The setup's elevated code, `cg-setup.exe` ([SPEC-024](../../docs/specs/SPEC-024-agent-installer-service.md) Amendment 2026-11-09, item 1), built from this workspace member with the workspace's existing Windows binding and no new third-party crate. It enables no privilege and does not adjust its token.

## What it holds today

- `inputs`: the bootstrap file's five admitted keys and their validation (item 2, §Data contracts §3).
- `staged`: the configuration and trust anchor the setup stages, generated with the fixed installed paths, the host name and a UUIDv7 placeholder, and checked by the service's own adoption checks (items 3 and 6.4).
- `preflight`: the host's state, read by `cg_agent::service::layout::classify` with the service's directory policy, the route each state and request allow, and the codes those checks reach, 22 to 25, 27, 28, 30, and 40 when the host cannot be read (items 3 and 7); `platform`: codes 20 and 21; `codes`: every exit code of item 7.

The executable, its verbs, the staging, the SCM client and the observer land next (SPEC-024 Amendment 2026-11-09, items 3, 5, 7 and 8). Its tests that run it against the host run only in `package-ci`, never on a development host.

## Test

```sh
cargo test -p cg-setup
```
