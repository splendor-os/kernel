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
fn rejects_empty_gap_conflict_payload_and_hash_mismatches_without_mutation() {
    let run_id = RunId::new();

    let empty_index = InMemoryCentralTraceIndex::default();
    let empty_error = empty_index
        .sync_batch(TraceSyncBatch {
            scope: scope_for(&run_id),
            records: Vec::new(),
        })
        .expect_err("empty sync batch is rejected");
    assert!(matches!(empty_error, TraceSyncError::EmptyBatch { .. }));
    assert!(empty_index.quarantined().expect("quarantine").is_empty());

    let local_with_gap = InMemoryTraceStore::default();
    append_basic_run(&local_with_gap, &run_id);
    let mut gap_batch =
        TraceSyncBatch::from_store(scope_for(&run_id), &local_with_gap, 0, 3).expect("gap batch");
    gap_batch.records.remove(1);
    let gap_index = InMemoryCentralTraceIndex::default();
    let gap_error = gap_index
        .sync_batch(gap_batch)
        .expect_err("in-batch sequence gap is rejected");
    assert!(matches!(
        gap_error,
        TraceSyncError::MissingSegment {
            expected_sequence: 1,
            actual_sequence: 2,
            ..
        }
    ));
    assert!(gap_index.quarantined().expect("quarantine").is_empty());

    let conflict_index = InMemoryCentralTraceIndex::default();
    let conflict_first = InMemoryTraceStore::default();
    append_event(
        &conflict_first,
        &run_id,
        0,
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    conflict_index
        .sync_batch(
            TraceSyncBatch::from_store(scope_for(&run_id), &conflict_first, 0, 1)
                .expect("first conflict batch"),
        )
        .expect("first conflict sync");
    let conflict_second = InMemoryTraceStore::default();
    append_event(
        &conflict_second,
        &run_id,
        0,
        TraceEventKind::LoopTickCompleted {
            tick_id: 1,
            integrity: None,
        },
    );
    let conflict_error = conflict_index
        .sync_batch(
            TraceSyncBatch::from_store(scope_for(&run_id), &conflict_second, 0, 1)
                .expect("second conflict batch"),
        )
        .expect_err("conflicting central sequence is rejected");
    assert!(matches!(
        conflict_error,
        TraceSyncError::CentralConflict { .. }
    ));
    assert_eq!(conflict_index.quarantined().expect("quarantine").len(), 1);

    let payload_index = InMemoryCentralTraceIndex::default();
    let payload_store = InMemoryTraceStore::default();
    append_event(
        &payload_store,
        &run_id,
        0,
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    let mut payload_batch = TraceSyncBatch::from_store(scope_for(&run_id), &payload_store, 0, 1)
        .expect("payload batch");
    payload_batch.records[0].payload["run_id"] = serde_json::json!(RunId::new().to_string());
    let payload_error = payload_index
        .sync_batch(payload_batch)
        .expect_err("payload run mismatch is rejected");
    assert!(matches!(
        payload_error,
        TraceSyncError::PayloadRunIdentityMismatch { .. }
    ));
    assert_eq!(payload_index.quarantined().expect("quarantine").len(), 1);

    let hash_index = InMemoryCentralTraceIndex::default();
    let hash_store = InMemoryTraceStore::default();
    append_event(
        &hash_store,
        &run_id,
        0,
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    let mut hash_batch =
        TraceSyncBatch::from_store(scope_for(&run_id), &hash_store, 0, 1).expect("hash batch");
    hash_batch.records[0].event_hash = ContentHash::blake3(b"wrong-event-hash");
    let hash_error = hash_index
        .sync_batch(hash_batch)
        .expect_err("event hash mismatch is rejected");
    assert!(matches!(hash_error, TraceSyncError::HashMismatch { .. }));
    assert_eq!(hash_index.quarantined().expect("quarantine").len(), 1);
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

    let by_nonmatching_action_id = index
        .query(&TraceIndexQuery {
            action_id: Some("action-id-not-present".to_string()),
            ..TraceIndexQuery::default()
        })
        .expect("query nonmatching action id");
    assert!(by_nonmatching_action_id.is_empty());
}
