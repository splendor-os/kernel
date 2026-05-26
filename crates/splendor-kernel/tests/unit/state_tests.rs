use super::*;
use splendor_store::InMemoryStateStore;
use splendor_types::{TraceId, WorkOrderSignature};
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

fn authority(tenant_id: TenantId, agent_id: AgentId, run_id: RunId) -> StateHandoffAuthority {
    StateHandoffAuthority {
        tenant_id,
        agent_id,
        run_id,
        work_order_id: "wo_state".to_string(),
    }
}

fn scope(tenant_id: TenantId, agent_id: AgentId, run_id: RunId) -> StateHandoffScope {
    StateHandoffScope {
        tenant_id,
        agent_id,
        run_id,
    }
}

fn work_order(
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: RunId,
    scopes: Vec<EndpointScope>,
    now: OffsetDateTime,
) -> WorkOrderAuthorization {
    WorkOrderAuthorization {
        work_order_id: "wo_state".to_string(),
        tenant_id,
        agent_id,
        run_id: Some(run_id),
        allowed_scopes: scopes,
        signature: Some(WorkOrderSignature {
            key_id: "key_state".to_string(),
            signature: "sig_state".to_string(),
        }),
        expires_at: now + time::Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn exported_handoff(
    previous_state_node_id: Option<String>,
    source_trace: bool,
) -> (StateHandoff, TenantId, AgentId, RunId, OffsetDateTime) {
    let now = OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(
        store,
        SnapshotPolicy {
            interval: Some(1),
            important_labels: Vec::new(),
        },
    );
    let commit = graph
        .commit(data(&[4, 5, 6]), metadata(Some("source")))
        .expect("source commit");
    let snapshot_id = commit.snapshot_id.expect("snapshot");
    let source_trace_id = source_trace.then(|| TraceId::from_run_sequence(&run_id, 1));
    let handoff = graph
        .export_handoff(
            &snapshot_id,
            StateHandoffExportRequest {
                handoff_id: "handoff_unit".to_string(),
                authority: authority(tenant_id.clone(), agent_id.clone(), run_id.clone()),
                source_instance_id: Some("source_instance".to_string()),
                receiver_instance_id: Some("receiver_instance".to_string()),
                previous_state_node_id,
                source_trace_id,
                created_at: now,
            },
        )
        .expect("export");
    (handoff, tenant_id, agent_id, run_id, now)
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

#[test]
fn state_graph_imports_valid_handoff_with_work_order_authority() {
    let (handoff, tenant_id, agent_id, run_id, now) = exported_handoff(None, true);
    let store = Arc::new(InMemoryStateStore::default());
    let mut receiver = StateGraph::new(store, SnapshotPolicy::default());
    let work_order = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    let scope = scope(tenant_id, agent_id, run_id);

    let commit = receiver
        .import_handoff(&handoff, &work_order, &scope, now, metadata(Some("import")))
        .expect("import");

    assert_eq!(receiver.head(), Some(&commit.node_id));
    assert_eq!(commit.node_id.to_string(), handoff.snapshot.state_node_id);
    assert_eq!(commit.snapshot_id, Some(handoff.snapshot.snapshot_id));
}

#[test]
fn state_graph_rejects_mismatched_handoff_authority() {
    let (handoff, tenant_id, agent_id, run_id, now) = exported_handoff(None, true);
    let store = Arc::new(InMemoryStateStore::default());
    let mut receiver = StateGraph::new(store, SnapshotPolicy::default());
    let work_order = work_order(
        tenant_id.clone(),
        agent_id,
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    let wrong_scope = scope(tenant_id, AgentId::new(), run_id);

    let error = receiver
        .import_handoff(&handoff, &work_order, &wrong_scope, now, metadata(None))
        .expect_err("authority denial");

    assert!(matches!(error, StateGraphError::IncompatibleWorkOrder));
    assert!(receiver.head().is_none());
}

#[test]
fn state_graph_rejects_invalid_handoff_work_orders_and_schema() {
    let (handoff, tenant_id, agent_id, run_id, now) = exported_handoff(None, true);
    let scope = scope(tenant_id.clone(), agent_id.clone(), run_id.clone());

    let mut unsigned = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    unsigned.signature = None;
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    assert!(matches!(
        receiver.import_handoff(&handoff, &unsigned, &scope, now, metadata(None)),
        Err(StateGraphError::UnsignedWorkOrder)
    ));

    let mut expired = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    expired.expires_at = now;
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    assert!(matches!(
        receiver.import_handoff(&handoff, &expired, &scope, now, metadata(None)),
        Err(StateGraphError::ExpiredWorkOrder)
    ));

    let mut revoked = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    revoked.revocation = RevocationStatus::Revoked {
        reason: "test revocation".to_string(),
    };
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    assert!(matches!(
        receiver.import_handoff(&handoff, &revoked, &scope, now, metadata(None)),
        Err(StateGraphError::RevokedWorkOrder { .. })
    ));

    let wrong_scope = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::StateRead],
        now,
    );
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    assert!(matches!(
        receiver.import_handoff(&handoff, &wrong_scope, &scope, now, metadata(None)),
        Err(StateGraphError::IncompatibleWorkOrder)
    ));

    let mut wrong_work_order = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    wrong_work_order.work_order_id = "wo_other".to_string();
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    assert!(matches!(
        receiver.import_handoff(&handoff, &wrong_work_order, &scope, now, metadata(None)),
        Err(StateGraphError::IncompatibleWorkOrder)
    ));

    let valid = work_order(
        tenant_id,
        agent_id,
        run_id,
        vec![EndpointScope::RunsResume],
        now,
    );
    let mut unsupported = handoff.clone();
    unsupported.schema_version = "splendor.state_handoff.v999".to_string();
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    assert!(matches!(
        receiver.import_handoff(&unsupported, &valid, &scope, now, metadata(None)),
        Err(StateGraphError::UnsupportedHandoffSchema { .. })
    ));
}

#[test]
fn state_graph_rejects_stale_handoff_head() {
    let (handoff, tenant_id, agent_id, run_id, now) =
        exported_handoff(Some("blake3:not-current".to_string()), true);
    let store = Arc::new(InMemoryStateStore::default());
    let mut receiver = StateGraph::new(store, SnapshotPolicy::default());
    let work_order = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    let scope = scope(tenant_id, agent_id, run_id);

    let error = receiver
        .import_handoff(&handoff, &work_order, &scope, now, metadata(None))
        .expect_err("stale head");

    assert!(matches!(error, StateGraphError::StaleStateHead { .. }));
    assert!(receiver.head().is_none());
}

#[test]
fn state_graph_failed_handoff_import_leaves_receiver_head_unchanged() {
    let store = Arc::new(InMemoryStateStore::default());
    let mut receiver = StateGraph::new(store, SnapshotPolicy::default());
    let existing = receiver
        .commit(data(&[9]), metadata(Some("existing")))
        .expect("existing");
    let existing_head = existing.node_id.clone();
    let (mut handoff, tenant_id, agent_id, run_id, now) =
        exported_handoff(Some(existing_head.to_string()), true);
    handoff.snapshot.state_bytes.push(99);
    let work_order = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    let scope = scope(tenant_id, agent_id, run_id);

    let error = receiver
        .import_handoff(&handoff, &work_order, &scope, now, metadata(None))
        .expect_err("corrupt snapshot");

    assert!(matches!(error, StateGraphError::Store(_)));
    assert_eq!(receiver.head(), Some(&existing_head));
}

#[test]
fn state_graph_rejects_handoff_without_source_trace() {
    let (handoff, tenant_id, agent_id, run_id, now) = exported_handoff(None, false);
    let store = Arc::new(InMemoryStateStore::default());
    let mut receiver = StateGraph::new(store, SnapshotPolicy::default());
    let work_order = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::RunsResume],
        now,
    );
    let scope = scope(tenant_id, agent_id, run_id);

    let error = receiver
        .import_handoff(&handoff, &work_order, &scope, now, metadata(None))
        .expect_err("missing trace");

    assert!(matches!(error, StateGraphError::MissingTraceContinuity));
    assert!(receiver.head().is_none());
}

