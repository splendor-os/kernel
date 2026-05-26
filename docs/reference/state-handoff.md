# State Handoff

State handoff v0 is the explicit state-transfer primitive for 0.03-S7. It moves
state through validated snapshots or attaches read-only references. It is **not**
shared mutable memory, a migration engine, a CRDT, or a conflict-resolution
system.

## Purpose

State handoff lets a receiving Splendor instance resume or inspect state without
implicitly sharing a mutable state head. The receiver either:

- imports a snapshot after validating authority, hash, parent linkage, previous
  head, and source trace continuity; or
- attaches a read-only reference that cannot become a mutable parent.

## Schemas

Rust schemas live in `splendor-types::state_handoff` and are re-exported from
`splendor-types` and `splendor-kernel`.

### `StateHandoff`

Important fields:

- `schema_version`: currently `splendor.state_handoff.v0`.
- `handoff_id`: trace-linking identifier for source and receiver events.
- `mode`: `snapshot_import` for ownership transfer.
- `authority`: tenant, agent, run, and work-order binding.
- `previous_state_node_id`: receiver head expected before import.
- `snapshot`: exported snapshot bytes, hash, snapshot ID, source node ID, and
  parent node IDs.
- `source_trace_id`: source trace event that proves the export boundary.

### `StateReference`

`StateReference` uses `mode = read_only_reference`. Attaching it records the
source node and authority binding but does not update the receiver state head.
`StateGraph::commit_from_read_only_reference` always fails closed.

### `StateHandoffTraceContext`

Trace context carries `handoff_id`, mode, tenant/agent/run IDs, work-order ID,
source and receiver instance IDs, source state node, previous receiver head,
receiver state node after import, snapshot ID, and source trace ID.

## Lifecycle

1. Source commits state and creates a snapshot through the state graph.
2. Source calls `StateGraph::export_handoff` for that snapshot.
3. Source records `state.handoff.exported` through
   `KernelRuntime::record_state_handoff_exported`, which writes the source trace
   ID back into the handoff envelope.
4. Receiver validates signed work-order authority, expected tenant/agent/run,
   previous head, source trace continuity, snapshot ID, state hash, and node
   parent linkage.
5. Receiver imports bytes into its own store and updates its head only after all
   validation succeeds.
6. Receiver records `state.handoff.imported` or `state.handoff.import_failed`.

## Trace events

Canonical event classes:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `StateHandoffExported` | `state.handoff.exported` | Source exported a snapshot handoff. |
| `StateHandoffImported` | `state.handoff.imported` | Receiver imported a validated snapshot. |
| `StateHandoffImportFailed` | `state.handoff.import_failed` | Receiver failed closed before changing head. |
| `ReadOnlyStateReferenced` | `state.reference.read_only` | Receiver attached a read-only reference. |

## Failure modes

Imports fail closed when:

- `schema_version` is not `splendor.state_handoff.v0`;
- work-order signature metadata is missing;
- work order is expired or revoked;
- tenant, agent, run, work-order ID, or required scope is incompatible;
- source trace ID is missing;
- receiver current head does not match `previous_state_node_id`;
- snapshot ID does not match exported bytes;
- state hash does not match exported bytes;
- source node ID does not match parent IDs plus state hash;
- the state store cannot persist the imported node or snapshot.

A failed import leaves the receiver state head unchanged.

## Security notes

`StateGraph::import_handoff` requires a signed `WorkOrderAuthorization` with
`EndpointScope::RunsResume`. `StateGraph::attach_read_only_reference` requires
`EndpointScope::StateRead`. Caller credentials and daemon endpoint checks remain
separate from this primitive; a daemon token alone does not authorize state
import or side effects.

## Replay behavior

Replay is inspect-only. `splendorctl replay` recognizes handoff trace events and
emits a `handoff_boundary` JSON line with the previous receiver state head and
receiver imported head, when present. Replay does not import state, re-run
policies, call gateways, or execute adapters.

## Compatibility notes

This is a 0.03-dev v0 schema. Fields are explicit and versioned so later
migration, state fork/merge, and cross-instance scheduling can extend the
primitive without introducing hidden shared mutable state.
