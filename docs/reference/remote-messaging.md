# Remote Messaging Reference

Remote messaging is the Splendor 0.03-S5 transport boundary for carrying the
existing canonical `MessageEnvelope` between Splendor instances. It strengthens
the `message`, `trace store`, `replay`, and work-order authority primitives
without changing the local message payload schema or adding a production broker.

Implemented in Rust as:

- `splendor_types::RemoteMessageEnvelope`
- `splendor_types::RemoteMessageRetryPolicy`
- `splendor_types::RemoteMessageTraceContext`
- `splendor_kernel::RemoteMessageTransport`
- `splendor_kernel::RemoteMessageReceiver`
- `splendor_kernel::InMemoryRemoteMessageTransport`

## Purpose

A remote message is coordination data sent from one Splendor instance to another.
It does not grant permissions, execute adapters, mutate remote state, or authorize
side effects. A receiving agent still operates only under its own tenant, agent,
run, work-order, quota, and Action Gateway boundaries.

## RemoteMessageEnvelope

`RemoteMessageEnvelope` wraps, but does not modify, the canonical local
`MessageEnvelope`:

| Field | Purpose |
| --- | --- |
| `remote_schema_version` | Version of the remote wrapper, currently `v1`. |
| `tenant_id` | Tenant boundary for the remote handoff. |
| `source_instance_id` | Origin Splendor instance identity as an opaque string. |
| `target_instance_id` | Destination Splendor instance identity as an opaque string. |
| `work_order` | Signed scoped `WorkOrderAuthorization` authorizing the target agent/run. |
| `message_envelope` | Existing transport-neutral local `MessageEnvelope`. |
| `attempt` | 1-based transport attempt counter. |
| `retry_policy` | Retry/idempotency policy; default is no retry. |
| `sent_at` | Time the current attempt was sent. |
| `expires_at` | Optional remote wrapper expiry. |

The wrapped canonical `Message` keeps the same `message_id`, source agent,
target agent, run, schema, payload, causal parent, response flag, and timestamp.
Remote metadata is never added to the canonical message payload.

## Work-order authority

Receive-side validation fails closed unless the remote envelope has:

- a non-nil tenant ID;
- distinct, non-empty source and target instance IDs;
- a valid local `MessageEnvelope`;
- a signed, unexpired, unrevoked work order;
- `work_order.tenant_id == envelope.tenant_id`;
- `work_order.agent_id == message.target_agent_id`;
- `work_order.run_id == message.run_id`;
- `work_order.allowed_scopes` containing `splendor.messages.send`.

This prevents remote messages from laundering sender permissions into a target
agent. The message is only accepted for the target agent/run authorized by the
work order.

## Transport adapter interface

`RemoteMessageTransport` is intentionally narrow:

```rust
pub trait RemoteMessageTransport: Send + Sync {
    fn transmit_once(
        &self,
        source_recorder: &dyn MessageTraceRecorder,
        envelope: RemoteMessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, RemoteMessageTransportError>;
}
```

The reference implementation is `InMemoryRemoteMessageTransport`, used for tests
and examples. It connects a source runtime recorder to a destination
`RemoteMessageReceiver`. It is not a production network transport.

## Receive lifecycle

1. Source calls `send_remote_message(...)` with a remote envelope and transport.
2. Source records `remote_message.sent`.
3. Transport either records a timeout/failure or passes the envelope to the
   destination receiver.
4. Receiver validates the remote envelope and work-order authority.
5. Receiver checks duplicate `message_id` values in its explicit duplicate
   ledger.
6. Receiver records `remote_message.accepted`.
7. Receiver bridges the wrapped `MessageEnvelope` into the local target inbox via
   `LocalMessageRouter::deliver_remote_inbound_at(...)`.
8. Local delivery records `message.delivered`.
9. Receiver records `remote_message.delivered`.

Remote inbound delivery does not write to a local source outbox because the source
agent context is owned by the source instance.

## Trace events

Remote transport emits these trace event classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `RemoteMessageSent` | `remote_message.sent` | Source instance attempted remote send. |
| `RemoteMessageAccepted` | `remote_message.accepted` | Receiver validated identity/schema/work-order/target before local delivery. |
| `RemoteMessageRejected` | `remote_message.rejected` | Receiver or source rejected the envelope fail-closed. |
| `RemoteMessageDelivered` | `remote_message.delivered` | Message reached the destination local inbox boundary. |
| `RemoteMessageTimedOut` | `remote_message.timed_out` | Transport timed out before receiver acceptance. |
| `RemoteMessageDuplicate` | `remote_message.duplicate` | Receiver detected a duplicate `message_id` and did not deliver again. |
| `RemoteMessageTransportFailed` | `remote_message.transport_failed` | Non-timeout transport failure before receiver acceptance. |

All remote events carry `RemoteMessageTraceContext`, which includes the local
message trace context, tenant ID, source/target instance IDs, work-order ID,
attempt number, and optional idempotency key.

## Duplicate and retry behavior

Duplicate detection is deterministic at the receiver and keyed by `message_id`.
A duplicate records `remote_message.duplicate` and does not enqueue a second
local inbox message.

Retry is disabled by default. `send_remote_message(...)` retries only when:

- the error is a timeout or transport failure;
- the envelope uses `RemoteMessageRetryPolicy::Idempotent`;
- the idempotency key is non-empty;
- the next attempt would not exceed `max_attempts`.

Rejected envelopes, wrong target instances, duplicate messages, trace failures,
and local delivery failures are not retried.

## Replay behavior

Remote messaging has no replay side effects. Replay can inspect the send and
receive traces by joining `RemoteMessageTraceContext.message.message_id` and the
canonical message `causal_parent`. Replay must not re-send or re-deliver remote
messages.

## Failure modes

Remote messaging fails closed and records a trace event when possible for:

- malformed local message schema or payload;
- missing tenant/source instance/target instance identity;
- same source and target instance ID;
- unsigned, expired, revoked, or incompatible work order;
- expired remote envelope;
- wrong target instance;
- duplicate message ID;
- target local inbox quota failure;
- transport timeout or failure;
- trace persistence failure.

## Non-goals

- No distributed consensus.
- No exactly-once global guarantee.
- No arbitrary remote state mutation.
- No production network broker.
- No daemon remote-message endpoint.
- No permission inheritance from source agent to target agent.
