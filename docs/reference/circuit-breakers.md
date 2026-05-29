# Circuit Breakers

Circuit breakers are explicit governance controls that stop matching work before
adapter execution. They are evaluated inside the Action Gateway verifier path and
fail closed when a tripped runtime-scoped breaker cannot be evaluated because the
runtime identity is missing.

This reference describes the 0.04-S4 local/config reference path. It does not add
an incident automation platform, dashboard, approval workflow, escalation engine,
policy TTL distribution, or kill-switch propagation.

## Functional requirement

- Milestone: `Splendor0.04-dev`
- Sprint: `0.04-S4 — Circuit breakers`
- FR: `FR-0.04-06`

## CircuitBreaker schema

Rust type: `splendor_types::CircuitBreaker`

```text
schema_version: "splendor.circuit_breaker.v1"
breaker_id: CircuitBreakerId
scope: CircuitBreakerScope
state: "tripped" | "cleared"
reason: string
created_at: timestamp
updated_at: timestamp
```

`CircuitBreakerId` is a UUID-backed typed control-object identifier. Local config
may use a non-empty operator label such as `cb_adapter_http`; `splendorctl` maps
that label deterministically to a `CircuitBreakerId` so breaker identity remains
distinct from tenant, agent, run, action, trace, node, instance, fleet, and other
runtime IDs.

## Supported scopes

`CircuitBreakerScope` supports:

| Scope | Match target |
| --- | --- |
| `global` | All work evaluated by the local evaluator. |
| `fleet` | `fleet_id` in runtime identity. |
| `node` | `node_id` in runtime identity. |
| `instance` | `instance_id` in runtime identity. |
| `tenant` | `tenant_id` on the action request. |
| `agent` | `agent_id` on the action request. |
| `adapter` | Resolved adapter ID after adapter registration lookup. |
| `action` | Action name. |
| `action_class` | Effective action side-effect class (`read_only`, `filesystem`, `network`, `external`, or `custom:<name>`). |

Fleet, node, and instance scoped breakers require the gateway runtime identity to
be populated. If a matching determination cannot be made because that identity is
missing, the breaker verifier denies with `circuit_breaker_scope_unknown` rather
than silently allowing side effects.

## Gateway behavior

`VerifiedActionGateway` now has a circuit-breaker evaluator. The default evaluator
has no breakers and allows work. The local config path installs
`StaticCircuitBreakerEvaluator` when `circuit_breakers` are present.

Evaluation order for an action is:

1. Validate action, tenant, agent, and run identity.
2. Resolve the registered adapter and reject adapter mismatches.
3. Normalize breaker evaluation to the effective side-effect class for known
   adapter IDs (`filesystem` and `http`) so action-class breakers cannot be
   bypassed by a caller-declared downgrade.
4. Evaluate tripped circuit breakers.
5. Reject declared/effective side-effect class mismatches for known adapters.
6. Evaluate tenant policy and preconditions.
7. Evaluate quota.
8. Execute the adapter only if all previous checks allow.
9. Evaluate postconditions.

Denied breaker outcomes use:

```json
{
  "allowed": false,
  "reasons": ["circuit_breaker_tripped"],
  "artifacts": {
    "circuit_breaker": {
      "breaker_id": "<uuid-derived-from-cb_adapter_http>",
      "scope": "adapter",
      "scope_value": "http",
      "state": "tripped",
      "reason": "adapter degraded"
    }
  }
}
```

The gateway wraps this under the verifier-chain source when aggregating results,
so replay readers must handle both the direct artifact and nested
`artifacts.circuit_breaker.circuit_breaker` form.

## Local config path

`splendorctl run` accepts optional `runtime_identity` and `circuit_breakers`
fields:

```yaml
runtime_identity:
  fleet_id: "11111111-1111-1111-1111-111111111111"
  node_id: "22222222-2222-2222-2222-222222222222"
  instance_id: "33333333-3333-3333-3333-333333333333"

circuit_breakers:
  - id: cb_filesystem_adapter
    scope: adapter
    value: filesystem
    state: tripped
    reason: filesystem disabled during incident
    authorized_by: operator:alice
```

`scope: global` does not require `value`. All other scopes require `value`.
Configured `state: cleared` breakers require explicit non-empty `authorized_by`
and do not block work. Configured `state: tripped` breakers block matching work;
when `authorized_by` is omitted for a tripped local config breaker, the trace
authority defaults to `local-config:circuit-breakers` because trips only remove
authority.

For local config actions, `side_effect_class` is fail-closed:

- known filesystem and HTTP adapters derive their effective classes from adapter
  identity (`filesystem` and `network` respectively);
- a declared class that conflicts with the adapter-derived class is rejected;
- unknown class strings are rejected unless they use the explicit `custom:<name>`
  form.

This prevents a side-effectful action from bypassing an `action_class` breaker by
omitting, misspelling, or downgrading its class to `read_only`.

Before registering local agents, `splendorctl run` evaluates global, fleet, node,
and instance breakers as a new-work admission check. Matching runtime-scope
breakers reject the run command before any adapter action is executed. Tenant,
agent, adapter, action, and action-class breakers are enforced per action inside
the gateway.

## Trace events

Circuit-breaker state changes are traceable with:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `CircuitBreakerTripped` | `circuit_breaker.tripped` | A breaker entered blocking state. |
| `CircuitBreakerCleared` | `circuit_breaker.cleared` | A breaker was explicitly reset/cleared. |

Both variants carry `CircuitBreakerTraceContext`:

```text
breaker_id: CircuitBreakerId
scope: CircuitBreakerScope
state: "tripped" | "cleared"
reason: string
authorized_by: string
recorded_at: timestamp
```

The `authorized_by` field is required by constructors for trip/clear trace
contexts. This records the local operator, service, or config authority that made
the explicit state change. It is not a replacement for daemon/client
authentication; authenticated management APIs remain future governance/control
plane work.

Action denials caused by breakers are still recorded as ordinary `ActionDenied`
events with breaker evidence in `VerificationResult.artifacts`.

## Replay behavior

Replay is inspect-only. It does not re-evaluate breakers, change breaker state,
re-submit actions, or invoke adapters.

When replay sees an `ActionDenied` event whose verification result includes
`circuit_breaker_tripped` or breaker artifacts, it emits
`circuit_breaker_denials` in replay output with:

```text
trace_event_id
action
reasons
breaker_id
scope
scope_value
reason
artifacts
```

This explains which breaker denied an action and at what scope without repeating
the denied side effect.

## Failure behavior

- Invalid or empty breaker labels are rejected by `CircuitBreakerId::try_new`.
- Empty reasons are rejected when building breaker objects or trace contexts.
- Missing `authorized_by` is rejected for explicit clear/reset trace contexts.
- Unknown or adapter-conflicting local action `side_effect_class` values are
  rejected instead of defaulting to read-only.
- Missing runtime `fleet_id`, `node_id`, or `instance_id` for a tripped
  runtime-scoped breaker fails closed with `circuit_breaker_scope_unknown`.
- A breaker denial returns `ActionStatus::Denied`; adapters are not called.

## Security notes

Circuit breakers do not grant permissions. They only remove authority by denying
matching work. They do not bypass tenant policy, quotas, approvals, signed work
orders, daemon caller authentication, or the Action Gateway. Side effects remain
gateway-mediated.

## Compatibility notes

`CircuitBreaker` and circuit-breaker trace variants are development contracts for
0.04-S4. They are schema-aligned in Rust and TypeScript, and may be incorporated
into the 0.1 stable primitive line after the governance milestone completes.
