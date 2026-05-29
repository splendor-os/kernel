# Governance Workflows

0.04-S2 implements the first governance workflow slice: approval-required actions
pause through the gateway verifier and resume only with valid scoped approval
evidence. This reference intentionally describes the implemented approval path and
marks later governance features as future work.

## Implemented in 0.04-S2

- Approval policies can mark scoped actions as approval-required.
- The action gateway returns `NeedsApproval` before adapter execution when
  evidence is missing.
- The daemon maps approval-required tick outcomes to `waiting_for_approval` and
  records a trace-linked pause.
- Resume from `waiting_for_approval` requires a signed resume work order and
  approval evidence.
- Valid grants allow the action to be re-evaluated by the gateway.
- Denial, expiry, revocation, wrong-scope evidence, or verifier uncertainty fails
  closed without adapter execution.
- Approval evidence must include action scope (`action_id` or `action_name`) and
  adapter scope for adapter-backed actions; omitted action/adapter scope is not a
  wildcard grant.
- Replay exposes approval lifecycle events without re-running verifiers or
  adapters.

## Explicit non-goals for this sprint

- No approval queue UI.
- No human notification system.
- No workflow DSL.
- No escalation engine.
- No circuit breakers.
- No kill-switch propagation.
- No policy TTL distribution beyond local approval-policy expiry checks.
- No product-specific Harmony dependency inside the kernel.

## Runtime loop impact

Approval is inserted into the existing loop at the verifier/gateway boundary:

```text
Percepts
  -> Policy proposes action
  -> Constraints evaluated
  -> Action Gateway
  -> Approval Verifier
  -> other verifier checks
  -> Adapter only if all verifiers allow
  -> Outcome
  -> State Commit
  -> Trace
```

The approval verifier does not execute side effects. It only classifies whether
the action may continue, must pause, is denied, or needs intervention.

## Run and action outcomes

Action statuses added for governance:

- `NeedsApproval`: approval is required and execution is paused.
- `NeedsIntervention`: a verifier/runtime boundary failed closed and operator or
  runtime intervention is required.

Daemon run statuses added for the approval path:

- `waiting_for_approval`: run paused after an approval-required action.
- `denied`: run reached a terminal denial due to approval denial, wrong scope, or
  revocation.
- `expired`: run reached a terminal expiry due to expired approval evidence.

## Approval lifecycle trace events

Governance decisions are trace-linked:

- `ActionNeedsApproval`
- `ApprovalRequested`
- `ApprovalGranted`
- `ApprovalDenied`
- `ApprovalExpired`
- `ApprovalRevoked`
- `RunPaused { reason: "waiting_for_approval" }`
- `RunResumed { reason }`

These events are ordered in the run trace and include approval/action/run identity
through `TraceIdentityContext` and `ApprovalTraceContext`.

## Replay and audit

Replay explains the approval path from trace records:

- why approval was requested;
- which approval grant, denial, expiry, or revocation was presented;
- the trace event ID and sequence for each approval lifecycle event;
- whether adapter execution remained suppressed until a grant was accepted.

Replay does not execute adapters, re-submit actions, re-deliver notifications,
call approval services, or mutate run state.

## Future governance work

Later 0.04 sprints will add broader governance primitives such as escalation
policies, circuit breakers, policy TTL distribution, kill-switch propagation, and
external control-plane adapters. Those features must build on this same verifier,
gateway, trace, and replay boundary rather than bypassing it.
