# Local Message Router Reference

The local message router is the Splendor 0.02-S2 reference implementation for
in-process agent-to-agent message delivery inside one Splendor instance. It
strengthens the `message`, `trace store`, and local runtime-context primitives
without adding remote transport, delegation, or daemon API behavior.

Implemented in Rust as `splendor_kernel::{MessageRouter,
LocalMessageRouter}`.

## Purpose

The router accepts validated `MessageEnvelope` values from a source agent runtime
context and makes them visible only to the target agent runtime context for the
same run. Delivery is explicit, trace-linked, and fail-closed.

Messages are coordination data only. They do not grant permissions, execute
policy callbacks, call adapters, or authorize side effects. Side-effectful work
must still be proposed by policy and mediated by the Action Gateway.

## Public contract

```rust
pub trait MessageRouter {
    fn register_agent(&self, agent_id: AgentId) -> Result<(), MessageRouterError>;
    fn send(
        &self,
        recorder: &dyn MessageTraceRecorder,
        envelope: MessageEnvelope,
    ) -> Result<MessageEnvelope, MessageRouterError>;
    fn send_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        envelope: MessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, MessageRouterError>;
    fn inbox(&self, agent_id: &AgentId, run_id: &RunId)
        -> Result<Vec<MessageEnvelope>, MessageRouterError>;
    fn outbox(&self, agent_id: &AgentId, run_id: &RunId)
        -> Result<Vec<MessageEnvelope>, MessageRouterError>;
    fn mailbox(&self, agent_id: &AgentId, run_id: &RunId)
        -> Result<AgentMailboxSnapshot, MessageRouterError>;
    fn consume_next(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<Option<MessageEnvelope>, MessageRouterError>;
    fn consume(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        message_id: &MessageId,
    ) -> Result<MessageEnvelope, MessageRouterError>;
}
```

`MessageTraceRecorder` is implemented for `KernelRuntime`. The recorder run must
match the message run. A mismatch fails closed instead of writing a trace event
under the wrong run.

## Router configuration

`MessageRouterConfig` provides local-only queue controls:

| Field | Default | Meaning |
| --- | ---: | --- |
| `max_inbox_messages` | `1024` | Maximum delivered messages retained in one agent inbox. |
| `max_outbox_messages` | `1024` | Maximum routed messages retained in one source outbox. |
| `max_message_age` | `None` | Optional TTL; stale messages expire before delivery or consumption. |

The queue limits are the S2 capacity/quota behavior. Per-agent permission and
quota ledgers remain future 0.02-S3 scope.

## Lifecycle

1. Register source and target agents with `register_agent` or
   `LocalMessageRouter::register_agent_context`.
2. Build a S1 `MessageEnvelope` around a typed `Message`.
3. Call `send`/`send_at` with a trace recorder scoped to the message run.
4. The router validates the envelope, source, target, TTL, and queue capacity.
5. On success, the router emits `message.queued` and `message.delivered`, stores
   the delivered envelope in the source outbox and target inbox, and returns the
   delivered envelope with trace links.
6. Target agents call `inbox` for a non-mutating snapshot or `consume_next` /
   `consume` to mark a message consumed and emit `message.consumed`.

Ordering is deterministic within a `(source_agent_id, target_agent_id, run_id)`
stream because messages are appended to per-agent FIFO queues.

## Trace events

The local router emits the message lifecycle events defined in
[`trace-events.md`](trace-events.md):

- `message.queued` / `TraceEventKind::MessageQueued`
- `message.delivered` / `TraceEventKind::MessageDelivered`
- `message.rejected` / `TraceEventKind::MessageRejected`
- `message.expired` / `TraceEventKind::MessageExpired`
- `message.consumed` / `TraceEventKind::MessageConsumed`

Every event carries `MessageTraceContext`, including `message_id`, source agent,
target agent, run, schema, and `causal_parent`. The envelope's
`MessageTraceLinks` stores trace IDs for successful delivery and consumption.

## Failure modes

The router fails closed and does not enqueue messages when:

- the envelope schema or identity validation fails;
- the source agent is not registered;
- the target agent is not registered;
- the target inbox or source outbox is full;
- the message is expired by router TTL;
- the trace recorder is scoped to a different run;
- trace persistence fails;
- router storage is unavailable.

Rejected messages emit `message.rejected` when the message run can be traced.
Expired messages emit `message.expired`.

## State and replay behavior

Message routing does not mutate the state graph and does not create state nodes.
It updates explicit router inbox/outbox queues only. Inbox and outbox reads return
snapshots and do not mutate unrelated agent state.

Replay remains inspect-only. The router's trace events preserve message identity
and causal parent data so later 0.02-S7 replay work can reconstruct local
multi-agent causality without re-delivering messages or executing adapters.

## Non-goals

- No cross-instance or remote messaging.
- No durable remote queue or broker semantics.
- No exactly-once distributed delivery.
- No scoped delegation model.
- No per-agent permission ledger beyond registered local source/target checks.
- No daemon API or TypeScript client surface.

## Minimal example

```rust
use splendor_kernel::{KernelRuntime, LocalMessageRouter, MessageRouter};
use splendor_types::{AgentId, Message, MessageEnvelope, MessageId, RunId};
use time::OffsetDateTime;

let run_id = RunId::new();
let source = AgentId::new();
let target = AgentId::new();
let runtime = KernelRuntime::new(splendor_kernel::KernelRuntimeConfig {
    run_id: Some(run_id.clone()),
    ..Default::default()
});
let router = LocalMessageRouter::new();
router.register_agent(source.clone())?;
router.register_agent(target.clone())?;

let message = Message::new(
    MessageId::new(),
    source,
    target.clone(),
    run_id.clone(),
    "splendor.message.task_request.v1",
    serde_json::json!({"task": "summarize"}),
    None,
    true,
    OffsetDateTime::now_utc(),
)?;
let envelope = MessageEnvelope::new(message)?;
router.send(&runtime, envelope)?;
let inbox = router.inbox(&target, &run_id)?;
assert_eq!(inbox.len(), 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```
