use super::*;
use splendor_store::InMemoryStateStore;
use time::OffsetDateTime;

fn metadata(label: Option<&str>) -> StateMetadata {
    StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: label.map(|value| value.to_string()),
        tenant_id: None,
        agent_id: None,
        run_id: None,
        trace_event_id: None,
    }
}

fn data(bytes: &[u8]) -> StateData {
    StateData {
        bytes: bytes.to_vec(),
        content_type: Some("application/octet-stream".to_string()),
    }
}

#[test]
fn state_graph_commits_and_tracks_head() {
    let store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(store, SnapshotPolicy::default());
    let commit = graph
        .commit(data(&[1, 2, 3]), metadata(None))
        .expect("commit");
    assert_eq!(graph.tick(), 1);
    assert_eq!(graph.head(), Some(&commit.node_id));
    assert!(commit.snapshot_id.is_none());
    assert!(!commit.node_id.to_string().is_empty());
}

#[test]
fn snapshot_policy_interval_triggers() {
    let store = Arc::new(InMemoryStateStore::default());
    let policy = SnapshotPolicy {
        interval: Some(2),
        important_labels: Vec::new(),
    };
    let mut graph = StateGraph::new(store, policy);
    let first = graph.commit(data(&[1]), metadata(None)).expect("commit");
    let second = graph.commit(data(&[2]), metadata(None)).expect("commit");
    assert!(first.snapshot_id.is_none());
    assert!(second.snapshot_id.is_some());
}

#[test]
fn snapshot_policy_label_triggers() {
    let store = Arc::new(InMemoryStateStore::default());
    let policy = SnapshotPolicy {
        interval: None,
        important_labels: vec!["important".to_string()],
    };
    let mut graph = StateGraph::new(store, policy);
    let commit = graph
        .commit(data(&[9]), metadata(Some("important")))
        .expect("commit");
    assert!(commit.snapshot_id.is_some());
}

#[test]
fn state_graph_with_head_keeps_existing_head() {
    let store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(store.clone(), SnapshotPolicy::default());
    let commit = graph.commit(data(&[1]), metadata(None)).expect("commit");
    let head = commit.node_id.clone();
    let graph = StateGraph::with_head(store, Some(head.clone()), SnapshotPolicy::default());
    assert_eq!(graph.head(), Some(&head));
}

#[test]
fn state_graph_restore_snapshot_updates_head() {
    let store = Arc::new(InMemoryStateStore::default());
    let policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let mut graph = StateGraph::new(store, policy);
    let commit = graph
        .commit(data(&[7]), metadata(Some("snap")))
        .expect("commit");
    let snapshot_id = commit.snapshot_id.expect("snapshot id");
    let snapshot = graph
        .restore_snapshot(&snapshot_id)
        .expect("restore snapshot");
    assert_eq!(graph.head(), Some(&snapshot.node_id));
    assert_eq!(snapshot.state.bytes, vec![7]);
}

#[test]
fn state_graph_setters_update_state() {
    let store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(store, SnapshotPolicy::default());
    assert_eq!(graph.tick(), 0);
    graph.set_tick(42);
    assert_eq!(graph.tick(), 42);

    graph.set_head(None);
    assert!(graph.head().is_none());
}
