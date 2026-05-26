# State Graph

The state graph models explicit, versioned state for kernel ticks. Each commit
creates a new `StateNodeId` string derived from the state bytes, parent nodes,
and state data hash. Runtime identity metadata is persisted with the node but is
not part of the state-node hash. A `SnapshotPolicy` determines when snapshots are
created.

## SnapshotPolicy

**Fields**

- `interval` (`Option<u64>`): snapshot every N ticks when set.
- `important_labels` (`Vec<String>`): labels that always trigger snapshots.

**Behavior**

- Snapshot when `tick % interval == 0` (for `interval > 0`).
- Snapshot when `StateMetadata.label` matches any `important_labels` entry.

## StateGraph

**Fields**

- `store` (`Arc<dyn StateStore>`): backing persistence layer.
- `head` (`Option<StateNodeId>`): current head node.
- `tick` (`u64`): commit counter.
- `policy` (`SnapshotPolicy`): snapshot policy applied at commit time.

**Commit sequence**

1. Persist `StateData` via `StateStore::put_state`.
2. Record node using `StateStore::commit_node` with the current head as parent.
3. Increment `tick` and evaluate `SnapshotPolicy`.
4. Optionally create a snapshot via `StateStore::snapshot`.

## StateCommit

**Fields**

- `node_id` (`StateNodeId`): identifier of the committed node.
- `tenant_id` (`Option<TenantId>`): tenant that owns the commit, when known.
- `agent_id` (`Option<AgentId>`): agent that owns the commit, when known.
- `run_id` (`Option<RunId>`): run that produced the commit, when known.
- `trace_event_id` (`Option<TraceEventId>`): trace event that records the commit, when known.
- `snapshot_id` (`Option<SnapshotId>`): snapshot identifier when created.

## StateMetadata

**Fields**

- `created_at` (`OffsetDateTime`): timestamp when the node was created.
- `label` (`Option<String>`): optional label used for snapshots/debugging.
- `tenant_id` (`Option<TenantId>`): tenant linkage for runtime-owned commits.
- `agent_id` (`Option<AgentId>`): agent linkage for runtime-owned commits.
- `run_id` (`Option<RunId>`): run linkage for runtime-owned commits.
- `trace_event_id` (`Option<TraceEventId>`): trace-event linkage when known.

## State handoff v0

0.03-S7 adds explicit handoff helpers to `StateGraph`:

- `export_handoff(snapshot_id, request) -> StateHandoff` builds a versioned
  snapshot handoff envelope from an existing snapshot.
- `import_handoff(handoff, work_order, scope, now, metadata) -> StateCommit`
  validates authority, source trace continuity, previous receiver head, snapshot
  hash, and parent linkage before updating the receiver head.
- `attach_read_only_reference(reference, work_order, scope, now)` records an
  immutable state reference without changing the receiver state head.
- `commit_from_read_only_reference(...)` always fails closed; read-only
  references cannot become mutable parents.

Successful snapshot import creates a receiver-owned state node with the same
content-addressed node ID and snapshot ID as the exported source snapshot. A
failed import leaves the receiver `head` unchanged.

State handoff is documented in detail in
[`state-handoff.md`](state-handoff.md).

## StateGraphError

`StateGraphError::Store` wraps `StateStoreError` failures. State handoff adds
fail-closed errors for incompatible work orders, unsigned/expired/revoked work
orders, stale previous heads, missing trace continuity, invalid modes, and
read-only reference mutation attempts.

## Example

```rust
use splendor_kernel::{SnapshotPolicy, StateGraph};
use splendor_store::{InMemoryStateStore, StateData, StateMetadata};
use std::sync::Arc;
use time::OffsetDateTime;

let store = Arc::new(InMemoryStateStore::default());
let policy = SnapshotPolicy { interval: Some(2), important_labels: vec![] };
let mut graph = StateGraph::new(store, policy);
let data = StateData { bytes: vec![1], content_type: None };
let metadata = StateMetadata::new(OffsetDateTime::now_utc(), None);
let commit = graph.commit(data, metadata).expect("commit");
assert_eq!(graph.head(), Some(&commit.node_id));
```
