# Local Multi-Agent Router Example

This example documents the 0.02-S2 local message router path. It is intentionally
in-process and local-only: no daemon API, remote broker, cross-instance transport,
delegation model, or permission ledger is involved.

## What it demonstrates

- Register two local agent runtime identities with `LocalMessageRouter`.
- Send a typed S1 `MessageEnvelope` from an orchestrator to a specialist.
- Emit `message.queued` and `message.delivered` trace events through a
  `KernelRuntime` scoped to the message `run_id`.
- Read the target inbox without exposing the message to unrelated agents.
- Consume the message and emit `message.consumed`.

Messages do not execute policy callbacks or adapters. Any side effect proposed as
a result of a message must still go through the Action Gateway.

## Minimal Rust fixture

```rust
use splendor_kernel::{KernelRuntime, KernelRuntimeConfig, LocalMessageRouter, MessageRouter};
use splendor_types::{AgentId, Message, MessageEnvelope, MessageId, RunId};
use time::OffsetDateTime;

let run_id = RunId::new();
let orchestrator = AgentId::new();
let specialist = AgentId::new();

let runtime = KernelRuntime::new(KernelRuntimeConfig {
    run_id: Some(run_id.clone()),
    ..KernelRuntimeConfig::default()
});
let router = LocalMessageRouter::new();
router.register_agent(orchestrator.clone())?;
router.register_agent(specialist.clone())?;

let message = Message::new(
    MessageId::new(),
    orchestrator,
    specialist.clone(),
    run_id.clone(),
    "splendor.message.task_request.v1",
    serde_json::json!({
        "task": "forecast revenue for Q3",
        "input_ref": "dataset:finance.revenue_monthly_v4"
    }),
    None,
    true,
    OffsetDateTime::now_utc(),
)?;
let envelope = MessageEnvelope::new(message)?;

let delivered = router.send(&runtime, envelope)?;
assert!(delivered.trace_links.queued_trace_id.is_some());
assert!(delivered.trace_links.delivered_trace_id.is_some());

let inbox = router.inbox(&specialist, &run_id)?;
assert_eq!(inbox.len(), 1);

let consumed = router.consume_next(&runtime, &specialist, &run_id)?;
assert!(consumed.is_some());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Validation commands

```bash
cargo test -p splendor-kernel message_router
cargo test -p splendor-kernel
```

## Expected trace behavior

Successful delivery emits, in order:

1. `message.queued`
2. `message.delivered`
3. `message.consumed` when the target consumes the message

Denial paths emit `message.rejected`; TTL expiry emits `message.expired`. Router
denial does not call target policy or adapter execution.

## Non-goals

- No remote transport.
- No durable broker queue.
- No exactly-once distributed semantics.
- No permission delegation or broad inherited authority.
- No replay re-delivery or adapter execution.
