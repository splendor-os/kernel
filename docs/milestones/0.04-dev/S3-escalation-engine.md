# 0.04-S3 — Escalation engine

## Objective

Implement a small deterministic escalation engine that turns explicit verifier,
quota, timeout, policy-expiry, safety, or repeated-failure facts into traceable
deny/pause/intervention decisions without creating a general workflow engine.

## Functional scope

- Added escalation policy, rule, trigger, scope, decision, observation, and trace
  context schemas.
- Added a Rust `EscalationEvaluator` for deterministic first-match threshold
  evaluation.
- Added loop-engine integration behind explicit `set_escalation_policy`.
- Added `ActionStatus::NeedsIntervention`, `ActionNeedsIntervention`, and
  `EscalationTriggered` trace events.
- Added inspect-only replay reconstruction of escalation contexts.

## Non-goals

- No approval verifier, approval-token validation, or approval queue UI (0.04-S2).
- No circuit-breaker registry or propagation (0.04-S4).
- No central policy bundle distribution, cache, or revocation source (0.04-S5).
- No BPMN/workflow language, ticketing, notification platform, Harmony-specific
  dependency, or product UI.

## Public contracts changed

- Rust: `splendor_types::{EscalationPolicy, EscalationRule,
  EscalationTrigger, EscalationScope, EscalationDecision, EscalationObservation,
  EscalationContext}`.
- Rust: `splendor_gateway::ActionStatus::NeedsIntervention`.
- Rust trace: `TraceEventKind::EscalationTriggered` and
  `TraceEventKind::ActionNeedsIntervention`.
- TypeScript: `ActionStatus`, `TraceEventKind`, and escalation context types were
  updated for schema parity.
- OpenAPI: `ActionOutcome.status` includes `NeedsIntervention`.

## Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | status vocabulary extended; no bypass added |
| Verifier | uncertain/expired/quota/safety facts can escalate fail-closed |
| State graph | unchanged; state commits remain explicit |
| Trace store | added escalation and intervention events |
| Replay | reconstructs escalation facts inspect-only |
| Message | none |
| Work order | none |
| Governance | added S3 escalation policy/evaluator |

## Trace behavior

- New event classes: `escalation.triggered`, `action.needs_intervention`.
- `EscalationTriggered` is emitted after `ActionVerificationCompleted` and before
  the final action outcome event for the action.
- The escalation trace includes trigger, threshold, observed count, scope,
  decision, tenant/agent/run, optional action/adapter references, reason, and
  structured evidence.
- `OutcomeRecorded` includes an `escalations` array in the tick payload.

## State behavior

- Escalation does not introduce hidden state in the local loop.
- Repeated failure thresholds are evaluated from explicit observed counts supplied
  by the caller/runtime trace context, not from an implicit global counter.
- State commits continue through the existing state graph; state commit failure
  still prevents `StateCommitted` and `LoopTickCompleted`.

## Gateway and verifier behavior

- Escalation consumes existing gateway/verifier outcomes and does not execute
  adapters.
- Verifier uncertainty can be converted from a fail-closed denial into
  `NeedsIntervention`.
- Quota pressure reads denial artifacts and does not mutate quota ledgers.
- Policy expiry is consumed only as an explicit high-risk action fact.

## Replay behavior

- Replay reports escalation decisions and action intervention status from traces.
- Replay keeps `side_effects_replayed: false` for escalation facts.
- Replay does not retry adapters, request approvals, notify operators, open
  tickets, or create circuit breakers.

## Failure behavior

- Invalid escalation policy schema versions fail validation.
- Zero thresholds fail validation.
- No matching rule means the original gateway/verifier decision remains
  authoritative.
- Verifier uncertainty, quota pressure, policy expiry, and safety risks fail
  closed as denial/intervention decisions when rules match.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `cargo test -p splendor-types escalation` | schema validation, matching, trace fields | `crates/splendor-types/tests/unit/escalation_tests.rs` |
| `cargo test -p splendor-types trace` | escalation trace round-trip and action identity | `crates/splendor-types/tests/unit/trace_tests.rs` |
| `cargo test -p splendor-kernel escalation` | evaluator trigger behavior | `crates/splendor-kernel/tests/unit/escalation_tests.rs` |
| `cargo test -p splendor-kernel quota_pressure_escalates_without_consuming_denied_usage` | quota pressure intervention trace and ledger safety | `crates/splendor-kernel/tests/integration_loop_engine_quota_denial.rs` |

## Example or fixture

See `examples/escalation-basic/README.md`.

## Future extension notes

- 0.04-S2 can feed approval-timeout observations from first-class approval state.
- 0.04-S4 can consume escalation trace evidence when deciding whether to trip a
  circuit breaker, but S3 does not create breakers.
- 0.04-S5 can feed policy-expiry observations from policy bundle TTL and
  revocation state.
- 0.04-S7 can build richer audit exports from the existing escalation trace
  events without re-running side effects.