#[test]
fn read_only_state_reference_cannot_be_mutated_by_receiver() {
    let now = OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let reference = StateReference {
        reference_id: "ref_read_only".to_string(),
        mode: StateReferenceMode::ReadOnlyReference,
        authority: authority(tenant_id.clone(), agent_id.clone(), run_id.clone()),
        state_node_id: "blake3:source".to_string(),
        snapshot_id: None,
        state_hash: None,
        source_trace_id: Some(TraceId::from_run_sequence(&run_id, 2)),
        created_at: now,
    };
    let store = Arc::new(InMemoryStateStore::default());
    let mut receiver = StateGraph::new(store, SnapshotPolicy::default());
    let work_order = work_order(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        vec![EndpointScope::StateRead],
        now,
    );
    let scope = scope(tenant_id, agent_id, run_id);

    receiver
        .attach_read_only_reference(reference, &work_order, &scope, now)
        .expect("attach");
    let before = receiver.head().cloned();
    let error = receiver
        .commit_from_read_only_reference("ref_read_only", data(&[1]), metadata(None))
        .expect_err("read only");

    assert!(matches!(
        error,
        StateGraphError::ReadOnlyReferenceMutationDenied { .. }
    ));
    assert_eq!(receiver.read_only_references().len(), 1);
    assert_eq!(receiver.head().cloned(), before);
}
