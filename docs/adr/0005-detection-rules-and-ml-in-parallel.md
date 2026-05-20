# ADR-0005: Detection — rules and ML in parallel

- Status: Accepted
- Date: 2026-05-20
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

Detection in a SOC / XDR platform can be performed with deterministic rules (Sigma-style pattern matching, stateful correlation engines) or with machine-learning models (anomaly detection, classifiers, LLM-based reasoning). The two approaches have complementary strengths. Rules are explainable, reproducible and auditable but brittle in the face of novel attacker behaviour. Models generalise across variants but are opaque, drift over time, and resist forensic reconstruction.

The product question is not which approach to pick. It is which approach gets to *do what*, and what guardrails govern automated action when either fires. Manuel and Claude (architecture) debated this explicitly during Blueprint design. An early draft framed detection as *"deterministic heuristic primary, AI only explains"*; that framing was explicitly superseded in favour of letting both detection paths produce alerts and discriminating instead by the reversibility of the action that may follow.

ADR-0001 placed `rules/`, `playbooks/`, `harness/scenarios/` and `services/ml/` in the layout. ADR-0002 fixed the languages of the surrounding services and confirmed via Rule 6 that detection rules and playbooks live as declarative YAML, not code. This ADR locks the detection philosophy itself.

The decision recorded here must:

1. Allow both rule-based and ML-based detection to produce alerts autonomously, without relegating ML to a permanently advisory role.
2. Pre-commit to an action policy that prevents catastrophic outcomes from any single signal, regardless of source.
3. Preserve the audit trail of every alert — which source fired, which input justified it.
4. Define how ML detections are validated, governed across model versions, and downgraded when they drift.
5. Define how the scoring of multiple signals composes.

## Decision

Rules and ML detect in parallel. Both can autonomously generate alerts. The differentiator is not *who detects* but *what action follows*: automatic actions scale by reversibility, with destructive actions always requiring human approval regardless of detection source.

### Detection source

Every alert carries a `detection_source` field:

```
detection_source: "rule" | "ml" | "hybrid"
```

`hybrid` is reserved for cases where a rule and an ML model produce alerts on the same incident within a configured correlation window.

### Action policy by reversibility

| Action class | Examples | Allowed detection sources |
|---|---|---|
| Reversible | Tag, notify, suppress, escalate severity, acknowledge | rule, ml, hybrid |
| Semi-reversible | Isolate endpoint, kill process | Requires a rule confirming, even if ML triggered |
| Destructive | Delete data, disable AD user, modify policies | Always requires explicit human approval |

A semi-reversible action triggered by an ML signal alone is held until a rule corroborates the signal or until human approval intervenes. A destructive action is never executed without a logged human approval, regardless of how confident the originating signal is.

### Playbook governance

Each SOAR playbook declares which detection sources it trusts:

```yaml
trust_sources: ["rule"]                # rule-only playbooks
trust_sources: ["rule", "ml"]          # either source
trust_sources: ["hybrid_only"]         # requires both within window
```

The field is configurable per organisation. Default `trust_sources` for shipped playbooks: `["rule"]` for semi-reversible actions; `["rule", "ml"]` for reversible actions; `["hybrid_only"]` is opt-in.

### Harness obligation

Every detection scenario in `harness/scenarios/` declares its expected `detection_source`. The harness validates both paths:

- For `detection_source: "rule"`, the relevant Sigma rule must fire.
- For `detection_source: "ml"`, the relevant ML model must score above its threshold.
- For `detection_source: "hybrid"`, both must fire within the correlation window.

CI blocks the merge of any new rule or ML model update that breaks an existing scenario.

New ML-generated alerts that appear in production without a corresponding harness scenario must be captured as scenarios within 30 days. If they are not, the model's autonomous alerting privilege is downgraded to *rule-gated only* until coverage is restored.

### ML model governance

- ML models live in `services/ml/` per ADR-0002 Rule 1.
- Models are versioned and pinned per deployment. No silent updates.
- Every model output that produces an alert must include a confidence score *and* the input feature snapshot that produced the score. Both are recorded with the alert for forensic reconstruction.
- A model that drifts — precision drops below its configured threshold over a 14-day rolling window — auto-downgrades to *advisory only*. It may continue to contribute to scoring but cannot generate alerts autonomously until manually re-promoted.

### Scoring composition

The final score of an alert combines the contributing signals:

```
final_score = w_rule · rule_score + w_ml · ml_score + w_ueba · ueba_score
where w_rule + w_ml + w_ueba = 1.0
```

Weights are configurable per organisation. Defaults are `w_rule = 0.6`, `w_ueba = 0.25`, `w_ml = 0.15`. The defaults are heuristic-heavy by design, in service of explainability and audit defensibility; organisations may rebalance via configuration.

