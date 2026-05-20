# CLAUDE.md — CyberGuard Enterprise

Project-level instructions for any Claude (or Claude-like) agent working on this repository. Read this file before editing code, ADRs, schemas, or workflows. The global `~/.claude/CLAUDE.md`, the foundational [Blueprint](docs/product/blueprint.md), the ADR catalog under [docs/adr/](docs/adr/), and the threat model at [docs/security/threat-model.md](docs/security/threat-model.md) all remain authoritative; this file is project-local additions.

## CI monitoring (mandatory after every push)

After every `git push` to any branch, the agent MUST verify the status of the GitHub Actions workflows triggered by the pushed SHA. The session is NOT closed, the task is NOT declared complete, and no follow-on work begins until every workflow for that SHA has reached a terminal state and the overall verdict is `ALL GREEN` — or until a red workflow has been explicitly downgraded to *Known CI debt* in chat by Manuel.

**Terminal states:** `success`, `failure`, `cancelled`, `timed_out`, `skipped`.

### Mechanism — primary (when `gh` CLI is available)

```sh
gh run list --commit <SHA> --json name,status,conclusion,databaseId
# For any run still in_progress or queued:
gh run view <id> --json status,conclusion
```

Poll any non-terminal run at approximately 10-second intervals. Hard timeout per workflow: 10 minutes. If a run exceeds the timeout, surface the run URL to Manuel and ask how to proceed — do not silently continue.

### Mechanism — fallback (when `gh` is not available in the agent environment)

Use the GitHub REST API directly, with a bearer token retrieved from the local git credential helper:

```sh
TOKEN=$(echo -e "protocol=https\nhost=github.com\n" | git credential fill \
  | grep '^password=' | sed 's/^password=//')
curl -s -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/AlmanInDaHouse/CyberGuard_Enterprise/actions/runs?head_sha=<SHA>"
unset TOKEN
```

Same poll cadence (≈10 s) and hard timeout (10 min per workflow). Never log the token value.

### Report format after every push

```
SHA <short-sha>
  <workflow-name-1> → <conclusion>     [run URL if failure]
  <workflow-name-2> → <conclusion>     [run URL if failure]
Verdict: ALL GREEN | FAILURES | IN PROGRESS
```

`IN PROGRESS` is acceptable only as an interim status during the poll loop. The final report after the poll must read either `ALL GREEN` or `FAILURES`.

### Hard rule on failures

If any workflow ends in `failure`, `cancelled`, or `timed_out`, the agent does NOT close the session, declare the task complete, or proceed to the next task until either:

1. The failure is diagnosed, fixed, and re-pushed until that workflow reports `success`, **or**
2. Manuel explicitly accepts the failure as known debt in chat, in which case the agent records it in the *Known CI debt* table below.

There is no third option. Assuming the failure is unrelated to the current change and ignoring it is not permitted.

`skipped` workflows (workflow not triggered by the path filters of the pushed commit) are reported as `skipped` and count toward `ALL GREEN`.

## Known CI debt

Workflows that are currently red on `main` and that Manuel has explicitly downgraded to accepted debt. The table is the live state, not a history — when a debt is cleared (workflow back to green), the row is removed in the same commit that clears it.

| Workflow | Declared on SHA | Reason | Owner | Target SHA / date |
|---|---|---|---|---|
| _none_ | — | — | — | — |

When adding an entry, also link to the relevant memory (e.g. `[[project-pending-...]]`) or the chat decision so the rationale is retrievable later.
