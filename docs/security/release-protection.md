# Release protection

The first public release waits for roadmap F2–F4 and Manuel's explicit
approval (ADR-0022 §7 and SPEC-023 §Operational §12). The repository's
`release.yml` runs on a pushed version tag; its comment alone cannot stop
an accidental publication. Debt #135 is addressed by the GitHub setting
below, chosen under Manuel's Session 48 delegation.

## Active pre-release rule

On 2026-10-27, the repository ruleset
[Pre-release version-tag freeze](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/rules/25923038)
was created and read back through the GitHub REST API:

| Field | Verified value |
|---|---|
| ID | `25923038` |
| Target | `tag` |
| Enforcement | `active` |
| Included refs | `refs/tags/v*` |
| Excluded refs | none |
| Rules | `creation`, `update`, `deletion` |
| Bypass actors | none |
| Current credential's bypass result | `never` |

GitHub's [ruleset API](https://docs.github.com/en/rest/repos/rules#create-a-repository-ruleset)
defines these three restrictions as allowing the operation only to actors
with bypass permission. An empty bypass list therefore freezes matching
tags, including the version tags that start this release workflow. The
match deliberately covers more than the workflow's numeric version pattern.
Branches, nonmatching archive tags and local release dry runs are unaffected.

This is a repository setting, not a file automatically enforced by cloning
this repository. Administrators able to edit rulesets can change or remove
it; it is not protection against a malicious repository administrator.
Verification checked the returned configuration and credential bypass
result. No release tag was pushed as a rejection probe, and no release was
published. At verification, the repository had only its two `archive/`
tags and no version tag.

## Publication remains a separate decision

Keep this rule active while F2–F4 and the pre-release work remain incomplete,
including third-party notices beyond the Node packages (#130). The manual
endpoint steps remain deploy_ac_021's elevated validation. The timed
installation runs from the published bundle with the first release
(deploy_ac_020). Once publication is authorized and artifacts exist,
positively verify their provenance as deploy_ac_019 requires.
The freeze is not evidence that any of those gates passed.

After Manuel explicitly approves the release, the publishing change must
record the approved version and commit, re-read this rule, and define the
smallest deliberate change needed to permit that publication. Do not add a
standing bypass or disable it as a side effect of normal development. Keep
tag update/deletion protection in the eventual release policy. No release
permission is granted by this document.

The alternative considered was a protected release environment with a
required reviewer. The tag freeze was selected because it stops the
workflow before publication work starts and does not depend on a second
eligible reviewer. A future release policy can add environment approval;
the current choice does not amend an ADR.
