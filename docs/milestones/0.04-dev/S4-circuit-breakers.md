# 0.04-S4 — Circuit Breakers

## Objective

Implement the 0.04-S4 circuit-breaker governance primitive so tripped breakers
deny matching work before adapter execution, preserve scoped identity, emit
traceable trip/clear state changes, and allow replay to explain breaker denials
without side effects.

## Functional scope

- Defines `CircuitBreaker`, `CircuitBreakerScope`, `CircuitBreakerState`, and
  `CircuitBreakerTraceContext` schemas in `splendor-types`.
- Adds a gateway circuit-breaker evaluator with a default allow-empty evaluator
  and a local static evaluator for configured breakers.
- Enforces global, fleet, node, instance, tenant, agent, adapter, action, and
  action-class scopes.
- Adds `splendorctl run` local config support for `runtime_identity` and
  `circuit_breakers`.
- Adds replay extraction for breaker-denied actions.

## Non-goals

- No automated incident response system.
- No UI dashboard.
- No predictive safety model.
- No approval verifier or approval pause/resume flow from 0.04-S2.
- No escalation engine from 0.04-S3.
- No central policy TTL/distribution from 0.04-S5.
- No full daemon breaker-management API; this sprint uses a local config path and
  traceable typed state-change objects.

## Public contracts changed

- Rust:
  - `splendor_types::{CircuitBreaker, CircuitBreakerId, CircuitBreakerScope,
    CircuitBreakerState, CircuitBreakerTraceContext, CircuitBreakerMatch}`.
  - `splendor_types::TraceEventKind::{CircuitBreakerTripped,
    CircuitBreakerCleared}`.
  - `splendor_gateway::{CircuitBreakerEvaluator,
    StaticCircuitBreakerEvaluator, NoopCircuitBreakerEvaluator}`.
  - `VerifiedActionGateway::set_circuit_breaker_evaluator`,
    `set_runtime_identity`, and `verify_runtime_admission`.
- TypeScript:
  - `CircuitBreakerId`, `CircuitBreakerScope`, `CircuitBreakerState`,
    `CircuitBreaker`, and `CircuitBreakerTraceContext`.
  - Trace event variants `CircuitBreakerTripped` and `CircuitBreakerCleared`.
- CLI config:
  - Optional `runtime_identity` and `circuit_breakers` fields for `splendorctl run`.

## Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | added breaker verifier before adapter execution |
| Verifier | added circuit-breaker evaluator |
| State graph | unchanged; breaker state is a typed governance control object, not hidden agent state |
| Trace store | added trip/clear trace variants and breaker denial artifacts |
| Replay | added inspect-only breaker denial reconstruction |
| Message | none |
| Work order | unchanged |
| Governance | added scoped circuit breakers |

## Trace behavior

New event variants:

- `CircuitBreakerTripped` (`circuit_breaker.tripped`)
- `CircuitBreakerCleared` (`circuit_breaker.cleared`)

Both carry breaker ID, scope, state, reason, authority attribution, and timestamp.
Action denials caused by breakers are emitted through the existing `ActionDenied`
event with `VerificationResult.reasons = ["circuit_breaker_tripped"]` and scoped
breaker artifacts.

## State behavior

No agent state graph format changes were made. Breaker control objects are
explicit serialized governance state. The local config path treats configured
breakers as input authority objects; it does not introduce hidden mutable agent
state or mutate state graph heads.

## Verifier/gateway behavior

- Breaker evaluation occurs after adapter ID resolution and before tenant policy,
  quota, and adapter execution.
- Action-class breaker evaluation uses the effective side-effect class for known
  adapter IDs (`filesystem` and `http`) rather than trusting a caller-declared
  downgrade.
- Adapter, tenant, agent, action, and action-class breakers deny matching action
  requests.
- In the local config reference path, tenant and agent breakers are enforced at
  the gateway action boundary inside runs; they do not reject run creation before
  an action exists.
- Global, fleet, node, and instance breakers are also checked as local new-work
  admission before `splendorctl run` registers agents.
- Missing fleet/node/instance identity for tripped runtime-scoped breakers fails
  closed with `circuit_breaker_scope_unknown`.
- Denied breaker actions return `ActionStatus::Denied`; adapters are not called.

## Replay behavior

Replay remains inspect-only. It does not re-evaluate breakers, clear breakers,
route actions, or execute adapters. It reconstructs breaker denials from stored
`ActionDenied` verification results and emits `circuit_breaker_denials` in tick
and causal-graph replay output.

## Failure behavior

- Empty breaker IDs fail validation.
- Empty breaker reasons fail validation.
- Trip/clear trace contexts require non-empty `authorized_by`.
- Unsupported local config scopes/states fail config loading.
- Runtime-scoped breaker uncertainty denies instead of allowing.
- Adapter execution count remains zero for breaker-denied actions.
- Local config action classes fail closed: unknown class strings and
  adapter-conflicting downgrades are rejected before adapter execution.
- Cleared local config breakers require explicit non-empty `authorized_by` rather
  than default authority attribution.

## Test evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `cargo test -p splendor-types` | Schema and trace serialization | `CircuitBreaker` round trips; trip/clear trace events round trip; reset requires authority. |
| `cargo test -p splendor-gateway` | Gateway denial and scope matching | Adapter/tenant/action-class/node/instance tests prove scoped denial and no adapter execution. |
| `cargo test -p splendorctl` | Local config + replay | Configured breaker denies filesystem action; replay reports breaker ID/scope. |

## Example or fixture

- `examples/circuit-breaker-basic/README.md`

## Future extension notes

- 0.04-S3 escalation can trip breakers by creating the same typed
  `CircuitBreaker` control objects and trace events.
- 0.04-S5 policy distribution can deliver breaker bundles, but the gateway
  evaluator contract stays the same.
- Future daemon/control-plane APIs can authorize breaker trip/reset operations and
  emit the existing `CircuitBreakerTripped` / `CircuitBreakerCleared` trace
  variants without changing the action denial artifact shape.
