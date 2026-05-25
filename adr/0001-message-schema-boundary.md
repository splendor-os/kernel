# ADR 0001: Message Schema Boundary

## Status

Accepted for Splendor0.02-dev Sprint 0.02-S1.

## Context

Splendor 0.02 introduces local multi-agent coordination. Before implementing a
router, inbox/outbox storage, local delegation, or remote transport, the kernel
needs a canonical message schema that preserves identity separation and trace
causality.

The sprint criteria require:

- canonical `Message` and `MessageEnvelope` schemas;
- validation errors for missing identity, schema, payload, or run fields, plus
  causal-parent preservation when a parent trace event is supplied;
- schema versioning and invalid-version rejection before routing;
- trace event definitions for queued, delivered, rejected, expired, and consumed
  message lifecycle states;
- transport neutrality with no HTTP, NATS, gRPC, broker, or fleet-specific
  routing semantics.

## Decision

Define messages in `crates/splendor-types` as a pure schema boundary:

- `MessageId` is a distinct UUID-backed identity type.
- `Message` carries `message_id`, `source_agent_id`, `target_agent_id`, `run_id`,
  `schema`, `payload`, optional `causal_parent`, `requires_response`, and
  `created_at`.
- `MessageEnvelope` carries the validated message, parsed schema version,
  delivery status, and trace event links.
- `MessageSchemaVersion` currently accepts only `.v1` payload schemas and rejects
  malformed or unsupported versions before routing.
- `TraceEventKind` includes message lifecycle variants for future router emission.
- `MessageTraceContext` records identity, schema, run scope, and causal parent in
  message trace events.

The payload remains `serde_json::Value`. The envelope validates only identity,
schema version, timestamp presence through serde, and non-null payload. Typed
payload validation belongs to the schema owner and must produce a
`message.rejected` trace event when it fails.

## Consequences

- Local message router work in 0.02-S2 can use the schema without redefining
  identity or trace semantics.
- Multi-agent replay work in 0.02-S7 can reconstruct causality from preserved
  `causal_parent` and message trace contexts.
- Remote transport work in 0.03 can wrap `MessageEnvelope` without changing local
  message semantics.
- Messages do not grant permissions or authorize side effects; receiving agents
  must still operate under their own permissions, quotas, work-order scope, and
  Action Gateway verification.

## Non-goals

- No message broker.
- No local router, inbox, or outbox implementation.
- No remote transport.
- No distributed delivery guarantee.
- No shared mutable state channel.
- No permission inheritance or local delegation model.

## Compatibility notes

This is the first Rust message schema contract. Future schema versions must use a
new `.vN` suffix and update validation, documentation, and compatibility tests.
Transport-specific wrappers must not alter the canonical message fields.
