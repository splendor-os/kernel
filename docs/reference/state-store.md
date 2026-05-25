# State Store

State stores persist state bytes, state graph nodes, and snapshots. The storage
interfaces are defined in `crates/splendor-store` and used by the kernel state
graph.

## Data Types

| Type            | Purpose                                                      |
| --------------- | ------------------------------------------------------------ |
| `StateData`     | Serialized state bytes with an optional `content_type`.      |
| `StateDataRef`  | UUID reference for stored state bytes.                       |
| `StateNodeId`   | Content hash identifying a state graph node.                 |
| `StateMetadata` | Metadata stored with each state node.                        |
| `StateNode`     | Node payload containing parents, data reference, and hashes. |
| `StateSnapshot` | Snapshot payload containing node ID and `StateData`.         |

## StateStore

Synchronous storage interface:

```
put_state(StateData) -> StateDataRef
get_state(StateDataRef) -> StateData
commit_node(parent_ids, data_ref, metadata) -> StateNodeId
snapshot(StateNodeId) -> SnapshotId
load_snapshot(SnapshotId) -> StateSnapshot
```

## AsyncStateStore

Async wrapper with the same semantics, returning futures for each operation.

## InMemoryStateStore

In-memory store intended for tests and local runs. Data is lost when the process
terminates.

## SqliteStateStore

SQLite-backed store that persists state data and snapshots on disk. The schema
contains:

- `state_data`: raw state bytes keyed by `StateDataRef`.
- `state_nodes`: serialized parent IDs, data refs, data hash, and metadata.
- `snapshots`: snapshot hash to node hash mapping.

## Example

```rust
use splendor_store::{SqliteStateStore, StateData, StateMetadata, StateStore};
use tempfile::NamedTempFile;
use time::OffsetDateTime;

let temp = NamedTempFile::new().expect("temp");
let store = SqliteStateStore::open(temp.path()).expect("open");
let data = StateData { bytes: vec![1, 2, 3], content_type: None };
let data_ref = StateStore::put_state(&store, data).expect("put");
let metadata = StateMetadata { created_at: OffsetDateTime::now_utc(), label: Some("seed".into()) };
let node_id = StateStore::commit_node(&store, Vec::new(), data_ref, metadata).expect("commit");
let snapshot_id = StateStore::snapshot(&store, &node_id).expect("snapshot");
let snapshot = StateStore::load_snapshot(&store, &snapshot_id).expect("load");
assert_eq!(snapshot.node_id, node_id);
```
