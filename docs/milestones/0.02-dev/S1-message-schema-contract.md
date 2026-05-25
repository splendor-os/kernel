# 0.02-S1 — Message Schema Contract

## Objective

Define typed local message primitives without coupling them to a specific
transport or future fleet implementation. This sprint strengthens the `message`
primitive and message-related trace schema while preserving the existing local
runtime loop.

## Functional scope

- Adds canonical Rust `MessageId`, `Message`, `MessageEnvelope`,
  `MessageSchemaVersion`, `MessageDeliveryStatus`, `MessageTraceLinks`,
  `MessageTraceContext`, and `MessageValidationError` in `splendor-types`.
- Validates required message identity, run scope, schema, payload, timestamp, and
  schema version before any router can handle a message.
- Adds message lifecycle trace event variants for queued, delivered, rejected,
  expired, and consumed events.
- Documents the transport-neutral message schema boundary and ADR.

## Non-goals

- No message broker.
- No local router, inbox, or outbox implementation.
- No remote transport.
- No distributed delivery guarantee.
- No shared mutable state channel.
- No local delegation or permission ledger implementation.
- No TypeScript package or Python SDK message API in this sprint.

## Public contracts changed

- New `splendor-types` exports:
  - `MessageId`
  - `Message`
  - `MessageEnvelope`
  - `MessageSchemaVersion`
  - `MessageDeliveryStatus`
  - `MessageTraceLinks`
  - `MessageTraceContext`
  - `MessageValidationError`
- New `TraceEventKind` variants:
  - `MessageQueued`
  - `MessageDelivered`
  - `MessageRejected`
  - `MessageExpired`
  - `MessageConsumed`
- New reference doc: `docs/reference/messages.md`.
- Updated reference docs: `docs/reference/trace-events.md` and
  `docs/reference/core-objects.md`.
- New ADR: `adr/0001-message-schema-boundary.md`.

## Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Message | Adds canonical message identity, payload, envelope, validation, lifecycle status, and causal parent fields. |
| Trace store | Adds message event payload taxonomy; existing append-only trace storage can serialize these events. |
| Replay | Causal parent and message trace context round-trip through serde so future replay can reconstruct causality. |
| Gateway | No behavior change. Messages do not authorize side effects; actions still require the Action Gateway. |
| Verifier | No verifier pipeline change. Invalid message schemas fail closed at validation. |
| State graph | No state nodes are created or modified. |

## Trace events added or changed

Added event definitions:

- `message.queued` / `TraceEventKind::MessageQueued`
- `message.delivered` / `TraceEventKind::MessageDelivered`
- `message.rejected` / `TraceEventKind::MessageRejected`
- `message.expired` / `TraceEventKind::MessageExpired`
- `message.consumed` / `TraceEventKind::MessageConsumed`

Payload validation failures must be represented as `message.rejected` events by
future routing code. This sprint defines and tests the event schema but does not
implement router emission.

## State behavior added or changed

No state graph behavior changes. Messages are scoped by `run_id` and preserve
`causal_parent`, but they do not mutate state in S1.

## Verifier/gateway behavior added or changed

No action verifier or gateway behavior changes. Message validation fails closed
for missing identity, missing schema, missing payload, malformed schema version,
unsupported schema version, or envelope/schema-version mismatch. Messages cannot
grant or broaden action authority.

## Replay behavior

Replay behavior remains inspect-only for message schema data in this sprint.
Message and envelope serialization preserve `message_id`, `run_id`, source and
target agent identities, `causal_parent`, delivery status, and trace links so a
future multi-agent replay harness can reconstruct causality without re-executing
side effects.

## Failure behavior

Structured `MessageValidationError` variants deny invalid messages before
routing. Unsupported versions are rejected instead of downgraded. JSON missing a
required `created_at` field fails deserialization.

## FR and acceptance traceability

| Requirement / criterion | Evidence |
| --- | --- |
| FR-0.02-01 canonical message type | `Message`, `MessageId`, `MessageEnvelope`; `round_trip_core_types` |
| Message cannot be created without source, target, run ID, message ID, schema, payload, timestamp | `Message::new`, `Message::validate`; `message_requires_all_identity_scope_fields`, `message_requires_schema_payload_and_timestamp` |
| Invalid schema versions rejected before routing | `MessageSchemaVersion::from_schema`; `invalid_schema_versions_are_rejected_before_routing` |
| Payload validation failure emits rejection trace event schema | `MessageValidationError::PayloadValidationFailed`, `TraceEventKind::MessageRejected`; `payload_validation_failure_is_structured_for_rejection_trace`, `message_rejection_trace_event_preserves_causal_parent` |
| Causal parent references trace event and is preserved during replay inputs | `causal_parent: Option<TraceId>`; `causal_parent_and_trace_links_round_trip_for_replay`, `message_rejection_trace_event_preserves_causal_parent` |
| Transport-neutral schema | No transport fields in `Message`/`MessageEnvelope`; `message_schema_is_transport_neutral`; ADR boundary decision |
| Rust/docs mirrored types consistent | `docs/reference/messages.md`, `docs/reference/trace-events.md#message-events`, Rust serde round-trip tests |

## Test evidence

| Test | Purpose |
| --- | --- |
| `message_requires_all_identity_scope_fields` | Negative/fail-closed identity validation. |
| `message_requires_schema_payload_and_timestamp` | Required schema, payload, and timestamp validation. |
| `invalid_schema_versions_are_rejected_before_routing` | Unsupported/malformed schema versions fail closed. |
| `message_envelope_validates_schema_version_and_status` | Envelope contract and status vocabulary. |
| `causal_parent_and_trace_links_round_trip_for_replay` | Replay input preservation through serde. |
| `payload_validation_failure_is_structured_for_rejection_trace` | Payload rejection reason shape. |
| `message_schema_is_transport_neutral` | No transport-specific envelope fields. |
| `message_rejection_trace_event_preserves_causal_parent` | Rejection trace event preserves message causality. |
| `message_lifecycle_trace_events_round_trip` | Message lifecycle trace variants serialize. |

## Example commands or fixtures

```bash
cargo test -p splendor-types message
cargo test -p splendor-types trace
cargo test --workspace
```

## Future extension notes

0.02-S2 local message router work should wrap and persist `MessageEnvelope`, use
`MessageDeliveryStatus`, and emit the message lifecycle trace events defined in
this sprint. 0.02-S7 replay work should read preserved `causal_parent` values
and message trace contexts to reconstruct the local causal graph. 0.03 remote
transport work may wrap the envelope but must not change message identity,
schema-version, run scope, or causal-parent semantics.
