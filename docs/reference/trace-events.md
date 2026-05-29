# Trace Events Reference

Trace events form the append-only audit log for each run. Events are ordered by
sequence number within a `RunId` and must be emitted in strict tick order.

## TraceEvent

**Fields**

- `trace_event_id` (`TraceEventId`): deterministic identifier derived from `RunId` + sequence. Deserialization accepts legacy `trace_id` as an input alias during the 0.02 migration window.
- `run_id` (`RunId`): owning run.
- `sequence` (`u64`): monotonic per-run sequence number.
- `timestamp` (`OffsetDateTime`): capture time at emission.
- `identity` (`TraceIdentityContext`): runtime identity context containing required `run_id` and optional fleet, node, instance, tenant, agent, tick, action, approval, state, and message IDs when applicable.
- `kind` (`TraceEventKind`): event payload.

Trace events are serialized into `TraceRecord` entries within a `TraceStore`.
The store records additional integrity hashes for audit validation.

## Ordering Rules

When a new persisted local run trace stream is created, `RunStarted` is emitted
before the first tick. If the run was authorized by a validated 0.03-S3 work
order, `WorkOrderAccepted` follows `RunStarted` and precedes the first tick. If
work-order validation fails, `WorkOrderRejected` is emitted as a management/audit
trace and no tick events are emitted. Events MUST then be emitted in the
following order for each tick:

1. `LoopTickStarted`
2. `PerceptsReceived`
3. `StateLoaded`
4. `PolicyInvoked`
5. `PolicyCompleted`
6. `CandidatesProposed`
7. `ConstraintsEvaluated`
8. `ActionVerificationStarted`
9. `ActionVerificationCompleted`
10. Optional `EscalationTriggered` when an escalation threshold is reached
11. `ActionNeedsApproval`, `ActionNeedsIntervention`, `ActionExecuted`, `ActionDenied`, or `ActionFailed`
12. `OutcomeRecorded`
13. `StateCommitted`
14. `LoopTickCompleted`

These Rust enum names correspond to the canonical runtime event classes used in
the rule documents: `run.started`, `tick.started`, `percepts.received`, `state.loaded`,
`policy.invoked`, `policy.completed`, `actions.proposed`,
`constraints.evaluated`, `verification.started`, `verification.completed`,
`escalation.triggered` when applicable, `action.needs_approval`,
`action.needs_intervention`, `action.executed`, `action.denied`, or
`action.failed`, `outcome.recorded`, `state.committed`, and `tick.completed`.

If post-verification fails after an action executes, the kernel records
`ActionExecuted` followed by `ActionFailed` with the post-verification result.

Message lifecycle events are also trace events. They are ordered by the same
per-run sequence counter and do not replace the required tick event ordering.
The local message router emits them when a message is queued, delivered,
rejected, expired, or consumed. Remote message transport emits additional
cross-instance message events for send, accept, reject, deliver, timeout,
duplicate, and transport failure.

## Identity context

Every trace event carries `identity.run_id`; runtime emission validates that it
matches the top-level `run_id` before persistence. Loop events emitted by the
kernel include tenant, agent, run, and tick identity where applicable. Action
events include `action_id`; approval lifecycle events include `approval_id`; state
commit events include `state_node_id`; message lifecycle events include
`message_id`. Fleet, node, and instance IDs are optional until later 0.03
registry and transport sprints populate them.

Node and instance registry lifecycle events introduced in 0.03-S2 are management
audit events rather than run-scoped `TraceEventKind` variants. They are documented
in [`node-registry.md`](node-registry.md) and remain suitable for later
aggregation without inventing fake run IDs.

State handoff events are trace events too. They identify source and receiver
handoff boundaries and preserve the previous receiver state head for replay.

Local delegation events are emitted outside the tick ordering when a parent run
creates, completes, fails, cancels, or rejects local child work. They are ordered
by each affected run's sequence counter and carry explicit parent/child IDs.

Governance events introduced in 0.04-S1 are emitted outside the tick body when
approval, escalation, intervention, circuit-breaker, or kill-switch state changes
or when an invalid governance transition is rejected. They are ordered by the
same per-run sequence counter and carry explicit governance scope plus issuer and
trace linkage. These events model state only; they do not grant approvals,
execute adapters, trip live breakers, or propagate kill switches by themselves.

