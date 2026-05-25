# State Graph

The state graph models explicit, versioned state for kernel ticks. Each commit
creates a new `StateNodeId` derived from the state bytes, parent nodes, and
metadata. A `SnapshotPolicy` determines when snapshots are created.

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
- `snapshot_id` (`Option<SnapshotId>`): snapshot identifier when created.

## StateGraphError

`StateGraphError::Store` wraps `StateStoreError` failures.

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
let metadata = StateMetadata { created_at: OffsetDateTime::now_utc(), label: None };
let commit = graph.commit(data, metadata).expect("commit");
assert_eq!(graph.head(), Some(&commit.node_id));
```
