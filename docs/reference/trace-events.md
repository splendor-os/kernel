# Trace Events Reference

Trace events form the append-only audit log for each run. Events are ordered by
sequence number within a `RunId` and must be emitted in strict tick order.

## TraceEvent

**Fields**

- `trace_id` (`TraceId`): deterministic identifier derived from `RunId` + sequence.
- `run_id` (`RunId`): owning run.
- `sequence` (`u64`): monotonic per-run sequence number.
- `timestamp` (`OffsetDateTime`): capture time at emission.
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
rejected, expired, or consumed.

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
- `MessageQueued { message: MessageTraceContext }`
- `MessageDelivered { message: MessageTraceContext }`
- `MessageRejected { message: MessageTraceContext, reason: String }`
- `MessageExpired { message: MessageTraceContext, reason: Option<String> }`
- `MessageConsumed { message: MessageTraceContext }`
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
```

## Replay validation contract

0.01-dev replay validates that stored trace records are contiguous, scoped to
the requested run, use deterministic trace IDs, and preserve hash-chain
continuity. A missing or corrupted segment causes replay to fail rather than
silently continuing.
