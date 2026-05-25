# RFC 0001 — ChildRunLinked Trace Event

## Motivation

Sprint 0.02-S7 requires multi-agent replay to show local parent/child run
relationships without executing child side effects. The current message lifecycle
trace events preserve message causality, but they do not name a child run. A
small explicit trace event is needed so replay can inspect delegation identity
without inventing a scheduler or remote transport.

## Primitive affected

- Trace store
- Replay
- Message causality
- Runtime context identity

## Schema/API proposal

Add `TraceEventKind::ChildRunLinked`:

```rust
ChildRunLinked {
    parent_run_id: RunId,
    child_run_id: RunId,
    parent_agent_id: AgentId,
    child_agent_id: AgentId,
    causal_parent: Option<TraceId>,
    source_message_id: Option<MessageId>,
}
```

The enclosing `TraceEvent.run_id` is the parent run stream and must match
`parent_run_id`. The event records identity and causality only. It does not start,
resume, schedule, authorize, or execute a child run.

## Migration plan

No existing trace event is renamed or reinterpreted. Older traces simply lack
`ChildRunLinked` events and replay reports an empty `parent_child_runs` array.
New traces that include the event remain append-only and sequence ordered.

## Compatibility impact

This is an additive 0.02-dev trace schema extension. Consumers that deserialize
`TraceEventKind` exhaustively must add a `ChildRunLinked` arm. The repository
tests were updated where exhaustive matching existed.

The stable 0.1 schema line is not yet frozen. This RFC records the dev-milestone
compatibility note required by `AGENTS.md` for public schema additions.

## Security impact

The event does not grant permissions or carry credentials. It must not be treated
as authorization to execute the child run. Replay reports the relationship with
`side_effects_replayed: false` and never invokes gateways, verifiers, adapters,
or child run execution from this event.

## Trace/replay impact

Replay uses the event to populate `parent_child_runs` in tick output and the
final `causal_graph`. Replay fails closed if `parent_run_id` does not match the
enclosing trace event `run_id`.

## Tests required

- Serialization round trip for `ChildRunLinked`.
- Replay output includes parent/child run IDs and agent IDs.
- Replay output marks child links `side_effects_replayed: false`.
- Replay rejects a child link whose parent run does not match the enclosing trace
  run.

## Docs required

- `docs/reference/trace-events.md`
- `docs/reference/replay.md`
- `docs/reference/multi-agent-replay.md`
- `docs/milestones/0.02-dev/S7-multi-agent-replay-test-harness.md`
- `examples/local-multi-agent-replay/README.md`
