# Governance State Reference

Sprint `0.04-S1` introduces governance state as a first-class Splendor kernel
primitive. The implementation is intentionally schema/state-machine only: it does
not add an approval UI, enterprise IAM integration, approval verifier,
escalation engine, circuit-breaker enforcement, or kill-switch propagation.

Governance state exists so later verifier/gateway work can pause, deny,
escalate, circuit-break, kill, or resume runtime work without inventing hidden
workflow state outside traceable kernel objects.

## Public Rust contract

Canonical Rust types live in `crates/splendor-types/src/governance.rs` and are
exported by `splendor-types`.

Schema version:

```text
splendor.governance_state.v1
```

Governance objects:

- `ApprovalRequest`
- `ApprovalGrant`
- `ApprovalDenial`
- `Escalation`
- `Intervention`
- `CircuitBreaker`
- `KillSwitch`

Supporting types:

- `GovernanceScope`
- `GovernanceIssuer`
- `GovernanceTraceLink`
- `GovernanceRevocation`
- `GovernanceObjectRef`
- `GovernanceTransition`
- `GovernanceTransitionRejection`
- status enums for approval, escalation, intervention, circuit breaker, and kill switch

## Required fields

Every governance object has:

| Field | Purpose |
| --- | --- |
| typed ID | `approval_id`, `escalation_id`, `intervention_id`, `circuit_breaker_id`, or `kill_switch_id` |
| `scope` | A typed governance scope; see below. |
| `status` | Explicit lifecycle state. |
| `created_at` | Creation timestamp. |
| `expires_at` | Optional expiry timestamp where applicable. If status is `expired`, this must be present. |
| `reason` | Sanitized human-readable reason. |
| `issuer` | `issuer_id` plus `source` attribution. |
| `trace` | `trace_event_id` and optional `run_id` linkage to the causal trace. |
| `revocation` | Optional explicit revocation marker. If status is `revoked`, this must be present. |
| `extensions` | Optional forward-compatible metadata that cannot carry authority. |

## Governance scopes

`GovernanceScope` is internally tagged with `scope_type` and supports:

| Scope | Required fields | Purpose |
| --- | --- | --- |
| `global` | none | Whole local governance domain. |
| `fleet` | `fleet_id` | Fleet-wide governance state. |
| `node` | `node_id` | Node-level governance state. |
| `instance` | `instance_id` | Runtime instance governance state. |
| `tenant` | `tenant_id` | Tenant-wide governance state. |
| `agent` | `tenant_id`, `agent_id` | Agent governance within a tenant. |
| `run` | `tenant_id`, `agent_id`, `run_id` | Run-specific governance. |
| `action` | `tenant_id`, `agent_id`, `run_id`, `action_id` | Action-specific governance. |
| `adapter` | optional `tenant_id`, `adapter` | Adapter-wide or tenant-scoped adapter governance. |

Scopes do not grant authority. They only declare the boundary a governance object
describes. Later verifiers consume this state and continue to fail closed through
the Action Gateway.

## Transition table

`GovernanceTransition::try_new` validates state changes before a transition can be
recorded in trace. Creation transitions use `from = None`.

| Object kind | Allowed transitions |
| --- | --- |
| approval | `None -> requested`; `requested -> granted`; `requested -> denied`; `requested -> expired`; `requested -> revoked`; `granted -> expired`; `granted -> revoked` |
| escalation | `None -> open`; `open -> resolved`; `open -> expired`; `open -> revoked` |
| intervention | `None -> requested`; `requested -> resolved`; `requested -> cancelled`; `requested -> expired`; `requested -> revoked` |
| circuit breaker | `None -> active`; `active -> cleared`; `active -> expired`; `active -> revoked` |
| kill switch | `None -> active`; `active -> cleared`; `active -> expired`; `active -> revoked` |

Invalid transitions return `GovernanceTransitionError::Rejected` containing a
`GovernanceTransitionRejection`. That rejection is trace-ready and should be
recorded with `TraceEventKind::GovernanceTransitionRejected` by runtime code that
attempts the transition.

## Trace events

Governance events are additive `TraceEventKind` variants. They use the same
append-only run trace stream and never execute side effects by themselves.

See [`trace-events.md#governance-events`](trace-events.md#governance-events) for
the complete event list. All governance events carry either:

- `transition: GovernanceTransition`, or
- `rejection: GovernanceTransitionRejection`.

When a governance scope includes tenant, agent, or action identity, trace event
identity context is populated with those IDs so replay/audit can locate the
affected boundary.

Runtime emitters that persist run/action-scoped governance events should use the
validating `TraceEvent::try_new_with_identity` path. It rejects governance scope
run IDs that do not match the enclosing trace stream. The infallible
`TraceEvent::new` constructor is intended for tests and callers that have already
validated this invariant.

## Extension fields

`extensions` is a JSON object map for future non-authoritative metadata. It is
validated recursively. The kernel rejects extension keys that attempt to carry
authority or overwrite runtime scope, including:

```text
scope, scope_type, tenant_id, agent_id, run_id, action_id, adapter,
allowed_actions, allowed_adapters, allowed_permissions, permissions, authority,
credential, work_order, signature, approval_token
```

This allows future UI hints or external references without permitting arbitrary
authority escalation through an extension document.

## Failure behavior

Governance state validation fails closed on:

- unsupported schema version;
- nil or missing typed IDs;
- malformed scope;
- blank reason, issuer, or source;
- missing trace linkage;
- run/action-scoped governance whose `scope.run_id` conflicts with
  `trace.run_id` when both are present;
- expiry not after creation;
- `expired` status without `expires_at`;
- `revoked` status without a `revocation` marker;
- revocation marker on a non-`revoked` object;
- invalid lifecycle transition;
- authoritative or reserved extension keys.

## Replay behavior

Replay can reconstruct governance state transitions from governance trace events
and can explain rejected transitions from `GovernanceTransitionRejected`. Replay
does not grant approvals, call verifiers, trip breakers, activate kill switches,
or execute adapters from these events.

## Minimal example

```rust
use splendor_types::{
    ApprovalId, ApprovalRequest, GovernanceExtensions, GovernanceIssuer,
    GovernanceScope, GovernanceTraceLink, RunId, TraceEventId,
};
use time::{Duration, OffsetDateTime};

let now = OffsetDateTime::now_utc();
let run_id = RunId::new();
let request = ApprovalRequest::new(
    ApprovalId::new(),
    GovernanceScope::Global,
    now,
    Some(now + Duration::minutes(30)),
    "publish requires approval",
    GovernanceIssuer::new("operator_cfo", "operator")?,
    GovernanceTraceLink::new(TraceEventId::from_run_sequence(&run_id, 7), Some(run_id)),
    GovernanceExtensions::new(),
)?;
assert_eq!(request.reason, "publish requires approval");
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility notes

This is an additive `0.04-dev` schema extension. Existing traces and runtime loops
remain valid. Consumers that exhaustively match `TraceEventKind` must add arms for
the governance event variants. The 0.1 stable schema line is not frozen yet.

The TypeScript package exposes schema-aligned inspection types only. Rust
validation remains authoritative for lifecycle transitions, reserved extension
keys, and scope/trace run consistency.

## Non-goals

- No approval UI.
- No approval verifier or pause/resume workflow.
- No escalation engine.
- No circuit-breaker enforcement.
- No kill-switch propagation.
- No external IAM/control-plane adapter.
- No side-effect execution path outside the Action Gateway.