### Out of scope

- The specific ML models, architectures or training data. Each model has its own SPEC.
- The Sigma-to-Go rule engine implementation. That is a SPEC of `services/pipeline/`.
- UEBA baseline computation. That is a SPEC that references this ADR for action policy.

## Alternatives considered

### A1 — Rules only; ML for explanation and summarisation only

Pros: maximum explainability, zero risk of model-generated false positives causing automated action.

Cons: leaves real detection capability on the table; modern attackers defeat static rules with cheap variants; ML offers generalisation that hand-maintained rules cannot match.

Rejected. This was the initial Blueprint framing, explicitly superseded by debate between Manuel and Claude (chat) during Blueprint review.

### A2 — ML primary, rules as fallback

Pros: maximum generalisation, fewer hand-maintained detection assets.

Cons: model outputs are not reproducible across versions; auditor defensibility is weak (*"the model decided"* is not an acceptable answer in a forensic context); adversarial robustness of detection models is an open research problem.

Rejected. Rules carry the audit trail that the product promises customers.

### A3 — Rules and ML detect, but ML never alerts autonomously (advisory only)

Pros: human-in-the-loop on every ML signal, low false-positive risk on automated actions.

Cons: caps the value of ML at *scoring contribution plus summarisation*; in practice the model never demonstrates novel detection capability, because no autonomous alert means no production validation data to feed back into the model lifecycle.

Rejected. Manuel pushed back on this framing: ML alerts autonomously, but the action policy by reversibility prevents catastrophic outcomes.

### A4 — Pure ensemble (single combined score, no source attribution)

Pros: simpler downstream — one score, one threshold.

Cons: loses the explainability signal that *"this fired because of rule X"* versus *"this fired because of model Y"*. Forensic and audit defensibility collapses.

Rejected. `detection_source` is non-negotiable.

## Consequences

### Positive

- ML detection capability is unlocked from day one. It is not deferred to some *"post-rules-mature"* phase that, in practice, never arrives.
- The audit trail is preserved: every alert names its detection source, and every source is testable in the harness.
- The action policy by reversibility is a defensible doctrine to customers and to auditors. *"We never let the AI delete your data autonomously"* is a sales argument and a true statement.
- ML model drift is an explicit lifecycle problem with an explicit auto-downgrade response, not an afterthought left to operations.
- Operators can tune trust per playbook and per organisation. There is no single global *"trust AI / do not trust AI"* toggle.

### Negative

- ML detection requires harness-obligation discipline. If new ML alerts in production are not captured as scenarios within 30 days, the privilege is downgraded. This is a real ops process, not just documentation.
- Two detection paths means two engines to operate, two failure modes to debug, and two performance profiles to monitor.
- The `hybrid_only` trust mode requires correlation-window tuning, which adds complexity to the correlation engine. The initial value is set in the SPEC of `cg-pipeline` and is revisable.
- Scoring weights are a per-organisation configuration surface that grows over time. Sensible defaults make this manageable, but exposing it without guardrails in the dashboard would be a footgun.

### Neutral

- The 14-day model-drift downgrade window is an initial setting; it is revisable via an ADR amendment if production data suggests a different value.
- Hybrid alerts (the same incident detected by both paths within the window) are treated as a single alert with elevated confidence, not two distinct alerts. The deduplication logic lives in the SPEC of `cg-pipeline`.

## Compliance

Subsequent ADRs, SPECs and harness scenarios that introduce new detection assets must reference this ADR. Specifically:

- Any new Sigma rule must declare `detection_source: "rule"` and have a paired scenario in `harness/scenarios/`.
- Any new ML model that emits alerts must declare its `detection_source: "ml"`, a confidence threshold, a 14-day drift threshold, and at least one harness scenario.
- Any new SOAR playbook must declare its `trust_sources` and must not perform a destructive action without an `approval` step.

Future ADRs that touch the pipeline (notably `ADR-0007` — Normalize before correlate) and the schema (notably `ADR-0006` — CGES alignment with OCSF) must honour the per-alert recording of `detection_source` and, for ML alerts, of the input feature snapshot and confidence score that justified them.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout (places `rules/`, `playbooks/`, `harness/`, `services/ml/`)
- [ADR-0002](0002-language-per-component.md) — Language per component (Rule 1 contains Python to `services/ml/`; Rule 6 treats `rules/` and `playbooks/` as configuration)
- Blueprint §9 — SIEM/XDR Pipeline (the *correlate* phase that consumes detection sources)
- Blueprint §11 — UEBA Strategy (references this ADR for the action policy)
- Onboarding §4 — Detection philosophy (locked framing)