0.02-S5 daemon lifecycle events are emitted outside the tick body but through the
same run trace runtime, preserving monotonic sequence order before the next tick
or replay inspection. Mutating daemon calls also emit `DaemonAudit` before the
mutation so caller attribution is persisted in the run trace.

0.04-S2 approval lifecycle events are emitted by the gateway/daemon approval path
when an action requires approval, a grant is presented, evidence is denied,
evidence is expired, or evidence is revoked. They are ordered in the same run
trace and do not authorize adapter execution outside the gateway.

## TraceEventKind Payloads

- `RunStarted`
- `WorkOrderAccepted { work_order_id, tenant_id, agent_id, run_id }`
- `WorkOrderRejected { work_order_id: Option<WorkOrderId>, tenant_id: Option<TenantId>, agent_id: Option<AgentId>, run_id: Option<RunId>, reason: String }`
- `PolicyBundleAccepted { bundle: PolicyBundleTraceContext }`
- `PolicyBundleRejected { policy_bundle_id: Option<PolicyBundleId>, version: Option<String>, reason: String }`
- `PolicySyncFailed { policy_bundle_id: Option<PolicyBundleId>, version: Option<String>, reason: String }`
- `PolicyExpired { policy_bundle_id: PolicyBundleId, version: String, action: Option<String> }`
- `PolicyRevoked { policy_bundle_id: PolicyBundleId, version: String, reason: String }`
- `RunPaused { reason: Option<String> }`
- `RunResumed { reason: Option<String> }`
- `RunStopped { reason: Option<String> }`
- `PerceptsAppended { count: usize, schemas: Vec<String> }`
- `DaemonAudit { endpoint: String, audit: AuditAttribution }`
- `CircuitBreakerTripped { breaker: CircuitBreakerTraceContext }`
- `CircuitBreakerCleared { breaker: CircuitBreakerTraceContext }`
- `LoopTickStarted { tick_id }`
- `PerceptsReceived { percepts: Vec<Percept> }`
- `StateLoaded { state_hash: Option<ContentHash> }`
- `PolicyInvoked { policy: String }`
- `PolicyCompleted { policy: String }`
- `CandidatesProposed { actions: Vec<Action> }`
- `ConstraintsEvaluated { constraints: Vec<Constraint>, result: VerificationResult }`
- `ActionVerificationStarted { action: Action }`
- `ActionVerificationCompleted { action: Action, result: VerificationResult }`
- `ActionNeedsApproval { action: Action, result: VerificationResult }`
- `ActionExecuted { action: Action, outcome: serde_json::Value }`
- `ActionDenied { action: Action, result: VerificationResult }`
- `ActionFailed { action: Action, error: String, result: VerificationResult }`
- `ActionNeedsIntervention { action: Action, result: VerificationResult }`
- `ApprovalRequested { approval: ApprovalTraceContext }`
- `ApprovalGranted { approval: ApprovalTraceContext }`
- `ApprovalDenied { approval: ApprovalTraceContext, reason: String }`
- `ApprovalExpired { approval: ApprovalTraceContext, reason: String }`
- `ApprovalRevoked { approval: ApprovalTraceContext, reason: String }`
- `EscalationTriggered { escalation: EscalationContext }`
- `OutcomeRecorded { outcome: serde_json::Value, feedback: Option<Feedback>, reward: Option<Reward> }`
- `StateCommitted { state_hash: ContentHash, snapshot_id: Option<SnapshotId> }`
- `StateHandoffExported { handoff: StateHandoffTraceContext }`
- `StateHandoffImported { handoff: StateHandoffTraceContext }`
- `StateHandoffImportFailed { handoff: StateHandoffTraceContext, reason: String }`
- `ReadOnlyStateReferenced { handoff: StateHandoffTraceContext }`
- `MessageQueued { message: MessageTraceContext }`
- `MessageDelivered { message: MessageTraceContext }`
- `MessageRejected { message: MessageTraceContext, reason: String }`
- `MessageExpired { message: MessageTraceContext, reason: Option<String> }`
- `MessageConsumed { message: MessageTraceContext }`
- `RemoteMessageSent { remote_message: RemoteMessageTraceContext }`
- `RemoteMessageAccepted { remote_message: RemoteMessageTraceContext }`
- `RemoteMessageRejected { remote_message: RemoteMessageTraceContext, reason: String }`
- `RemoteMessageDelivered { remote_message: RemoteMessageTraceContext }`
- `RemoteMessageTimedOut { remote_message: RemoteMessageTraceContext, reason: String }`
- `RemoteMessageDuplicate { remote_message: RemoteMessageTraceContext, reason: String }`
- `RemoteMessageTransportFailed { remote_message: RemoteMessageTraceContext, reason: String }`
- `DelegationRequested { delegation: LocalDelegationTraceContext }`
- `DelegationRejected { delegation: LocalDelegationTraceContext, reason: String }`
- `ParentRunCancelled { parent_run_id: RunId, agent_id: AgentId, reason: String }`
- `ChildRunStarted { delegation: LocalDelegationTraceContext }`
- `ChildRunCompleted { delegation: LocalDelegationTraceContext }`
- `ChildRunFailed { delegation: LocalDelegationTraceContext, failure: TaskFailure }`
- `ChildRunLinked { parent_run_id, child_run_id, parent_agent_id, child_agent_id, causal_parent, source_message_id }`
- `GovernanceApprovalRequested { transition: GovernanceTransition }`
- `GovernanceApprovalGranted { transition: GovernanceTransition }`
- `GovernanceApprovalDenied { transition: GovernanceTransition }`
- `GovernanceApprovalExpired { transition: GovernanceTransition }`
- `GovernanceApprovalRevoked { transition: GovernanceTransition }`
- `EscalationOpened { transition: GovernanceTransition }`
- `EscalationResolved { transition: GovernanceTransition }`
- `EscalationExpired { transition: GovernanceTransition }`
- `EscalationRevoked { transition: GovernanceTransition }`
- `InterventionRequested { transition: GovernanceTransition }`
- `InterventionResolved { transition: GovernanceTransition }`
- `InterventionCancelled { transition: GovernanceTransition }`
- `InterventionExpired { transition: GovernanceTransition }`
- `InterventionRevoked { transition: GovernanceTransition }`
- `CircuitBreakerTripped { transition: GovernanceTransition }`
- `CircuitBreakerCleared { transition: GovernanceTransition }`
- `CircuitBreakerExpired { transition: GovernanceTransition }`
- `CircuitBreakerRevoked { transition: GovernanceTransition }`
- `KillSwitchActivated { transition: GovernanceTransition }`
- `KillSwitchCleared { transition: GovernanceTransition }`
- `KillSwitchExpired { transition: GovernanceTransition }`
- `KillSwitchRevoked { transition: GovernanceTransition }`
- `GovernanceTransitionRejected { rejection: GovernanceTransitionRejection }`
- `LoopTickCompleted { tick_id, integrity: Option<TraceIntegrity> }`

