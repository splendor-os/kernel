use super::*;
use rusqlite::Connection;
use serde::ser::Error as _;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use tempfile::NamedTempFile;
use uuid::Uuid;

fn block_on<F: Future>(mut future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(raw_waker()) };
    let mut context = Context::from_waker(&waker);
    let mut future = unsafe { Pin::new_unchecked(&mut future) };
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

fn raw_waker() -> RawWaker {
    fn no_op(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        raw_waker()
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
    RawWaker::new(std::ptr::null(), &VTABLE)
}

#[test]
fn state_data_ref_round_trip() {
    let data_ref = StateDataRef::new();
    let uuid = *data_ref.as_uuid();
    let rebuilt = StateDataRef::from(uuid);
    assert_eq!(data_ref.to_string(), rebuilt.to_string());
}

#[test]
fn state_store_commits_and_snapshots() {
    let store = InMemoryStateStore::default();
    let state_data = StateData {
        bytes: vec![1, 2, 3],
        content_type: Some("application/octet-stream".to_string()),
    };
    let data_ref = StateStore::put_state(&store, state_data.clone()).expect("put state");
    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: Some("seed".to_string()),
    };
    let node_id = StateStore::commit_node(&store, Vec::new(), data_ref.clone(), metadata)
        .expect("commit node");
    let node = StateStore::get_node(&store, &node_id).expect("get node");
    let snapshot_id = StateStore::snapshot(&store, &node_id).expect("snapshot");
    let snapshot = StateStore::load_snapshot(&store, &snapshot_id).expect("load snapshot");
    assert!(!node_id.to_string().is_empty());
    assert_eq!(node.id, node_id);
    assert_eq!(node.data_ref, data_ref);
    assert_eq!(snapshot.node_id, node_id);
    assert_eq!(snapshot.state.bytes, state_data.bytes);
    assert_eq!(snapshot.state.content_type, state_data.content_type);

    let data_hash = ContentHash::blake3(&snapshot.state.bytes);
    let hash_input = StateNodeHashInput {
        parent_ids: &[],
        data_hash: &data_hash,
    };
    let encoded = serde_json::to_vec(&hash_input).expect("serialize hash input");
    let expected_hash = ContentHash::blake3(encoded);
    assert_eq!(node_id.hash(), &expected_hash);
    assert_eq!(snapshot_id, SnapshotId::from_bytes(&snapshot.state.bytes));
}

#[test]
fn state_store_error_paths() {
    let store = InMemoryStateStore::default();
    let missing_ref = StateDataRef::new();
    assert!(matches!(
        StateStore::get_state(&store, &missing_ref),
        Err(StateStoreError::MissingState)
    ));

    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: None,
    };
    assert!(matches!(
        StateStore::commit_node(&store, Vec::new(), missing_ref, metadata),
        Err(StateStoreError::MissingState)
    ));

    let missing_node = StateNodeId::from_hash(ContentHash::blake3(b"missing-node"));
    assert!(matches!(
        StateStore::snapshot(&store, &missing_node),
        Err(StateStoreError::MissingNode)
    ));
    assert!(matches!(
        StateStore::get_node(&store, &missing_node),
        Err(StateStoreError::MissingNode)
    ));

    let missing_snapshot = SnapshotId::from_bytes(b"missing-snapshot");
    assert!(matches!(
        StateStore::load_snapshot(&store, &missing_snapshot),
        Err(StateStoreError::MissingSnapshot)
    ));
}

#[test]
fn state_store_serialization_error_message() {
    let error = StateStoreError::Serialization(serde_json::Error::custom("boom"));
    assert!(error.to_string().contains("boom"));
}

#[test]
fn sqlite_store_persists_state() {
    let temp = NamedTempFile::new().expect("temp file");
    let path = temp.path().to_path_buf();
    let store = SqliteStateStore::open(&path).expect("open store");
    let state = StateData {
        bytes: vec![4, 5, 6],
        content_type: Some("application/octet-stream".to_string()),
    };
    let data_ref = StateStore::put_state(&store, state.clone()).expect("put state");
    let stored = StateStore::get_state(&store, &data_ref).expect("get state");
    assert_eq!(stored.bytes, state.bytes);
    assert_eq!(stored.content_type, state.content_type);

    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: Some("sqlite".to_string()),
    };
    let node_id =
        StateStore::commit_node(&store, Vec::new(), data_ref, metadata).expect("commit node");
    let node = StateStore::get_node(&store, &node_id).expect("get node");
    assert_eq!(node.id, node_id);
    let snapshot_id = StateStore::snapshot(&store, &node_id).expect("snapshot");
    let snapshot = StateStore::load_snapshot(&store, &snapshot_id).expect("load snapshot");
    assert_eq!(snapshot.node_id, node_id);
}

