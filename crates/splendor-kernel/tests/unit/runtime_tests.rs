use super::*;
use splendor_store::{InMemoryTraceStore, TraceStore};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

#[derive(Default)]
struct CapturingSink {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl TraceSink for CapturingSink {
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError> {
        self.events.lock().expect("events lock").push(event.clone());
        Ok(())
    }
}

#[test]
fn boot_emits_kernel_event_and_increments_sequence() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::boot(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    })
    .expect("boot runtime");

    {
        let recorded = events.lock().expect("events lock");
        assert_eq!(recorded.len(), 1);
        assert!(matches!(
            recorded[0].kind,
            TraceEventKind::LoopTickStarted { tick_id: 0 }
        ));
        assert_eq!(recorded[0].sequence, 0);
    }

    let event = runtime
        .record_event(TraceEventKind::LoopTickCompleted {
            tick_id: 0,
            integrity: None,
        })
        .expect("record event");
    assert_eq!(event.sequence, 1);
    if let TraceEventKind::LoopTickCompleted { integrity, .. } = event.kind {
        assert!(integrity.is_some());
    } else {
        panic!("unexpected event kind");
    }

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[1].sequence, 1);
    assert_eq!(recorded[1].run_id, runtime.run_id().clone());
}

#[test]
fn default_config_records_to_stdout_sink() {
    let runtime = KernelRuntime::new(KernelRuntimeConfig::default());
    let event = runtime
        .record_event(TraceEventKind::PolicyInvoked {
            policy: "default".to_string(),
        })
        .expect("record event");
    assert_eq!(event.sequence, 0);
    assert!(matches!(event.kind, TraceEventKind::PolicyInvoked { .. }));
}

#[test]
fn runtime_rejects_mismatched_trace_identity_before_persistence() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingSink {
        events: Arc::clone(&events),
    };
    let run_id = RunId::new();
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        run_id: Some(run_id.clone()),
        ..KernelRuntimeConfig::default()
    });

    let error = runtime
        .record_event_with_identity(
            TraceIdentityContext::new(RunId::new()),
            TraceEventKind::PolicyInvoked {
                policy: "mismatch".to_string(),
            },
        )
        .expect_err("identity mismatch");

    assert!(matches!(error, TraceError::Identity(_)));
    assert_eq!(runtime.run_id(), &run_id);
    assert_eq!(runtime.next_sequence(), 0);
    assert!(events.lock().expect("events lock").is_empty());
}

#[test]
fn runtime_resumes_sequence_with_trace_store() {
    let store = Arc::new(InMemoryTraceStore::default());
    let run_id = RunId::new();
    let event = TraceEvent::new(
        run_id.clone(),
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    let payload = serde_json::to_value(&event).expect("payload");
    let sequence =
        TraceStore::append(store.as_ref(), &run_id.to_string(), payload).expect("append");
    assert_eq!(sequence, 0);

    let runtime = KernelRuntime::with_trace_store(store, Some(run_id.clone())).expect("runtime");
    let next = runtime
        .record_event(TraceEventKind::LoopTickCompleted {
            tick_id: 1,
            integrity: None,
        })
        .expect("record event");
    assert_eq!(runtime.run_id(), &run_id);
    assert_eq!(next.sequence, 1);
    if let TraceEventKind::LoopTickCompleted { integrity, .. } = next.kind {
        assert!(integrity.is_some());
    } else {
        panic!("unexpected event kind");
    }
}

#[test]
fn loop_tick_completed_integrity_matches_trace_store_record() {
    let store = Arc::new(InMemoryTraceStore::default());
    let run_id = RunId::new();
    let runtime =
        KernelRuntime::with_trace_store(store.clone(), Some(run_id.clone())).expect("runtime");
    runtime
        .record_event(TraceEventKind::PolicyInvoked {
            policy: "unit".to_string(),
        })
        .expect("policy event");
    runtime
        .record_event(TraceEventKind::LoopTickCompleted {
            tick_id: 1,
            integrity: None,
        })
        .expect("completed event");

    let records = TraceStore::read(store.as_ref(), &run_id.to_string()).expect("records");
    let completed_record = records.last().expect("completed record");
    let completed: TraceEvent =
        serde_json::from_value(completed_record.payload.clone()).expect("completed event payload");
    if let TraceEventKind::LoopTickCompleted {
        integrity: Some(integrity),
        ..
    } = completed.kind
    {
        assert_eq!(integrity.prev_event_hash, completed_record.prev_event_hash);
        assert_eq!(integrity.event_hash, completed_record.event_hash);
    } else {
        panic!("missing completion integrity");
    }
}
