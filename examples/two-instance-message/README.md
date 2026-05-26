# Two-instance remote message example

This example documents the 0.03-S5 reference path for sending one typed message
from a source Splendor instance to a target Splendor instance using the in-memory
test transport. It is intentionally not a production network transport.

## What it proves

- A canonical `MessageEnvelope` is wrapped in `RemoteMessageEnvelope` without
  changing the local message payload.
- The receiver validates tenant, run, target agent, target instance, schema, and
  signed work-order authority before local delivery.
- The target local inbox receives exactly one message.
- Source and target runtimes emit remote trace events that can be joined by
  `message_id` and `causal_parent`.

## Minimal Rust path

```rust,no_run
use splendor_kernel::{
    send_remote_message, InMemoryRemoteMessageTransport, KernelRuntime,
    KernelRuntimeConfig, LocalMessageRouter, MessageRouter, RemoteMessageReceiver,
};
use splendor_types::{
    AgentId, EndpointScope, Message, MessageEnvelope, MessageId,
    RemoteMessageEnvelope, RemoteMessageRetryPolicy, RevocationStatus, RunId,
    TenantId, WorkOrderAuthorization, WorkOrderSignature,
};
use time::{Duration, OffsetDateTime};

let now = OffsetDateTime::now_utc();
let run_id = RunId::new();
let tenant_id = TenantId::new();
let source_agent = AgentId::new();
let target_agent = AgentId::new();

let source_runtime = KernelRuntime::new(KernelRuntimeConfig {
    run_id: Some(run_id.clone()),
    ..KernelRuntimeConfig::default()
});
let target_runtime = KernelRuntime::new(KernelRuntimeConfig {
    run_id: Some(run_id.clone()),
    ..KernelRuntimeConfig::default()
});

let target_router = LocalMessageRouter::new();
target_router.register_agent(target_agent.clone())?;
let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
let transport = InMemoryRemoteMessageTransport::new(&receiver, &target_runtime);

let message = Message::new(
    MessageId::new(),
    source_agent,
    target_agent.clone(),
    run_id.clone(),
    "splendor.message.task_request.v1",
    serde_json::json!({"task": "summarize"}),
    None,
    true,
    now,
)?;

let work_order = WorkOrderAuthorization {
    work_order_id: "wo_remote_example".to_string(),
    tenant_id: tenant_id.clone(),
    agent_id: target_agent.clone(),
    run_id: Some(run_id.clone()),
    allowed_scopes: vec![EndpointScope::MessagesSend],
    signature: Some(WorkOrderSignature {
        key_id: "key_example".to_string(),
        signature: "sig_example".to_string(),
    }),
    expires_at: now + Duration::hours(1),
    revocation: RevocationStatus::Active,
};

let remote = RemoteMessageEnvelope::new(
    tenant_id,
    "instance_source",
    "instance_target",
    work_order,
    MessageEnvelope::new(message)?,
    RemoteMessageRetryPolicy::Never,
    now,
    Some(now + Duration::minutes(5)),
)?;

send_remote_message(&transport, &source_runtime, remote, now)?;
let inbox = target_router.inbox(&target_agent, &run_id)?;
assert_eq!(inbox.len(), 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Expected trace behavior

Successful delivery records:

1. source `remote_message.sent`
2. target `remote_message.accepted`
3. target local `message.delivered`
4. target `remote_message.delivered`

Failures record `remote_message.rejected`, `remote_message.timed_out`,
`remote_message.transport_failed`, or `remote_message.duplicate` and do not
silently mutate target runtime state.

## Non-goals

- No distributed consensus.
- No exactly-once global guarantee.
- No arbitrary remote state mutation.
- No production broker or daemon endpoint.
- No side-effect execution; actions still require the Action Gateway.
