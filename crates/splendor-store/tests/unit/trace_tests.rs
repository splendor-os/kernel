use super::*;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use tempfile::NamedTempFile;

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
fn trace_store_append_and_read() {
    let store = InMemoryTraceStore::default();
    let sequence =
        TraceStore::append(&store, "run-1", serde_json::json!({"ok": true})).expect("append");
    assert_eq!(sequence, 0);

    let records = TraceStore::read(&store, "run-1").expect("read");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sequence, 0);
    assert!(records[0].prev_event_hash.is_none());

    let range = TraceStore::read_range(&store, "run-1", 0, 1).expect("range");
    assert_eq!(range.len(), 1);
}

#[test]
fn trace_store_chains_hashes() {
    let store = InMemoryTraceStore::default();
    TraceStore::append(&store, "run-1", serde_json::json!({"step": 1})).expect("append");
    TraceStore::append(&store, "run-1", serde_json::json!({"step": 2})).expect("append");
    let records = TraceStore::read(&store, "run-1").expect("read");
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[1].prev_event_hash,
        Some(records[0].event_hash.clone())
    );
}

#[test]
fn trace_store_missing_run() {
    let store = InMemoryTraceStore::default();
    assert!(matches!(
        TraceStore::read(&store, "missing"),
        Err(TraceStoreError::RunNotFound)
    ));
    assert!(matches!(
        TraceStore::read_range(&store, "missing", 0, 1),
        Err(TraceStoreError::RunNotFound)
    ));
}

#[test]
fn sqlite_trace_store_persists_records() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 1})).expect("append");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 2})).expect("append");
    let records = TraceStore::read(&store, "run-1").expect("read");
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[1].prev_event_hash,
        Some(records[0].event_hash.clone())
    );
}

#[test]
fn sqlite_trace_store_missing_run() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    assert!(matches!(
        TraceStore::read(&store, "missing"),
        Err(TraceStoreError::RunNotFound)
    ));
}

#[test]
fn sqlite_trace_store_read_range_missing_run() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    assert!(matches!(
        TraceStore::read_range(&store, "missing", 0, 1),
        Err(TraceStoreError::RunNotFound)
    ));
}

#[test]
fn sqlite_trace_store_read_range_empty_for_existing_run() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 1})).expect("append");
    let records = TraceStore::read_range(&store, "run-1", 2, 2).expect("range");
    assert!(records.is_empty());
}

#[test]
fn sqlite_trace_store_read_range_returns_records() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 1})).expect("append");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 2})).expect("append");

    let records = TraceStore::read_range(&store, "run-1", 0, 2).expect("range");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].sequence, 0);
}

#[test]
fn optional_hash_parts_rejects_partial_values() {
    let error = SqliteTraceStore::optional_hash_from_parts(Some("blake3".to_string()), None)
        .expect_err("error");
    assert!(matches!(error, TraceStoreError::InvalidHashParts { .. }));

    let error = SqliteTraceStore::optional_hash_from_parts(None, Some("value".to_string()))
        .expect_err("error");
    assert!(matches!(error, TraceStoreError::InvalidHashParts { .. }));
}

#[test]
fn optional_hash_parts_none_returns_none() {
    let value = SqliteTraceStore::optional_hash_from_parts(None, None).expect("ok");
    assert!(value.is_none());
}

#[test]
fn parse_algorithm_rejects_unknown() {
    let error = SqliteTraceStore::parse_algorithm("unknown").expect_err("error");
    assert!(matches!(error, TraceStoreError::InvalidHashAlgorithm(_)));
}

#[test]
fn decode_timestamp_rejects_invalid_value() {
    let error = decode_timestamp("not-a-timestamp").expect_err("error");
    assert!(matches!(error, TraceStoreError::InvalidTimestamp(_)));
}

#[test]
fn sequence_encoding_and_decoding_errors() {
    let error = encode_sequence(u64::MAX).expect_err("error");
    assert!(matches!(error, TraceStoreError::SequenceOverflow(_)));

    let error = decode_sequence(-1).expect_err("error");
    assert!(matches!(error, TraceStoreError::InvalidSequence(-1)));
}

#[test]
fn async_trace_store_round_trip() {
    let store = InMemoryTraceStore::default();
    let sequence = block_on(AsyncTraceStore::append(
        &store,
        "async-run",
        serde_json::json!({"ok": true}),
    ))
    .expect("append");
    assert_eq!(sequence, 0);

    let records = block_on(AsyncTraceStore::read(&store, "async-run")).expect("read");
    assert_eq!(records.len(), 1);

    let range = block_on(AsyncTraceStore::read_range(&store, "async-run", 0, 1)).expect("range");
    assert_eq!(range.len(), 1);
}

#[test]
fn async_sqlite_trace_store_round_trip() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    let sequence = block_on(AsyncTraceStore::append(
        &store,
        "async-run",
        serde_json::json!({"ok": true}),
    ))
    .expect("append");
    assert_eq!(sequence, 0);
    let records = block_on(AsyncTraceStore::read(&store, "async-run")).expect("read");
    assert_eq!(records.len(), 1);
}

#[test]
fn async_sqlite_trace_store_read_range() {
    let temp = NamedTempFile::new().expect("temp");
    let store = SqliteTraceStore::open(temp.path()).expect("open");
    block_on(AsyncTraceStore::append(
        &store,
        "async-run",
        serde_json::json!({"event": 1}),
    ))
    .expect("append");

    let records = block_on(AsyncTraceStore::read_range(&store, "async-run", 0, 1)).expect("range");
    assert_eq!(records.len(), 1);
}

#[test]
fn trace_hash_normalizes_loop_tick_completed_integrity() {
    let payload_with_integrity = serde_json::json!({
        "kind": {
            "LoopTickCompleted": {
                "tick_id": 1,
                "integrity": {
                    "prev_event_hash": "blake3:abc",
                    "event_hash": "blake3:def"
                }
            }
        }
    });
    let payload_without_integrity = serde_json::json!({
        "kind": {
            "LoopTickCompleted": {
                "tick_id": 1
            }
        }
    });

    let store_with = InMemoryTraceStore::default();
    let sequence =
        TraceStore::append(&store_with, "run-1", payload_with_integrity).expect("append");
    let record_with = TraceStore::read(&store_with, "run-1").expect("read");
    assert_eq!(sequence, 0);

    let store_without = InMemoryTraceStore::default();
    TraceStore::append(&store_without, "run-2", payload_without_integrity).expect("append");
    let record_without = TraceStore::read(&store_without, "run-2").expect("read");

    assert_eq!(record_with[0].event_hash, record_without[0].event_hash);
}