## Governance Events

0.04-S1 adds governance event variants for explicit governance state changes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `GovernanceApprovalRequested` | `governance.approval.requested` | Approval request state was created. |
| `GovernanceApprovalGranted` | `governance.approval.granted` | Approval request transitioned to granted. |
| `GovernanceApprovalDenied` | `governance.approval.denied` | Approval request transitioned to denied. |
| `GovernanceApprovalExpired` | `governance.approval.expired` | Approval or grant expired explicitly. |
| `GovernanceApprovalRevoked` | `governance.approval.revoked` | Approval state was revoked explicitly. |
| `EscalationOpened` | `escalation.opened` | Escalation state was opened. |
| `EscalationResolved` | `escalation.resolved` | Escalation state was resolved. |
| `EscalationExpired` | `escalation.expired` | Escalation state expired explicitly. |
| `EscalationRevoked` | `escalation.revoked` | Escalation state was revoked explicitly. |
| `InterventionRequested` | `intervention.requested` | Operator/runtime intervention was requested. |
| `InterventionResolved` | `intervention.resolved` | Intervention was resolved. |
| `InterventionCancelled` | `intervention.cancelled` | Intervention was cancelled. |
| `InterventionExpired` | `intervention.expired` | Intervention state expired explicitly. |
| `InterventionRevoked` | `intervention.revoked` | Intervention state was revoked explicitly. |
| `CircuitBreakerTripped` | `circuit_breaker.tripped` | Circuit-breaker state became active/tripped. |
| `CircuitBreakerCleared` | `circuit_breaker.cleared` | Circuit-breaker state was cleared. |
| `CircuitBreakerExpired` | `circuit_breaker.expired` | Circuit-breaker state expired explicitly. |
| `CircuitBreakerRevoked` | `circuit_breaker.revoked` | Circuit-breaker state was revoked explicitly. |
| `KillSwitchActivated` | `kill_switch.activated` | Kill-switch state became active. |
| `KillSwitchCleared` | `kill_switch.cleared` | Kill-switch state was cleared. |
| `KillSwitchExpired` | `kill_switch.expired` | Kill-switch state expired explicitly. |
| `KillSwitchRevoked` | `kill_switch.revoked` | Kill-switch state was revoked explicitly. |
| `GovernanceTransitionRejected` | `governance.transition_rejected` | Invalid transition failed closed and was recorded for audit/replay. |

