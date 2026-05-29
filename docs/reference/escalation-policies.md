# Escalation Policies Reference

Sprint 0.04-S3 adds a small deterministic escalation engine for governed local
runtime decisions. It consumes explicit verifier/runtime facts and emits traceable
decisions when configured thresholds are reached.

## Purpose

Escalation policies make these situations fail closed and explainable:

- verifier uncertainty or unavailability;
- repeated adapter failure or denial;
- approval timeout facts produced by the approval layer;
- quota pressure without spending denied quota;
- expired policy facts for high-risk actions;
- safety-risk facts.

The engine does **not** implement an approval workflow, circuit-breaker registry,
central policy distribution, ticketing, notifications, or UI.

## Schema

Rust source of truth:

```rust
pub const ESCALATION_POLICY_SCHEMA_VERSION: &str = "splendor.escalation_policy.v1";

pub struct EscalationPolicy {
    pub schema_version: String,
    pub rules: Vec<EscalationRule>,
}

pub struct EscalationRule {
    pub trigger: EscalationTrigger,
    pub scope: EscalationScope,
    pub threshold: u32,
    pub decision: EscalationDecision,
    pub reason: Option<String>,
}
```

Triggers:

```text
VerifierUncertainty
RepeatedAdapterFailure
ApprovalTimeout
QuotaPressure
PolicyExpired
SafetyRisk
```

Decisions:

```text
NoAction
Deny
Pause
NeedsIntervention
```

Scopes:

```text
Tenant
Agent
Run
Action
Adapter
```

`threshold` must be greater than zero. The evaluator uses ordered rules and the
first matching trigger/scope whose observed count reaches the threshold.

## Runtime lifecycle

1. A verifier/gateway/runtime path produces an explicit `ActionOutcome` or
   `EscalationObservation` with trigger evidence.
2. `EscalationEvaluator` checks the observation against `EscalationPolicy`.
3. If a threshold is reached, the engine creates `EscalationContext`.
4. The loop records `EscalationTriggered` with trigger, threshold, scope,
   action/run references, evidence, and decision.
5. If the decision requires intervention, the action outcome is marked
   `NeedsIntervention` and the tick `needs_intervention` flag is set.

Adapter execution still depends on the Action Gateway. Escalation never creates a
side-effect path around the gateway.

## Trace events

New trace event classes:

- `escalation.triggered` (`TraceEventKind::EscalationTriggered`)
- `action.needs_intervention` (`TraceEventKind::ActionNeedsIntervention`)

`EscalationContext` contains:

| Field | Description |
| --- | --- |
| `trigger` | Trigger category. |
| `threshold` | Configured rule threshold. |
| `observed_count` | Observed occurrences at the selected scope. |
| `scope` | Tenant, agent, run, action, or adapter. |
| `decision` | NoAction, Deny, Pause, or NeedsIntervention. |
| `tenant_id`, `agent_id`, `run_id` | Authority and run scope. |
| `action_id`, `action_name`, `adapter` | Action/adapter reference when applicable. |
| `reason` | Stable reason code or summary. |
| `evidence` | Structured verifier/runtime evidence. |
| `decided_at` | Decision timestamp. |

## Failure modes

- Invalid policy schema version is rejected by `EscalationPolicy::validate`.
- Zero thresholds are rejected by `EscalationPolicy::validate`.
- Missing matching rule means no escalation; the original gateway/verifier outcome
  remains authoritative.
- Verifier uncertainty should be supplied as a fail-closed denial or intervention
  fact. It must not be converted into allow.
- Quota pressure is read from quota denial artifacts and does not mutate the quota
  ledger.
- Policy expiry is consumed only as an explicit fact for high-risk actions. 0.04-S5
  owns policy bundle TTL sources and distribution.

## Replay behavior

Replay is inspect-only. It reconstructs escalation contexts and
`action.needs_intervention` statuses from trace events with
`side_effects_replayed: false`. Replay does not re-run verifiers, retry adapters,
request approvals, notify operators, or create tickets.

## Minimal example

```rust
use splendor_kernel::{EscalationDecision, EscalationPolicy, EscalationRule,
    EscalationScope, EscalationTrigger, LoopEngine};

let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
    EscalationTrigger::QuotaPressure,
    EscalationScope::Action,
    1,
    EscalationDecision::Pause,
)]);

// Existing local loop setup omitted.
// engine.set_escalation_policy(policy);
```

See [`examples/escalation-basic/README.md`](../../examples/escalation-basic/README.md)
for a runnable test command and expected trace behavior.

## Compatibility notes

0.04-S3 adds a public `ActionStatus::NeedsIntervention` variant and two trace
event variants. Existing `Executed`, `Denied`, and `Failed` semantics are
unchanged. Consumers that exhaustively match action statuses or trace kinds must
handle the new variants.
