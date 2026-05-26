# 0.03-S1 — Distributed identity model

## Objective

Splendor0.03-dev Sprint 0.03-S1 defines the canonical identity model that lets
resident nodes, fleet execution, governance, and physical/edge milestones extend
the local runtime without overloading IDs. The sprint strengthens fleet/node
identity, runtime context, trace store, state graph, action gateway, message,
replay, SDK/API, and docs/test primitives while keeping execution local.

## Functional scope

- Implement distinct Rust identity types for `fleet_id`, `node_id`,
  `instance_id`, `tenant_id`, `agent_id`, `run_id`, `tick_id`, `action_id`,
  `state_node_id`, `trace_event_id`, and `message_id` in `splendor-types`.
- Move canonical `ActionId` and `StateNodeId` to `splendor-types` while keeping
  source-compatible re-exports from gateway/store crates.
- Rename serialized trace identity from `trace_id` to `trace_event_id`, with a
  serde input alias for `trace_id` during 0.02 migration.
- Add `TraceIdentityContext` to trace events so each event can locate run,
  tenant, agent, tick, action, state, message, fleet, node, and instance identity
  when applicable.
- Add `ActionRequest.run_id` and fail-closed gateway identity validation before
  adapter execution.
- Add optional runtime identity linkage to state metadata/commits so committed
  state can reference tenant, agent, run, and trace-event scope.
- Align Python SDK trace/replay serialization with `trace_event_id` and identity
  context fields.

## Non-goals

- No node or instance registry implementation.
- No remote messaging or remote transport.
- No placement engine.
- No distributed consensus, global ID registry, fleet scheduler, or central
  manager.
- No governance approval workflow or physical/edge adapter behavior.

## Public contracts changed

- `docs/reference/identity.md` is the canonical distributed identity reference.
- `splendor_types::FleetId`, `NodeId`, `InstanceId`, `ActionId`, `TickId`,
  `StateNodeId`, `TraceEventId`, `RuntimeIdentityContext`, and
  `TraceIdentityContext` are added or centralized.
- `splendor_types::TraceId` remains only as a compatibility alias for
  `TraceEventId`.
- `TraceEvent` serializes `trace_event_id` and `identity`; deserialization still
  accepts legacy `trace_id`.
- `Message.causal_parent` and message trace links continue to serialize as trace
  event UUID strings; new docs call the type `TraceEventId`.
- `ActionRequest` now includes `run_id` and exposes `validate_identity()`.
- `StateMetadata` and `StateCommit` include optional tenant, agent, run, and
  trace-event linkage.
- `StateNodeId` JSON identity values serialize as `algorithm:digest` strings;
  Rust state graph commits emit BLAKE3 strings and Python local SDK traces emit
  SHA-256 strings in the same shape.
- Python `KernelRuntime` traces now emit `trace_event_id` and identity context.
- No TypeScript package exists in this worktree; the reference doc defines the
  future `@splendor/types` field names that must be used when that package lands.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | changed: action requests carry run identity and fail closed on invalid IDs |
| Verifier | changed indirectly: identity validation runs before verifier/adapter execution |
| State graph | changed: commits can carry tenant/agent/run/trace-event linkage |
| Trace store | changed: trace events carry `trace_event_id` plus identity context |
| Replay | changed: replay validates/round-trips the renamed trace-event identity |
| Message | changed: causal parent semantics are documented as trace-event identity |
| Work order | schema names reserved only; no work-order ingestion implemented |
| Governance | none |

## Trace behavior

- No event class names are changed.
- Every `TraceEvent` now serializes `trace_event_id` instead of legacy
  `trace_id`; the deterministic derivation remains
  `uuid_v5(NAMESPACE_OID, "{run_id}:{sequence}")`.
- Every `TraceEvent` includes `identity.run_id`; runtime emission validates it
  against the owning runtime run before persistence.
- Loop tick events include tenant, agent, run, and tick identity when emitted by
  the loop engine.
- Action verification/execution/denial/failure/outcome events include
  `action_id` when the loop engine has submitted an action.
- State commit events include `state_node_id`.
- Message lifecycle events include `message_id` and preserve `causal_parent`.
- Fleet, node, and instance fields are supported but optional until later 0.03
  registry/transport sprints populate them.

## State behavior

- State node creation, parent linkage, and state-head update semantics remain the
  same.
- `StateMetadata` can store optional `tenant_id`, `agent_id`, `run_id`, and
  `trace_event_id` for runtime-owned commits.
- `StateCommit` echoes the same identity linkage plus `node_id` and optional
  `snapshot_id`.
- `StateNodeId` remains content-addressed from parent IDs plus state data hash;
  metadata linkage does not affect the state-node hash.
- State commit failures still fail the tick before `StateCommitted` and
  `LoopTickCompleted` are emitted.

## Gateway and verifier behavior

- `ActionRequest` now carries `action_id`, `tenant_id`, `agent_id`, and `run_id`
  as distinct typed identities.
- `VerifiedActionGateway` validates those identities before adapter lookup and
  execution.
- Missing or nil action, tenant, agent, or run IDs produce an `ActionOutcome`
  with `status = Denied`, verification reason `identity_invalid`, no output, and
  no adapter execution.
- Existing tenant policy, quota, invariant, adapter mismatch, adapter failure,
  and postcondition behavior remains unchanged.

## Replay behavior

- Replay remains inspect-only and never invokes perceptors, policies, gateways,
  verifiers, adapters, network, filesystem, or external systems.
- Rust replay validates deterministic `trace_event_id` values and still accepts
  persisted 0.02 records that deserialize through the legacy `trace_id` alias.
- Python replay validates run scope and contiguous sequence numbers while
  preserving identity context in copied trace events.
- Replay can reconstruct message causality through message trace contexts and
  `causal_parent` trace-event IDs, but remote causal graphs remain future work.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | Identity constructors, validation, trace serde, state metadata, message schemas | Passed: `cargo test -p splendor-types` via workspace run |
| gateway | Deny invalid action/run/tenant/agent identity before adapter execution | Passed: `cargo test -p splendor-gateway` via workspace run |
| integration | Runtime loop propagates identity through action/state/trace paths | Passed: `cargo test --workspace` |
| negative | Nil/mismatched IDs fail closed in gateway, trace, Python SDK, and router paths | Passed: `cargo test --workspace`; `pytest python/tests` |
| replay | Replay validates renamed trace-event identity and avoids side effects | Passed: `cargo test --workspace`; `pytest python/tests` |
| docs/format/lint | Workspace formatting and lints remain clean | Passed: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings` |

## Example or fixture

Use the existing local examples and test fixtures after rebuilding the workspace:

```bash
cargo test -p splendor-types
cargo test -p splendor-gateway
cargo test --workspace
pytest python/tests
```

The TypeScript identity contract is documented in
[`docs/reference/identity.md`](../../reference/identity.md) because no
`typescript/` package exists yet in this repository checkout.

## Future extension notes

- 0.03-S2 node/instance registry should populate the existing `fleet_id`,
  `node_id`, and `instance_id` fields rather than adding competing identity
  names.
- 0.03-S3 signed work orders should use these exact tenant, agent, run, node,
  instance, and fleet fields for compatibility.
- 0.03-S5 remote messaging may wrap `MessageEnvelope`, but must not alter
  `message_id`, `run_id`, or `causal_parent` semantics.
- 0.03-S6 trace aggregation should preserve `trace_event_id`, `sequence`, and
  `identity` exactly and validate run/instance/fleet continuity.
- 0.03-S7 state handoff should reuse `state_node_id` and state metadata linkage;
  it must not introduce hidden shared mutable state.