Governance success events carry `GovernanceTransition`:

| Field | Purpose |
| --- | --- |
| `schema_version` | `splendor.governance_state.v1`. |
| `object` | Typed governance object identity (`approval_id`, `escalation_id`, `intervention_id`, `circuit_breaker_id`, or `kill_switch_id`). |
| `scope` | Tenant/agent/run/action/adapter/node/instance/fleet/global scope. |
| `from` | Previous state or omitted for creation. |
| `to` | Target state. |
| `occurred_at` | Transition timestamp. |
| `reason` | Sanitized reason. |
| `issuer` | `issuer_id` and source attribution. |
| `trace` | Causal trace linkage. |
| `extensions` | Optional non-authoritative metadata. |

`GovernanceTransitionRejected` carries `GovernanceTransitionRejection` with the
same object, scope, issuer, and trace linkage plus `attempted`, `from`,
`rejected_at`, and a stable rejection reason. Rejections do not become implicit
allows and do not execute side effects.

## Message Events

Message event variants correspond to these canonical event classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `MessageQueued` | `message.queued` | Message was accepted into a local delivery path. |
| `MessageDelivered` | `message.delivered` | Message reached the target agent's delivery boundary. |
| `MessageRejected` | `message.rejected` | Message was rejected before delivery. Payload validation failures must use this event with a reason. |
| `MessageExpired` | `message.expired` | Message expired before delivery or consumption. |
| `MessageConsumed` | `message.consumed` | Target agent runtime context consumed the message. |

All message events carry `MessageTraceContext`:

| Field | Purpose |
| --- | --- |
| `message_id` | Message identity distinct from trace, run, action, and state IDs. |
| `source_agent_id` | Agent that authored the message. |
| `target_agent_id` | Agent intended to consume the message. |
| `run_id` | Run that scopes the message. |
| `schema` | Message payload schema. |
| `causal_parent` | Optional trace event that causally produced the message. |

0.02-S1 defines these event payloads and serialization behavior. 0.02-S2 local
router behavior emits the lifecycle events for accepted, rejected, expired, and
consumed local messages. 0.02-S7 replay preserves `causal_parent` and rebuilds
message causality without re-delivering messages or executing adapter actions.

## Parent/child run links

`ChildRunLinked` is an explicit local replay/audit event for 0.02 parent/child
run relationships. It does not start, resume, or execute a child run. It records
only the relationship needed for inspect-only replay:

| Field | Purpose |
| --- | --- |
| `parent_run_id` | Run that delegated local work. |
| `child_run_id` | Child run receiving scoped local work. |
| `parent_agent_id` | Agent that owns the parent side. |
| `child_agent_id` | Agent that owns the child side. |
| `causal_parent` | Optional trace event that caused the delegation link. |
| `source_message_id` | Optional message carrying the local delegation request. |

Replay reports these links with `side_effects_replayed: false`; child adapter
actions are never executed by replay.

## Local Delegation Events

0.02-S4 adds local parent/child delegation events:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `DelegationRequested` | `delegation.requested` | Parent run requested scoped local child work. |
| `DelegationRejected` | `delegation.rejected` | Delegation failed closed before child execution. |
| `ParentRunCancelled` | `run.cancelled` | Parent run cancellation blocks future child delegation. |
| `ChildRunStarted` | `run.child_started` | Child run started and references the parent causal trace. |
| `ChildRunCompleted` | `run.child_completed` | Child run completed and parent references the response. |
| `ChildRunFailed` | `run.child_failed` | Child run failure is structured as `TaskFailure`. |

`LocalDelegationTraceContext` fields:

