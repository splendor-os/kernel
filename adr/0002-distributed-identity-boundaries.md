# ADR 0002: Distributed Identity Boundaries

## Status

Accepted for Splendor0.03-dev Sprint 0.03-S1.

## Context

Splendor 0.03 begins the resident-node and fleet execution foundation. Before
implementing node registration, signed work orders, remote messages, trace
aggregation, or state handoff, the kernel needs a stable identity model that
prevents one runtime ID from representing multiple concepts.

The sprint criteria require:

- distinct schema/documentation identities for fleet, node, instance, tenant,
  agent, run, tick, action, state, trace, and message;
- no runtime path using one ID to represent multiple concepts;
- trace events carrying enough identity context to locate fleet, node, instance,
  run, agent, tick, and action when applicable;
- invalid, missing, or mismatched identity failing before execution;
- stable serialization across Rust, Python, and future TypeScript surfaces;
- migration notes for changes from 0.02 identifiers.

## Decision

Define the canonical identity surface in `crates/splendor-types` and reuse it
across runtime, gateway, store, message, trace, Python, and future client schema
surfaces:

- UUID-backed distinct types: `FleetId`, `NodeId`, `InstanceId`, `TenantId`,
  `AgentId`, `RunId`, `ActionId`, `MessageId`, and `TraceEventId`.
- Counter-backed `TickId` scoped within a run.
- Content-addressed `StateNodeId` backed by `ContentHash`.
- `TraceId` remains a Rust compatibility alias for `TraceEventId` only.
- Trace events serialize `trace_event_id` and carry a `TraceIdentityContext` with
  optional fleet, node, instance, tenant, agent, tick, action, state, and message
  fields plus required run identity.
- The action gateway validates action, tenant, agent, and run IDs before adapter
  lookup/execution and denies invalid identity with `identity_invalid`.
- State metadata/commit objects carry optional runtime identity linkage without
  changing state-node hash derivation.

No global ID registry, node registry, remote transport, or placement dependency
is introduced in this sprint. Later fleet features may populate the optional
identity fields but must not redefine them.

## Consequences

- Runtime code cannot accidentally pass an `AgentId` where a `RunId`, `ActionId`,
  `StateNodeId`, or `MessageId` is expected at Rust type boundaries.
- Trace aggregation and replay have stable fields for locating the event's run,
  tick, action, state, message, and future fleet/node/instance scope.
- Gateway denial for malformed or nil identity happens before adapters can
  execute side effects.
- State commits can be audited back to tenant, agent, run, and trace-event scope
  without making metadata part of the state-node content hash.
- Python and future TypeScript clients must use the serialized field names in
  `docs/reference/identity.md`.

## Non-goals

- No node or instance registry.
- No remote message transport.
- No placement engine.
- No signed work-order validation.
- No distributed consensus or global identity allocation service.
- No governance approval or physical/edge behavior.

## Compatibility notes

- `TraceEvent.trace_id` is renamed to `trace_event_id`. Rust deserialization
  accepts legacy `trace_id` as an alias; new output uses `trace_event_id`.
- `ActionId` is centralized in `splendor-types`; `splendor-gateway` re-exports it
  for source compatibility.
- `StateNodeId` is centralized in `splendor-types`; `splendor-store` re-exports
  it for source compatibility.
- `StateNodeId` JSON identity serialization changes to the canonical
  `algorithm:digest` string shape; existing structural content-hash JSON is not
  the 0.03 identity contract.
- `ActionRequest` adds required `run_id`.
- `TraceEvent` adds required `identity`.
- `StateMetadata` and `StateCommit` add optional identity linkage fields.

Any future public schema change to these names requires an RFC or compatibility
note because later fleet, governance, physical/edge, and TypeScript surfaces will
depend on this identity contract.
