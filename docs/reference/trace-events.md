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
before the first tick. Events MUST then be emitted in the following order for
each tick:

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

Local delegation events are emitted outside the tick ordering when a parent run
creates, completes, fails, cancels, or rejects local child work. They are ordered
by each affected run's sequence counter and carry explicit parent/child IDs.

0.02-S5 daemon lifecycle events are emitted outside the tick body but through the
same run trace runtime, preserving monotonic sequence order before the next tick
or replay inspection. Mutating daemon calls also emit `DaemonAudit` before the
mutation so caller attribution is persisted in the run trace.

## TraceEventKind Payloads

- `RunStarted`
- `RunPaused { reason: Option<String> }`
- `RunResumed { reason: Option<String> }`
- `RunStopped { reason: Option<String> }`
- `PerceptsAppended { count: usize, schemas: Vec<String> }`
- `DaemonAudit { endpoint: String, audit: AuditAttribution }`
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
- `DelegationRequested { delegation: LocalDelegationTraceContext }`
- `DelegationRejected { delegation: LocalDelegationTraceContext, reason: String }`
- `ParentRunCancelled { parent_run_id: RunId, agent_id: AgentId, reason: String }`
- `ChildRunStarted { delegation: LocalDelegationTraceContext }`
- `ChildRunCompleted { delegation: LocalDelegationTraceContext }`
- `ChildRunFailed { delegation: LocalDelegationTraceContext, failure: TaskFailure }`
- `ChildRunLinked { parent_run_id, child_run_id, parent_agent_id, child_agent_id, causal_parent, source_message_id }`
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
