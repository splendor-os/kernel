# Escalation Basic Example

This example documents the 0.04-S3 reference path for local escalation policies.
It does not start an approval queue, ticketing system, notification service,
circuit breaker, or central policy distributor.

## What it proves

- A local loop can configure an `EscalationPolicy`.
- Quota pressure can produce a `NeedsIntervention` action outcome without
  executing the adapter.
- Denied quota usage is not consumed by the quota ledger.
- Trace includes `EscalationTriggered` with trigger, threshold, scope, run/action
  reference, and decision.
- Replay remains inspect-only and never replays side effects.

## Run the focused check

```bash
cargo test -p splendor-kernel quota_pressure_escalates_without_consuming_denied_usage
```

## Policy shape

```rust
use splendor_kernel::{
    EscalationDecision, EscalationPolicy, EscalationRule,
    EscalationScope, EscalationTrigger,
};

let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
    EscalationTrigger::QuotaPressure,
    EscalationScope::Action,
    1,
    EscalationDecision::Pause,
)]);
```

The local loop uses `LoopEngine::set_escalation_policy(policy)` to enable the
evaluator. Without this explicit call, escalation is disabled and existing
gateway outcomes remain unchanged.

## Expected trace behavior

The focused test emits the normal tick sequence plus:

```text
verification.completed
escalation.triggered
action.needs_intervention
outcome.recorded
```

The `escalation.triggered` event contains:

- `trigger = QuotaPressure`
- `threshold = 1`
- `scope = Action`
- `decision = Pause`
- `tenant_id`, `agent_id`, and `run_id`
- `action_id` and `action_name`
- quota denial evidence from the verifier/gateway path

## Non-goals

- No approval grant/denial flow.
- No reusable circuit breaker.
- No policy TTL distribution or revocation cache.
- No external side effects outside the Action Gateway.