| Field | Purpose |
| --- | --- |
| `parent_run_id` | Parent/orchestrator run. |
| `child_run_id` | Child/specialist run. |
| `parent_trace_id` | Parent trace event that caused or recorded delegation. |
| `request_message_id` | Task request message, when routed. |
| `response_message_id` | Task response message, when routed. |
| `source_agent_id` | Parent/orchestrator agent. |
| `target_agent_id` | Child/specialist agent. |
| `objective` | Scoped child objective. |

## Daemon audit events

`DaemonAudit` records the caller attribution validated at the daemon boundary for
mutating daemon operations. It is emitted before the accepted mutation is applied
and carries:

| Field | Purpose |
| --- | --- |
| `endpoint` | Canonical endpoint scope such as `splendor.runs.create` or `splendor.actions.submit`. |
| `audit` | `AuditAttribution` with caller principal, optional credential ID, and request timestamp. |

These events do not authorize side effects. They only make accepted mutating
daemon calls trace/audit attributable; action execution still requires the
gateway/verifier path.

## Approval Events

0.04-S2 approval event variants correspond to these canonical event classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `ActionNeedsApproval` | `action.needs_approval` | The approval verifier paused the action before adapter execution. |
| `ApprovalRequested` | `approval.requested` | A policy-created approval request was recorded. |
| `ApprovalGranted` | `approval.granted` | Scoped approval grant evidence was presented. |
| `ApprovalDenied` | `approval.denied` | Approval denial or wrong-scope evidence was rejected. |
| `ApprovalExpired` | `approval.expired` | Expired approval evidence was rejected. |
| `ApprovalRevoked` | `approval.revoked` | Revoked approval evidence was rejected. |

All approval lifecycle events carry `ApprovalTraceContext`, which includes
`approval_id`, tenant, agent, run, action, adapter, decision, reason, policy/risk
metadata where available, expiry, and revocation state. Replay reconstructs these
events as approval facts only; it does not call approval services, verifiers,
gateways, or adapters.

## Circuit-breaker events

0.04-S4 adds circuit-breaker state-change events:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `CircuitBreakerTripped` | `circuit_breaker.tripped` | A breaker entered blocking state. |
| `CircuitBreakerCleared` | `circuit_breaker.cleared` | A breaker was explicitly reset/cleared. |

Both variants carry `CircuitBreakerTraceContext`: `breaker_id`, `scope`,
`state`, `reason`, `authorized_by`, and `recorded_at`. Matching action denials
remain ordinary `ActionDenied` events with `circuit_breaker_tripped` verification
reasons and scoped breaker artifacts.

## Work-order Events

Work-order events correspond to the 0.03-S3 signed work-order lifecycle:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `WorkOrderAccepted` | `work_order.accepted` | A signed work order validated and scoped a run. |
| `WorkOrderRejected` | `work_order.rejected` | Work-order ingestion failed closed before runtime execution. |

`WorkOrderAccepted` carries only identity metadata (`work_order_id`, `tenant_id`,
`agent_id`, and optional `run_id`). `WorkOrderRejected` carries those fields when
parseable plus a sanitized reason code. Neither event records signature material,
verification secrets, caller tokens, or broad credentials.

## Policy Distribution Events

0.04-S5 policy distribution event variants correspond to these canonical event
classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `PolicyBundleAccepted` | `policy.bundle.accepted` | A signed policy bundle validated and became run-local authority metadata. |
| `PolicyBundleRejected` | `policy.bundle.rejected` | A supplied policy bundle failed validation before authority changed. |
| `PolicySyncFailed` | `policy.sync.failed` | Central policy sync failed and cached authority was preserved. |
| `PolicyExpired` | `policy.expired` | TTL checks denied policy invocation or action forwarding. |
| `PolicyRevoked` | `policy.revoked` | Revocation denied policy invocation or action forwarding. |

`PolicyBundleAccepted` carries `PolicyBundleTraceContext`: policy bundle ID,
version, tenant ID, optional agent ID, expiry, and degraded-mode flags. It omits
signature material, shared secrets, caller credentials, and policy-language
internals. Replay inspects these events but does not refresh bundles, contact a
policy distributor, invoke policies, or execute adapters.

### Remote Message Events

