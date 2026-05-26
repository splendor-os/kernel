# Trace Events Reference

Trace events form the append-only audit log for each run. Events are ordered by
sequence number within a `RunId` and must be emitted in strict tick order.

## TraceEvent

**Fields**

- `trace_event_id` (`TraceEventId`): deterministic identifier derived from `RunId` + sequence. Deserialization accepts legacy `trace_id` as an input alias during the 0.02 migration window.
- `run_id` (`RunId`): owning run.
- `sequence` (`u64`): monotonic per-run sequence number.
- `timestamp` (`OffsetDateTime`): capture time at emission.
- `identity` (`TraceIdentityContext`): runtime identity context containing required `run_id` and optional fleet, node, instance, tenant, agent, tick, action, state, and message IDs when applicable.
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
10. `ActionExecuted`, `ActionDenied`, or `ActionFailed`
11. `OutcomeRecorded`
12. `StateCommitted`
13. `LoopTickCompleted`

These Rust enum names correspond to the canonical runtime event classes used in
the rule documents: `run.started`, `tick.started`, `percepts.received`, `state.loaded`,
`policy.invoked`, `policy.completed`, `actions.proposed`,
`constraints.evaluated`, `verification.started`, `verification.completed`,
`action.executed`, `action.denied`, or `action.failed`, `outcome.recorded`,
`state.committed`, and `tick.completed`.

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
events include `action_id`; state commit events include `state_node_id`; message
lifecycle events include `message_id`. Fleet, node, and instance IDs are optional
until later 0.03 registry and transport sprints populate them.

Node and instance registry lifecycle events introduced in 0.03-S2 are management
audit events rather than run-scoped `TraceEventKind` variants. They are documented
in [`node-registry.md`](node-registry.md) and remain suitable for later
aggregation without inventing fake run IDs.

State handoff events are trace events too. They identify source and receiver
handoff boundaries and preserve the previous receiver state head for replay.

## TraceEventKind Payloads

- `RunStarted`
- `WorkOrderAccepted { work_order_id, tenant_id, agent_id, run_id }`
- `WorkOrderRejected { work_order_id: Option<WorkOrderId>, tenant_id: Option<TenantId>, agent_id: Option<AgentId>, run_id: Option<RunId>, reason: String }`
- `LoopTickStarted { tick_id }`
- `PerceptsReceived { percepts: Vec<Percept> }`
- `StateLoaded { state_hash: Option<ContentHash> }`
- `PolicyInvoked { policy: String }`
- `PolicyCompleted { policy: String }`
- `CandidatesProposed { actions: Vec<Action> }`
- `ConstraintsEvaluated { constraints: Vec<Constraint>, result: VerificationResult }`
- `ActionVerificationStarted { action: Action }`
- `ActionVerificationCompleted { action: Action, result: VerificationResult }`
- `ActionExecuted { action: Action, outcome: serde_json::Value }`
- `ActionDenied { action: Action, result: VerificationResult }`
- `ActionFailed { action: Action, error: String, result: VerificationResult }`
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
- `LoopTickCompleted { tick_id, integrity: Option<TraceIntegrity> }`

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
consumed local messages. Replayed trace records preserve `causal_parent`,
allowing future multi-agent replay to rebuild message causality without executing
message side effects or adapter actions.

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
