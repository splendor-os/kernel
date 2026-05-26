use super::*;
use crate::{InMemoryTraceStore, TraceStore};
use splendor_types::{
    Action, ContentHash, RunId, SideEffectClass, TraceEvent, TraceEventKind, VerificationResult,
};
use time::OffsetDateTime;

fn scope_for(run_id: &RunId) -> TraceSyncScope {
    TraceSyncScope::new(run_id.to_string())
}

fn append_event(store: &InMemoryTraceStore, run_id: &RunId, sequence: u64, kind: TraceEventKind) {
    let event = TraceEvent::new(run_id.clone(), sequence, OffsetDateTime::now_utc(), kind);
    let stored = TraceStore::append(
        store,
        &run_id.to_string(),
        serde_json::to_value(event).unwrap(),
    )
    .expect("append event");
    assert_eq!(stored, sequence);
}

fn append_basic_run(store: &InMemoryTraceStore, run_id: &RunId) {
    append_event(
        store,
        run_id,
        0,
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    append_event(
        store,
        run_id,
        1,
        TraceEventKind::LoopTickCompleted {
            tick_id: 1,
            integrity: None,
        },
    );
    append_event(
        store,
        run_id,
        2,
        TraceEventKind::LoopTickStarted { tick_id: 2 },
    );
}

fn action(name: &str) -> Action {
    Action {
        name: name.to_string(),
        params: serde_json::json!({"path": "out.txt"}),
        side_effect_class: SideEffectClass::Filesystem,
        cost_estimate: None,
        required_permissions: vec!["fs.write".to_string()],
        preconditions: vec![],
        postconditions: vec![],
    }
}

#[test]
fn local_buffer_sync_preserves_order_across_partial_sync() {
    let run_id = RunId::new();
    let local = InMemoryTraceStore::default();
    append_basic_run(&local, &run_id);
    let index = InMemoryCentralTraceIndex::default();

    let first = TraceSyncBatch::from_store(scope_for(&run_id), &local, 0, 2).expect("batch");
    let first_report = index.sync_batch(first).expect("sync first batch");
    assert_eq!(first_report.accepted_records, 2);

    let second = TraceSyncBatch::from_store(scope_for(&run_id), &local, 2, 3).expect("batch");
    let second_report = index.sync_batch(second).expect("sync second batch");
    assert_eq!(second_report.accepted_records, 1);

    let records = index
        .query(&TraceIndexQuery {
            run_id: Some(run_id.to_string()),
            ..TraceIndexQuery::default()
        })
        .expect("query");
    let sequences = records
        .iter()
        .map(|record| record.record.sequence)
        .collect::<Vec<_>>();
    assert_eq!(sequences, vec![0, 1, 2]);
}

#[test]
fn duplicate_sync_attempts_are_idempotent() {
    let run_id = RunId::new();
    let local = InMemoryTraceStore::default();
    append_basic_run(&local, &run_id);
    let index = InMemoryCentralTraceIndex::default();
    let batch = TraceSyncBatch::from_store(scope_for(&run_id), &local, 0, 2).expect("batch");

    let first = index.sync_batch(batch.clone()).expect("sync first");
    let second = index.sync_batch(batch).expect("sync duplicate");

    assert_eq!(first.accepted_records, 2);
    assert_eq!(first.duplicate_records, 0);
    assert_eq!(second.accepted_records, 0);
    assert_eq!(second.duplicate_records, 2);
    assert_eq!(index.latest_sequence(&run_id.to_string()).unwrap(), Some(1));
    assert_eq!(
        index
            .query(&TraceIndexQuery {
                run_id: Some(run_id.to_string()),
                ..TraceIndexQuery::default()
            })
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn missing_segments_are_reported_clearly() {
    let run_id = RunId::new();
    let local = InMemoryTraceStore::default();
    append_basic_run(&local, &run_id);
    let index = InMemoryCentralTraceIndex::default();
    let batch = TraceSyncBatch::from_store(scope_for(&run_id), &local, 1, 2).expect("batch");

    let error = index.sync_batch(batch).expect_err("missing segment");

    assert!(matches!(
        error,
        TraceSyncError::MissingSegment {
            expected_sequence: 0,
            actual_sequence: 1,
            ..
        }
    ));
    assert!(index.quarantined().expect("quarantine").is_empty());
}

#[test]
fn corrupted_trace_chain_is_rejected_and_quarantined() {
    let run_id = RunId::new();
    let local = InMemoryTraceStore::default();
    append_basic_run(&local, &run_id);
    let mut batch = TraceSyncBatch::from_store(scope_for(&run_id), &local, 0, 2).expect("batch");
    batch.records[1].prev_event_hash = Some(ContentHash::blake3(b"wrong-prev"));
    let index = InMemoryCentralTraceIndex::default();

    let error = index.sync_batch(batch).expect_err("corrupted chain");

    assert!(matches!(error, TraceSyncError::ChainMismatch { .. }));
    let quarantine = index.quarantined().expect("quarantine");
    assert_eq!(quarantine.len(), 1);
    assert!(quarantine[0].reason.contains("trace chain mismatch"));
}

#[test]
fn mismatched_run_identity_is_rejected_and_quarantined() {
    let record_run_id = RunId::new();
    let scope_run_id = RunId::new();
    let local = InMemoryTraceStore::default();
    append_basic_run(&local, &record_run_id);
    let batch =
        TraceSyncBatch::from_store(scope_for(&scope_run_id), &local, 0, 1).unwrap_or_else(|_| {
            TraceSyncBatch {
                scope: scope_for(&scope_run_id),
                records: TraceStore::read_range(&local, &record_run_id.to_string(), 0, 1)
                    .expect("range"),
            }
        });
    let index = InMemoryCentralTraceIndex::default();

    let error = index.sync_batch(batch).expect_err("run mismatch");

    assert!(matches!(error, TraceSyncError::RunIdentityMismatch { .. }));
    assert_eq!(index.quarantined().expect("quarantine").len(), 1);
}

#[test]
fn central_index_queries_available_identity_dimensions() {
    let run_id = RunId::new();
    let local = InMemoryTraceStore::default();
    append_event(
        &local,
        &run_id,
        0,
        TraceEventKind::LoopTickStarted { tick_id: 7 },
    );
    append_event(
        &local,
        &run_id,
        1,
        TraceEventKind::ActionVerificationStarted {
            action: action("file.write"),
        },
    );
    append_event(
        &local,
        &run_id,
        2,
        TraceEventKind::ActionVerificationCompleted {
            action: action("file.write"),
            result: VerificationResult::allow(),
        },
    );

    let scope = TraceSyncScope {
        fleet_id: Some("fleet-alpha".to_string()),
        node_id: Some("node-a".to_string()),
        instance_id: Some("instance-a".to_string()),
        tenant_id: Some("tenant-a".to_string()),
        agent_id: Some("agent-a".to_string()),
        run_id: run_id.to_string(),
        work_order_id: Some("wo-a".to_string()),
    };
    let batch = TraceSyncBatch::from_store(scope, &local, 0, 3).expect("batch");
    let index = InMemoryCentralTraceIndex::default();
    index.sync_batch(batch).expect("sync");

    let by_identity_and_tick = index
        .query(&TraceIndexQuery {
            fleet_id: Some("fleet-alpha".to_string()),
            node_id: Some("node-a".to_string()),
            instance_id: Some("instance-a".to_string()),
            tenant_id: Some("tenant-a".to_string()),
            agent_id: Some("agent-a".to_string()),
            run_id: Some(run_id.to_string()),
            tick_id: Some(7),
            work_order_id: Some("wo-a".to_string()),
            ..TraceIndexQuery::default()
        })
        .expect("query tick");
    assert_eq!(by_identity_and_tick.len(), 1);
    assert_eq!(by_identity_and_tick[0].record.sequence, 0);

    let by_action = index
        .query(&TraceIndexQuery {
            action: Some("file.write".to_string()),
            ..TraceIndexQuery::default()
        })
        .expect("query action");
    assert_eq!(by_action.len(), 2);

    let by_work_order = index
        .query(&TraceIndexQuery {
            work_order_id: Some("wo-a".to_string()),
            ..TraceIndexQuery::default()
        })
        .expect("query work order");
    assert_eq!(by_work_order.len(), 3);
}