#[test]
fn async_state_store_round_trip() {
    let store = InMemoryStateStore::default();
    let state_data = StateData {
        bytes: vec![9],
        content_type: None,
    };
    let data_ref =
        block_on(AsyncStateStore::put_state(&store, state_data.clone())).expect("put state");
    let restored = block_on(AsyncStateStore::get_state(&store, &data_ref)).expect("get state");
    assert_eq!(restored, state_data);
}

#[test]
fn async_inmemory_commit_snapshot_round_trip() {
    let store = InMemoryStateStore::default();
    let state_data = StateData {
        bytes: vec![9, 9],
        content_type: None,
    };
    let data_ref =
        block_on(AsyncStateStore::put_state(&store, state_data.clone())).expect("put state");
    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: Some("snapshot".to_string()),
    };
    let node_id = block_on(AsyncStateStore::commit_node(
        &store,
        Vec::new(),
        data_ref,
        metadata,
    ))
    .expect("commit node");
    let snapshot_id = block_on(AsyncStateStore::snapshot(&store, &node_id)).expect("snapshot");
    let snapshot =
        block_on(AsyncStateStore::load_snapshot(&store, &snapshot_id)).expect("load snapshot");
    assert_eq!(snapshot.state, state_data);
}

#[test]
fn async_sqlite_state_store_round_trip() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteStateStore::open(temp.path()).expect("open store");
    let state_data = StateData {
        bytes: vec![2, 4],
        content_type: None,
    };
    let data_ref =
        block_on(AsyncStateStore::put_state(&store, state_data.clone())).expect("put state");
    let restored = block_on(AsyncStateStore::get_state(&store, &data_ref)).expect("get state");
    assert_eq!(restored, state_data);
}

#[test]
fn sqlite_store_reports_missing_state() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteStateStore::open(temp.path()).expect("open store");
    let missing_ref = StateDataRef::from(Uuid::new_v4());
    assert!(matches!(
        StateStore::get_state(&store, &missing_ref),
        Err(StateStoreError::MissingState)
    ));
}

#[test]
fn sqlite_store_reports_invalid_hash_algorithm() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteStateStore::open(temp.path()).expect("open store");

    let snapshot_id = SnapshotId::from_hash(ContentHash::new(HashAlgorithm::Blake3, "snap"));
    let connection = Connection::open(temp.path()).expect("open connection");
    connection
        .execute(
            "INSERT INTO snapshots (snapshot_algo, snapshot_value, node_hash_algo, node_hash_value) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["blake3", "snap", "bad", "node"],
        )
        .expect("insert snapshot");

    let error = StateStore::load_snapshot(&store, &snapshot_id).expect_err("error");
    assert!(matches!(error, StateStoreError::InvalidHashAlgorithm(_)));
}

#[test]
fn sqlite_store_reports_invalid_data_ref() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteStateStore::open(temp.path()).expect("open store");

    let snapshot_id = SnapshotId::from_hash(ContentHash::new(HashAlgorithm::Blake3, "snap"));
    let connection = Connection::open(temp.path()).expect("open connection");
    connection
        .execute(
            "INSERT INTO state_nodes (node_hash_algo, node_hash_value, parent_ids, data_ref, data_hash_algo, data_hash_value, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "blake3",
                "node",
                "[]",
                "not-a-uuid",
                "blake3",
                "data",
                "{}",
            ],
        )
        .expect("insert node");
    connection
        .execute(
            "INSERT INTO snapshots (snapshot_algo, snapshot_value, node_hash_algo, node_hash_value) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["blake3", "snap", "blake3", "node"],
        )
        .expect("insert snapshot");

    let error = StateStore::load_snapshot(&store, &snapshot_id).expect_err("error");
    assert!(matches!(error, StateStoreError::InvalidDataRef(_)));
}

#[test]
fn async_sqlite_commit_snapshot_round_trip() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteStateStore::open(temp.path()).expect("open store");
    let state_data = StateData {
        bytes: vec![7, 8],
        content_type: None,
    };
    let data_ref =
        block_on(AsyncStateStore::put_state(&store, state_data.clone())).expect("put state");
    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: None,
    };
    let node_id = block_on(AsyncStateStore::commit_node(
        &store,
        Vec::new(),
        data_ref,
        metadata,
    ))
    .expect("commit node");
    let snapshot_id = block_on(AsyncStateStore::snapshot(&store, &node_id)).expect("snapshot");
    let snapshot =
        block_on(AsyncStateStore::load_snapshot(&store, &snapshot_id)).expect("load snapshot");
    assert_eq!(snapshot.state, state_data);
}
