# 0.04-S1 — Governance state model

## Objective

Define approval, escalation, intervention, circuit-breaker, and kill-switch
states as explicit, versioned, traceable governance objects. This strengthens the
`governance` and `trace store` primitives without implementing the later
approval verifier, escalation engine, circuit-breaker enforcement, kill-switch
propagation, or external control-plane adapter.

## Functional scope

- Added canonical `splendor-types` governance schemas:
  `ApprovalRequest`, `ApprovalGrant`, `ApprovalDenial`, `Escalation`,
  `Intervention`, `CircuitBreaker`, and `KillSwitch`.
- Added typed IDs for governance lifecycle objects:
  `ApprovalId`, `EscalationId`, `InterventionId`, `CircuitBreakerId`, and
  `KillSwitchId`.
- Added `GovernanceScope` for global, fleet, node, instance, tenant, agent, run,
  action, and adapter scopes.
- Added explicit expiry and revocation representation.
- Added `GovernanceTransition` and `GovernanceTransitionRejection` with a narrow
  transition table.
- Added governance trace event variants and TypeScript schema surface parity.

## Non-goals

- No approval UI.
- No enterprise org model or IAM integration.
- No approval verifier or pause/resume runtime flow.
- No escalation engine.
- No circuit-breaker enforcement.
- No kill-switch propagation.
- No external control-plane/Harmony adapter.
- No side-effectful action path outside the Action Gateway.

## Public contracts changed

- Rust `splendor-types` exports governance IDs, schemas, status enums, scope,
  issuer/source attribution, trace links, transition records, and validation
  errors.
- `TraceEventKind` adds governance event variants for approval, escalation,
  intervention, circuit breaker, kill switch, and rejected transitions.
- `@splendor/types` adds schema-aligned governance TypeScript types and extends
  `TRACE_EVENT_KIND_VARIANTS`.
- New docs:
  - `docs/reference/governance-state.md`
  - `adr/0003-governance-state-scopes.md`
- Updated docs:
  - `docs/reference/trace-events.md#governance-events`

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | unchanged |
| Policy | unchanged |
| Gateway | no new execution behavior; schemas are designed for later verifier/gateway consumption |
| Verifier | no verifier implementation yet; invalid schema/transition validation fails closed in type layer |
| State graph | no state node format change; governance state is explicit schema data and trace payload |
| Trace store | added governance transition and rejection event variants |
| Replay | governance events are reconstructable; no side effects are replayed |
| Message | unchanged |
| Work order | unchanged |
| Governance | added first-class object model, scopes, transitions, expiry/revocation |

## Trace behavior

- Added transition events:
  `GovernanceApprovalRequested`, `GovernanceApprovalGranted`,
  `GovernanceApprovalDenied`, `GovernanceApprovalExpired`,
  `GovernanceApprovalRevoked`, `EscalationOpened`, `EscalationResolved`,
  `EscalationExpired`, `EscalationRevoked`, `InterventionRequested`,
  `InterventionResolved`, `InterventionCancelled`, `InterventionExpired`,
  `InterventionRevoked`, `CircuitBreakerTripped`, `CircuitBreakerCleared`,
  `CircuitBreakerExpired`, `CircuitBreakerRevoked`, `KillSwitchActivated`,
  `KillSwitchCleared`, `KillSwitchExpired`, and `KillSwitchRevoked`.
- Added fail-closed rejection event:
  `GovernanceTransitionRejected`.
- Governance trace events populate trace identity context with tenant, agent, and
  action identity when those are present in `GovernanceScope`.
- Governance events do not replace required tick ordering and do not authorize
  side effects.

## State behavior

- No state graph storage format changed.
- Governance objects are explicit, versioned schemas with typed IDs, scope,
  status, `created_at`, optional `expires_at`, reason, issuer/source, trace
  linkage, optional revocation, and non-authoritative extensions.
- Invalid governance object shape or lifecycle markers are rejected before the
  object can be treated as valid state.

## Gateway and verifier behavior

- No side-effectful gateway path was added.
- No approval verifier or circuit-breaker verifier was implemented in this
  sprint.
- The type-layer transition validator rejects invalid governance lifecycle
  changes and returns a trace-ready rejection payload.
- Future verifier/gateway sprints can consume `GovernanceScope` without broadening
  permissions or adding enterprise UI assumptions.

## Replay behavior

- Replay can decode governance trace events and reconstruct governance lifecycle
  transitions and rejected attempts.
- Replay does not grant approvals, call verifiers, trip circuit breakers,
  activate kill switches, pause/resume runs, or execute adapters.
- Rejected transitions remain audit evidence only and never become implicit
  allows.

## Failure behavior

- Unsupported schema version fails validation.
- Nil/blank identities, scope fields, issuer, source, reason, or trace linkage
  fail validation.
- `expires_at <= created_at` fails validation.
- Run/action-scoped governance transitions fail validation when `scope.run_id`
  and `trace.run_id` disagree.
- `expired` without `expires_at` fails validation.
- `revoked` without explicit `GovernanceRevocation` fails validation.
- Revocation markers on non-revoked objects fail validation.
- Invalid lifecycle transitions return `GovernanceTransitionError::Rejected`.
- Extension keys that attempt to carry authority (`allowed_permissions`,
  `work_order`, `signature`, `approval_token`, etc.) fail validation recursively.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | Governance schemas require identity, scope, time, reason, issuer/source, trace linkage and round-trip | `cargo test -p splendor-types governance_objects_validate_required_fields_and_round_trip` |
| unit | Scope model covers global/fleet/node/instance/tenant/agent/run/action/adapter | `cargo test -p splendor-types governance_scope_covers_required_boundaries` |
| unit | Transition table allows intended creation and terminal transitions | `cargo test -p splendor-types transition_table_accepts_expected_governance_paths` |
| negative/trace | Invalid transition rejects and can be recorded as trace event | `cargo test -p splendor-types invalid_transition_is_rejected_and_traceable` |
| negative | Expiry/revocation markers are explicit and validated | `cargo test -p splendor-types expiry_and_revocation_are_explicit` |
| negative/trace | Run-scope and trace-link run mismatches are rejected | `cargo test -p splendor-types run_scoped_governance_transition_rejects_trace_run_mismatch`; `cargo test -p splendor-types governance_trace_event_rejects_scope_run_mismatch_with_explicit_identity` |
| schema | Extensions allow future metadata but reject authority escalation | `cargo test -p splendor-types extensions_are_forward_compatible_but_non_authoritative` |
| trace/schema parity | Governance trace variants round-trip and TypeScript variant list stays aligned | `cargo test -p splendor-types governance_trace_events_round_trip_and_apply_scope_identity`; `npm test --workspaces` |

## Example or fixture

- Reference example in `docs/reference/governance-state.md#minimal-example`.
- Executable fixtures are the `splendor-types` governance and trace unit tests
  listed above.

## Future extension notes

- 0.04-S2 can consume `ApprovalRequest`/`ApprovalGrant`/`ApprovalDenial` in an
  approval verifier without changing the scope model.
- 0.04-S3 can use `Escalation` and `Intervention` as deterministic escalation
  state without introducing an enterprise workflow engine.
- 0.04-S4 can enforce `CircuitBreaker` scopes in the gateway/verifier path.
- 0.04-S5 can add policy TTL state while preserving the governance trace pattern.
- 0.04-S6 can bridge an external control plane by issuing these schemas rather
  than owning kernel state.
