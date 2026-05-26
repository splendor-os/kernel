# Messages Reference

Messages are the transport-neutral primitive for local agent-to-agent
coordination in Splendor 0.02. A message is not a chat transcript and it is not a
transport packet. It is a typed, versioned runtime object scoped by explicit
agent and run identity so later local routing, delegation, and replay work can
preserve causality without inheriting permissions implicitly.

Implemented in Rust as `splendor_types::{Message, MessageEnvelope}`.

## Functional scope

0.02-S1 defines the schema contract:

- canonical `MessageId` identity distinct from tenant, agent, run, trace, action,
  and state IDs;
- `Message` payload and identity fields;
- `MessageEnvelope` with schema version, delivery status, and trace links;
- schema-version validation that fails closed before routing;
- message lifecycle trace event payloads.

0.02-S2 adds the in-process local router, inbox/outbox APIs, delivery state
transitions, and trace emission documented in
[`local-message-router.md`](local-message-router.md). The message schema remains
transport-neutral: no broker, remote transport, distributed delivery guarantee,
or shared mutable state channel is part of the message object.

0.03-S5 adds a remote wrapper documented in
[`remote-messaging.md`](remote-messaging.md). The wrapper carries instance,
work-order, retry, and idempotency metadata but does not change the canonical
`Message` payload or local `MessageEnvelope` schema.

## Message

| Field | Rust type | Required | Purpose |
| --- | --- | --- | --- |
| `message_id` | `MessageId` | yes | Unique message identity. The nil UUID is rejected. |
| `source_agent_id` | `AgentId` | yes | Agent that authored the message. The nil UUID is rejected. |
| `target_agent_id` | `AgentId` | yes | Agent intended to consume the message. The nil UUID is rejected. |
| `run_id` | `RunId` | yes | Run that scopes the message and trace causality. The nil UUID is rejected. |
| `schema` | `String` | yes | Versioned payload schema, such as `splendor.message.task_request.v1`. |
| `payload` | `serde_json::Value` | yes | Typed JSON payload. JSON `null` is rejected at the envelope layer. |
| `causal_parent` | `Option<TraceId>` | no | Trace event that causally produced the message. Preserved by serialization/replay inputs. |
| `requires_response` | `bool` | yes | Whether the sender expects a response message. |
| `created_at` | `OffsetDateTime` | yes | Message creation timestamp. |

Example:

```json
{
  "message_id": "00000000-0000-0000-0000-000000000001",
  "source_agent_id": "00000000-0000-0000-0000-000000000002",
  "target_agent_id": "00000000-0000-0000-0000-000000000003",
  "run_id": "00000000-0000-0000-0000-000000000004",
  "schema": "splendor.message.task_request.v1",
  "payload": {
    "task": "forecast revenue for Q3",
    "input_ref": "dataset:finance.revenue_monthly_v4"
  },
  "causal_parent": "00000000-0000-0000-0000-000000000005",
  "requires_response": true,
  "created_at": "2026-05-25T00:00:00Z"
}
```

## MessageEnvelope

`MessageEnvelope` is strict around the message object while remaining
transport-neutral.

| Field | Rust type | Purpose |
| --- | --- | --- |
| `message` | `Message` | Validated message payload and identities. |
| `schema_version` | `MessageSchemaVersion` | Parsed version from `message.schema`; currently only `V1`. |
| `delivery_status` | `MessageDeliveryStatus` | Local lifecycle status. |
| `trace_links` | `MessageTraceLinks` | Optional trace IDs for queued, delivered, rejected, expired, and consumed events. |

`MessageDeliveryStatus` values are `pending`, `queued`, `delivered`, `rejected`,
`expired`, and `consumed`. The local router updates these statuses for accepted,
delivered, expired, and consumed envelopes while preserving trace links.

## Schema versioning

Message payload schemas must end with a `.vN` suffix. The 0.02-S1 contract
accepts only `v1`:

```text
splendor.message.<schema-name>.v1
```

Validation failures are structured as `MessageValidationError` and fail closed
before routing:

- missing message, source agent, target agent, or run identity;
- missing schema;
- missing or malformed schema version;
- unsupported schema version;
- missing payload;
- schema-specific payload validation failure;
- envelope/message schema-version mismatch.

The envelope validates payload presence only. Schema-specific payload validators
remain flexible and belong to the schema owner. When a schema-specific payload
validator rejects a message, routing code must record a `message.rejected` trace
event with the message trace context and reason.

## Trace and replay behavior

Messages carry an optional `causal_parent: TraceId`. The value identifies the
trace event that caused the message to be proposed or produced. Serialization
round trips preserve this field, allowing replay and future multi-agent causal
graph inspection to reconstruct message lineage without re-executing side
effects.

Message lifecycle trace events are documented in
[`trace-events.md#message-events`](trace-events.md#message-events).

## Transport neutrality

The canonical schema contains no transport-specific fields such as endpoint URLs,
topics, stream names, connection IDs, node IDs, fleet IDs, or protocol names.
Later local or remote transports may wrap `MessageEnvelope`; they must not change
message identity, run scope, causal parent, or payload schema semantics.

`RemoteMessageEnvelope` is such a wrapper. It is the remote transport contract,
not a replacement for `Message`.

## Security notes

Messages do not grant permissions. A receiving agent must operate under its own
agent identity, permissions, quotas, work-order scope, and gateway/verifier
checks. Shared or specialist agents must not use messages as a permission
laundering channel.
