# 0.03-S5 — Remote message transport

## Objective

Carry typed messages between Splendor instances while preserving the 0.02 local
message contract, identity separation, causal trace linkage, work-order authority,
and failure visibility.

## Functional scope

- Added `RemoteMessageEnvelope` as a remote wrapper around canonical
  `MessageEnvelope` without changing `Message`.
- Added retry/idempotency metadata and validation.
- Added a narrow `RemoteMessageTransport` interface and one in-memory reference
  transport for tests/examples.
- Added `RemoteMessageReceiver` to validate remote envelopes and bridge accepted
  messages into the destination local inbox.
- Added duplicate detection by `message_id`.
- Added remote message trace events for send, accept, reject, deliver, timeout,
  duplicate, and transport failure.

## Non-goals

- No distributed consensus.
- No exactly-once global guarantee.
- No arbitrary remote state mutation.
- No production network broker or daemon endpoint.
- No fleet scheduler, trace aggregation, or state handoff implementation.
- No permission inheritance from sender to receiver.

## Public contracts changed

- Rust types:
  - `splendor_types::RemoteMessageEnvelope`
  - `splendor_types::RemoteMessageEnvelopeVersion`
  - `splendor_types::RemoteMessageRetryPolicy`
  - `splendor_types::RemoteMessageTraceContext`
  - `splendor_types::RemoteMessageValidationError`
  - `splendor_kernel::RemoteMessageTransport`
  - `splendor_kernel::RemoteMessageReceiver`
  - `splendor_kernel::InMemoryRemoteMessageTransport`
  - `splendor_kernel::send_remote_message`
- Security scope:
  - `EndpointScope::MessagesSend` / `splendor.messages.send`
- Trace events:
  - `RemoteMessageSent`
  - `RemoteMessageAccepted`
  - `RemoteMessageRejected`
  - `RemoteMessageDelivered`
  - `RemoteMessageTimedOut`
  - `RemoteMessageDuplicate`
  - `RemoteMessageTransportFailed`

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | none; messages still do not authorize side effects |
| Verifier | work-order authority validation added at remote receive boundary |
| State graph | none; remote messages do not create state nodes |
| Trace store | added remote message events |
| Replay | remote send/receive causal linkage is reconstructable from trace |
| Message | added remote wrapper and transport boundary |
| Work order | remote receive requires signed scoped work order |
| Governance | none |

## Trace behavior

- Source traces `remote_message.sent` before a transport attempt.
- Source traces `remote_message.timed_out` or
  `remote_message.transport_failed` on transport failure.
- Receiver traces `remote_message.rejected` for invalid identity, schema,
  work-order, target, or local delivery failures.
- Receiver traces `remote_message.duplicate` and does not deliver a duplicate.
- Receiver traces `remote_message.accepted`, local `message.delivered`, then
  `remote_message.delivered` for a successful handoff.

## State behavior

Remote messaging does not mutate the state graph and does not create state nodes.
It only updates explicit local router inbox state for accepted destination
messages. Source outbox state remains owned by the source instance.

## Gateway and verifier behavior

Remote messaging does not execute adapters or side effects. It validates remote
identity/schema/work-order/target before local delivery and fails closed on
uncertainty. Any side-effectful action proposed as a result of a message must
still pass through the Action Gateway.

## Replay behavior

Replay remains inspect-only. Remote send and receive sides can be reconstructed
from `RemoteMessageTraceContext.message.message_id` and the canonical
`causal_parent`; replay must not re-send, re-deliver, or execute actions.

## Failure behavior

- Invalid envelope/work-order/target: trace `remote_message.rejected`, no inbox
  mutation.
- Duplicate: trace `remote_message.duplicate`, no second delivery.
- Timeout/failure: trace source-side timeout/failure; retry only if explicitly
  idempotent and within `max_attempts`.
- Trace persistence failure: return error and fail closed.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | remote envelope validation, unchanged canonical payload, retry metadata | `cargo test -p splendor-types message -- --nocapture` |
| contract | remote trace event serialization and causal linkage | `cargo test -p splendor-types message -- --nocapture` |
| integration-like unit | two in-memory instances exchange a trace-linked message | `cargo test -p splendor-kernel remote_message_transport -- --nocapture` |
| negative | wrong target instance and incompatible work orders fail closed | `cargo test -p splendor-kernel remote_message_transport -- --nocapture` |
| replay/trace | send/receive sides share message ID and causal parent in trace | `cargo test -p splendor-kernel remote_message_transport -- --nocapture` |

## Example or fixture

- `examples/two-instance-message/README.md`

## Future extension notes

Later 0.03 work can replace the in-memory reference transport with a real
authenticated transport without changing the canonical `Message` payload. Typed
fleet/node/instance IDs, trace aggregation, and state handoff can attach to the
remote envelope metadata and trace context without introducing shared mutable
state or global exactly-once claims.