Remote message event variants correspond to these canonical event classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `RemoteMessageSent` | `remote_message.sent` | Source instance attempted a remote transport send. |
| `RemoteMessageAccepted` | `remote_message.accepted` | Destination instance accepted the remote envelope after identity/schema/work-order validation. |
| `RemoteMessageRejected` | `remote_message.rejected` | Remote envelope failed validation or local delivery before enqueue. |
| `RemoteMessageDelivered` | `remote_message.delivered` | Wrapped local message reached the destination inbox boundary. |
| `RemoteMessageTimedOut` | `remote_message.timed_out` | Transport timed out before receiver acceptance. |
| `RemoteMessageDuplicate` | `remote_message.duplicate` | Receiver detected a duplicate `message_id` and did not deliver again. |
| `RemoteMessageTransportFailed` | `remote_message.transport_failed` | Non-timeout transport failure before receiver acceptance. |

All remote message events carry `RemoteMessageTraceContext`, including the local
`MessageTraceContext`, tenant ID, source/target instance IDs, work-order ID,
attempt number, and optional idempotency key. Replay can join source and receiver
traces by message ID and causal parent without re-sending or re-delivering.

## Escalation Events

0.04-S3 adds deterministic escalation trace events:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `EscalationTriggered` | `escalation.triggered` | A configured escalation trigger reached its threshold and produced a decision. |
| `ActionNeedsIntervention` | `action.needs_intervention` | The current action cannot proceed until operator/control-plane intervention occurs. |

`EscalationTriggered` carries `EscalationContext` with:

| Field | Purpose |
| --- | --- |
| `trigger` | Trigger category: verifier uncertainty, repeated adapter failure, approval timeout, quota pressure, policy expiry, or safety risk. |
| `threshold` / `observed_count` | Configured threshold and observed occurrences for deterministic replay. |
| `scope` | Scope evaluated by the policy: tenant, agent, run, action, or adapter. |
| `decision` | `NoAction`, `Deny`, `Pause`, or `NeedsIntervention`. |
| `tenant_id`, `agent_id`, `run_id` | Authority and run boundary. |
| `action_id`, `action_name`, `adapter` | Action/adapter reference when applicable. |
| `reason` | Stable reason code or summary. |
| `evidence` | Structured verifier/runtime evidence; never includes secrets. |
| `decided_at` | Decision timestamp. |

Escalation events do not execute adapters, contact notification systems, create
tickets, or implement circuit breakers. They are trace facts that make the local
governance decision replayable and auditable.

## State Handoff Events

State handoff event variants correspond to these canonical event classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `StateHandoffExported` | `state.handoff.exported` | Source exported a snapshot handoff. |
| `StateHandoffImported` | `state.handoff.imported` | Receiver imported a validated snapshot. |
| `StateHandoffImportFailed` | `state.handoff.import_failed` | Receiver failed closed before changing state head. |
| `ReadOnlyStateReferenced` | `state.reference.read_only` | Receiver attached an immutable state reference. |

All state handoff events carry `StateHandoffTraceContext`:

| Field | Purpose |
| --- | --- |
| `handoff_id` | Links source and receiver handoff events. |
| `mode` | `snapshot_import` or `read_only_reference`. |
| `tenant_id`, `agent_id`, `run_id` | Authority scope for the state boundary. |
| `work_order_id` | Signed work order authorizing import/reference. |
| `source_state_node_id` | Source state node being transferred or referenced. |
| `previous_state_node_id` | Receiver head expected before import/reference. |
| `receiver_state_node_id` | Receiver-owned node after successful import. |
| `snapshot_id` | Snapshot ID verified from state bytes. |
| `source_trace_id` | Source event proving the export/reference boundary. |

### TraceIntegrity

`TraceIntegrity` captures optional chain metadata emitted at the end of a tick:

- `prev_event_hash` (`Option<ContentHash>`): hash of the previous event in the run.
- `event_hash` (`ContentHash`): hash of the `LoopTickCompleted` event computed with
  `integrity` omitted from the payload.

## Example

```rust
use splendor_types::{RunId, TraceEvent, TraceEventKind};
use time::OffsetDateTime;

let run_id = RunId::new();
let event = TraceEvent::new(
    run_id,
    0,
    OffsetDateTime::now_utc(),
    TraceEventKind::LoopTickStarted { tick_id: 1 },
);
assert_eq!(event.sequence, 0);
assert_eq!(event.trace_event_id.to_string().len(), 36);
```

## Replay validation contract

Replay validates that stored trace records are contiguous, scoped to the
requested run, use deterministic `trace_event_id` values, and preserve hash-chain
continuity. A missing or corrupted segment causes replay to fail rather than
silently continuing.
