use super::*;
use serde::ser::Error as _;
use splendor_store::{InMemoryTraceStore, TraceRecord, TraceStore, TraceStoreError};
use splendor_types::{RunId, TraceEventKind};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use time::OffsetDateTime;

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
fn trace_sink_records_event() {
    let event = TraceEvent::new(
        RunId::new(),
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    let sink = StdoutTraceSink;
    assert!(TraceSink::record(&sink, &event).is_ok());
    let async_result = block_on(AsyncTraceSink::record(&sink, &event));
    assert!(async_result.is_ok());
}

#[test]
fn trace_error_displays_context() {
    let error = TraceError::Serialization(serde_json::Error::custom("boom"));
    let message = error.to_string();
    assert!(message.contains("boom"));
}

#[test]
fn trace_store_sink_records_event() {
    let store = Arc::new(InMemoryTraceStore::default());
    let run_id = RunId::new();
    let sink = TraceStoreSink::new(run_id.clone(), store.clone());
    let event = TraceEvent::new(
        run_id.clone(),
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    TraceSink::record(&sink, &event).expect("record event");

    let records = TraceStore::read(store.as_ref(), &run_id.to_string()).expect("read traces");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sequence, 0);
    let restored: TraceEvent = serde_json::from_value(records[0].payload.clone()).expect("payload");
    assert_eq!(restored.sequence, 0);
    assert_eq!(restored.run_id, run_id);
}

#[test]
fn trace_store_sink_latest_sequence_none_for_missing_run() {
    let store = Arc::new(InMemoryTraceStore::default());
    let run_id = RunId::new();
    let sink = TraceStoreSink::new(run_id, store);
    let latest = sink.latest_sequence().expect("latest sequence");
    assert!(latest.is_none());
}

#[test]
fn trace_store_sink_reports_sequence_mismatch() {
    struct MismatchTraceStore;

    impl TraceStore for MismatchTraceStore {
        fn append(
            &self,
            _run_id: &str,
            _payload: serde_json::Value,
        ) -> Result<u64, TraceStoreError> {
            Ok(7)
        }

        fn read(&self, _run_id: &str) -> Result<Vec<TraceRecord>, TraceStoreError> {
            Err(TraceStoreError::RunNotFound)
        }

        fn read_range(
            &self,
            _run_id: &str,
            _start: u64,
            _end: u64,
        ) -> Result<Vec<TraceRecord>, TraceStoreError> {
            Err(TraceStoreError::RunNotFound)
        }
    }

    let store = Arc::new(MismatchTraceStore);
    let run_id = RunId::new();
    let sink = TraceStoreSink::new(run_id.clone(), store);
    let event = TraceEvent::new(
        run_id,
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    let error = TraceSink::record(&sink, &event).expect_err("sequence mismatch");
    match error {
        TraceError::SequenceMismatch { expected, actual } => {
            assert_eq!(expected, 0);
            assert_eq!(actual, 7);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn trace_store_sink_reports_store_errors() {
    struct ErrorTraceStore;

    impl TraceStore for ErrorTraceStore {
        fn append(
            &self,
            _run_id: &str,
            _payload: serde_json::Value,
        ) -> Result<u64, TraceStoreError> {
            Ok(0)
        }

        fn read(&self, _run_id: &str) -> Result<Vec<TraceRecord>, TraceStoreError> {
            Err(TraceStoreError::Poisoned)
        }

        fn read_range(
            &self,
            _run_id: &str,
            _start: u64,
            _end: u64,
        ) -> Result<Vec<TraceRecord>, TraceStoreError> {
            Err(TraceStoreError::Poisoned)
        }
    }

    let run_id = RunId::new();
    let sink = TraceStoreSink::new(run_id.clone(), Arc::new(ErrorTraceStore));
    assert_eq!(sink.run_id(), &run_id);

    let error = sink.latest_sequence().expect_err("store error");
    assert!(matches!(
        error,
        TraceError::Store(TraceStoreError::Poisoned)
    ));

    let error = sink.latest_event_hash().expect_err("store error");
    assert!(matches!(
        error,
        TraceError::Store(TraceStoreError::Poisoned)
    ));
}
