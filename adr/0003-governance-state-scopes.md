# ADR 0003: Governance State Scopes

## Status

Accepted for Splendor0.04-dev Sprint 0.04-S1.

## Context

Splendor 0.04 introduces governance workflows. Before implementing approval
verifiers, escalation engines, circuit-breaker enforcement, kill-switch
propagation, policy TTL enforcement, or external control-plane adapters, the
kernel needs a stable governance state model that is explicit, trace-linked, and
scope-safe.

The sprint criteria require:

- governance schemas for approvals, escalations, interventions, circuit breakers,
  and kill switches;
- stable identity, scope, created time, expiry where applicable, reason,
  issuer/source, and trace linkage on every governance object;
- invalid transitions rejected and traceable;
- scope support for tenant, agent, run, action, adapter, node, instance, fleet,
  and global boundaries;
- explicit expiry and revocation representation;
- extension fields that do not create authority escalation.

## Decision

Define governance state in `splendor-types` as additive primitive schemas:

- typed IDs: `ApprovalId`, `EscalationId`, `InterventionId`,
  `CircuitBreakerId`, and `KillSwitchId`;
- object schemas: `ApprovalRequest`, `ApprovalGrant`, `ApprovalDenial`,
  `Escalation`, `Intervention`, `CircuitBreaker`, and `KillSwitch`;
- `GovernanceScope`, an internally tagged enum with global, fleet, node,
  instance, tenant, agent, run, action, and adapter variants;
- `GovernanceIssuer` carrying `issuer_id` and `source` attribution;
- `GovernanceTraceLink` carrying causal `trace_event_id` and optional `run_id`;
- `GovernanceRevocation` as an explicit revocation marker;
- `GovernanceTransition` and `GovernanceTransitionRejection` as trace-ready
  lifecycle records.

Governance trace events are run-scoped `TraceEventKind` variants that carry the
transition or rejection payload. Scope identity is copied into the trace identity
context where possible, but the event does not execute, approve, deny, pause,
resume, or propagate anything by itself.

## Consequences

- Governance state is explicit data, not hidden UI/workflow state.
- Future verifier/gateway sprints can consume the same scope model without
  redefining tenant, agent, run, action, adapter, node, instance, fleet, or global
  boundaries.
- Invalid lifecycle transitions fail closed and produce a structured rejection
  payload suitable for trace/audit/replay.
- TypeScript clients can inspect governance state without owning kernel runtime
  semantics.
- Extension fields can carry non-authoritative metadata, but recursive validation
  rejects fields such as `allowed_permissions`, `authority`, `work_order`,
  `signature`, and `approval_token`.

## Non-goals

- No approval UI or enterprise approval queue.
- No enterprise org/IAM integration.
- No approval verifier or pause/resume runtime flow.
- No escalation engine.
- No circuit-breaker enforcement.
- No kill-switch propagation.
- No external Harmony/control-plane adapter.
- No side-effect path outside the Action Gateway.

## Compatibility notes

This is an additive `0.04-dev` schema and trace taxonomy extension. Existing
runtime traces remain valid. Consumers that exhaustively match `TraceEventKind`
must add arms for governance variants. The stable `0.1` schema line is not yet
frozen, so this ADR records the compatibility note required for a public
governance schema addition.

## Alternatives considered

1. **Single untyped JSON governance record.** Rejected because it would hide
   identity/scope rules and make authority escalation through metadata easier.
2. **Enterprise workflow model in the kernel.** Rejected as out of scope for the
   runtime primitive layer and explicitly forbidden by the sprint non-goals.
3. **Separate scope enums per governance object.** Rejected because later
   verifiers need one consistent scope contract for approvals, escalations,
   circuit breakers, kill switches, and interventions.
