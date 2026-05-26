use super::*;
use crate::{KernelRuntime, KernelRuntimeConfig, MessageRouter, TraceError, TraceSink};
use splendor_types::{
    AgentId, EndpointScope, Message, MessageDeliveryStatus, MessageEnvelope, MessageId,
    RemoteMessageEnvelope, RemoteMessageRetryPolicy, RevocationStatus, RunId, TenantId, TraceEvent,
    TraceId, WorkOrderAuthorization, WorkOrderSignature,
};
use std::sync::{Arc, Barrier, Mutex};
use time::{Duration, OffsetDateTime};

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

struct FailRemoteDeliveredSink {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl TraceSink for FailRemoteDeliveredSink {
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError> {
        if matches!(event.kind, TraceEventKind::RemoteMessageDelivered { .. }) {
            return Err(TraceError::IntegrityLock);
        }
        self.events.lock().expect("events lock").push(event.clone());
        Ok(())
    }
}

fn runtime_failing_remote_delivered(run_id: RunId) -> (KernelRuntime, Arc<Mutex<Vec<TraceEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(FailRemoteDeliveredSink {
            events: Arc::clone(&events),
        }),
        run_id: Some(run_id),
        ..KernelRuntimeConfig::default()
    });
    (runtime, events)
}

fn runtime_for(run_id: RunId) -> (KernelRuntime, Arc<Mutex<Vec<TraceEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        run_id: Some(run_id),
        ..KernelRuntimeConfig::default()
    });
    (runtime, events)
}

fn work_order(
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: RunId,
    now: OffsetDateTime,
) -> WorkOrderAuthorization {
    WorkOrderAuthorization {
        work_order_id: "wo_remote".to_string(),
        tenant_id,
        agent_id,
        run_id: Some(run_id),
        allowed_scopes: vec![EndpointScope::MessagesSend],
        signature: Some(WorkOrderSignature {
            key_id: "key_remote".to_string(),
            signature: "sig_remote".to_string(),
        }),
        expires_at: now + Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn remote_envelope(
    source: AgentId,
    target: AgentId,
    run_id: RunId,
    tenant_id: TenantId,
    now: OffsetDateTime,
    retry_policy: RemoteMessageRetryPolicy,
) -> RemoteMessageEnvelope {
    let message = Message::new(
        MessageId::new(),
        source,
        target.clone(),
        run_id.clone(),
        "splendor.message.task_request.v1",
        serde_json::json!({"task": "forecast", "input_ref": "dataset:test"}),
        Some(TraceId::from_run_sequence(&run_id, 3)),
        true,
        now,
    )
    .expect("valid message");
    RemoteMessageEnvelope::new(
        tenant_id.clone(),
        "instance_source",
        "instance_target",
        work_order(tenant_id, target, run_id, now),
        MessageEnvelope::new(message).expect("valid envelope"),
        retry_policy,
        now,
        Some(now + Duration::minutes(5)),
    )
    .expect("valid remote envelope")
}

#[test]
fn sends_between_two_instances_with_remote_and_local_trace_events() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (source_runtime, source_events) = runtime_for(run_id.clone());
    let (target_runtime, target_events) = runtime_for(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
    let transport = InMemoryRemoteMessageTransport::new(&receiver, &target_runtime);

    let remote = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Never,
    );
    let message_id = remote.message().message_id.clone();
    let delivered = send_remote_message(&transport, &source_runtime, remote, now)
        .expect("remote message delivered");

    assert_eq!(delivered.delivery_status, MessageDeliveryStatus::Delivered);
    assert_eq!(delivered.message.message_id, message_id);
    let inbox = target_router.inbox(&target_agent, &run_id).expect("inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].message.message_id, message_id);

    let source_recorded = source_events.lock().expect("source events");
    assert_eq!(source_recorded.len(), 1);
    assert!(matches!(
        source_recorded[0].kind,
        TraceEventKind::RemoteMessageSent { .. }
    ));

    let target_recorded = target_events.lock().expect("target events");
    assert_eq!(target_recorded.len(), 3);
    assert!(matches!(
        target_recorded[0].kind,
        TraceEventKind::RemoteMessageAccepted { .. }
    ));
    assert!(matches!(
        target_recorded[1].kind,
        TraceEventKind::MessageDelivered { .. }
    ));
    assert!(matches!(
        target_recorded[2].kind,
        TraceEventKind::RemoteMessageDelivered { .. }
    ));

    for event in target_recorded.iter().chain(source_recorded.iter()) {
        match &event.kind {
            TraceEventKind::RemoteMessageSent { remote_message }
            | TraceEventKind::RemoteMessageAccepted { remote_message }
            | TraceEventKind::RemoteMessageDelivered { remote_message } => {
                assert_eq!(remote_message.message.message_id, message_id);
                assert_eq!(remote_message.message.source_agent_id, source_agent);
                assert_eq!(remote_message.message.target_agent_id, target_agent);
            }
            TraceEventKind::MessageDelivered { message } => {
                assert_eq!(message.message_id, message_id);
                assert_eq!(message.source_agent_id, source_agent);
                assert_eq!(message.target_agent_id, target_agent);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}

#[test]
fn receiver_rejects_wrong_instance_and_does_not_deliver() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (target_runtime, target_events) = runtime_for(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_other", &target_router);
    let remote = remote_envelope(
        source_agent,
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Never,
    );

    let error = receiver
        .accept_at(&target_runtime, remote, now)
        .expect_err("wrong instance rejected");
    assert!(matches!(
        error,
        RemoteMessageTransportError::WrongTargetInstance { .. }
    ));
    assert!(target_router
        .inbox(&target_agent, &run_id)
        .expect("inbox")
        .is_empty());
    let recorded = target_events.lock().expect("events");
    assert_eq!(recorded.len(), 1);
    assert!(matches!(
        recorded[0].kind,
        TraceEventKind::RemoteMessageRejected { .. }
    ));
}

#[test]
fn duplicate_remote_message_records_duplicate_and_delivers_once() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (source_runtime, _source_events) = runtime_for(run_id.clone());
    let (target_runtime, target_events) = runtime_for(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
    let transport = InMemoryRemoteMessageTransport::new(&receiver, &target_runtime);
    let remote = remote_envelope(
        source_agent,
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Never,
    );
    let message_id = remote.message().message_id.clone();

    send_remote_message(&transport, &source_runtime, remote.clone(), now).expect("first delivery");
    let error = send_remote_message(&transport, &source_runtime, remote, now)
        .expect_err("duplicate rejected");

    assert!(matches!(
        error,
        RemoteMessageTransportError::Duplicate { message_id: id } if id == message_id
    ));
    assert_eq!(
        target_router
            .inbox(&target_agent, &run_id)
            .expect("target inbox")
            .len(),
        1
    );
    let recorded = target_events.lock().expect("events");
    assert!(recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RemoteMessageDuplicate { .. })));
}

#[test]
fn concurrent_duplicates_reserve_message_id_atomically() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (target_runtime, target_events) = runtime_for(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
    let remote = remote_envelope(
        source_agent,
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Never,
    );
    let attempts = 8;
    let barrier = Barrier::new(attempts);

    let successes = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..attempts {
            let receiver = &receiver;
            let target_runtime = &target_runtime;
            let remote = remote.clone();
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                barrier.wait();
                receiver.accept_at(target_runtime, remote, now).is_ok()
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread finished"))
            .filter(|success| *success)
            .count()
    });

    assert_eq!(successes, 1);
    assert_eq!(
        target_router
            .inbox(&target_agent, &run_id)
            .expect("target inbox")
            .len(),
        1
    );
    let duplicate_events = target_events
        .lock()
        .expect("events")
        .iter()
        .filter(|event| matches!(event.kind, TraceEventKind::RemoteMessageDuplicate { .. }))
        .count();
    assert_eq!(duplicate_events, attempts - 1);
}

#[test]
fn remote_delivered_trace_failure_fails_closed_without_inbox_mutation() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (source_runtime, _source_events) = runtime_for(run_id.clone());
    let (target_runtime, target_events) = runtime_failing_remote_delivered(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
    let transport = InMemoryRemoteMessageTransport::new(&receiver, &target_runtime);
    let remote = remote_envelope(
        source_agent,
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Never,
    );

    let error = send_remote_message(&transport, &source_runtime, remote, now)
        .expect_err("remote delivered trace failure");
    assert!(matches!(
        error,
        RemoteMessageTransportError::LocalDelivery(MessageRouterError::Trace(_))
    ));
    assert!(target_router
        .inbox(&target_agent, &run_id)
        .expect("target inbox")
        .is_empty());
    let recorded = target_events.lock().expect("events");
    assert!(recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RemoteMessageAccepted { .. })));
    assert!(recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::MessageDelivered { .. })));
    assert!(recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RemoteMessageRejected { .. })));
    assert!(!recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RemoteMessageDelivered { .. })));
}

#[test]
fn transport_failure_is_traced_and_not_silently_dropped() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (source_runtime, source_events) = runtime_for(run_id.clone());
    let (target_runtime, _target_events) = runtime_for(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
    let transport = InMemoryRemoteMessageTransport::with_faults(
        &receiver,
        &target_runtime,
        vec![InMemoryRemoteTransportFault::Failure {
            reason: "connection reset".to_string(),
        }],
    );
    let remote = remote_envelope(
        source_agent,
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Never,
    );

    let error = send_remote_message(&transport, &source_runtime, remote, now)
        .expect_err("transport failure");
    assert!(matches!(
        error,
        RemoteMessageTransportError::TransportFailed { .. }
    ));
    assert!(target_router
        .inbox(&target_agent, &run_id)
        .expect("target inbox")
        .is_empty());
    let recorded = source_events.lock().expect("source events");
    assert_eq!(recorded.len(), 2);
    assert!(matches!(
        recorded[0].kind,
        TraceEventKind::RemoteMessageSent { .. }
    ));
    assert!(matches!(
        recorded[1].kind,
        TraceEventKind::RemoteMessageTransportFailed { .. }
    ));
}

#[test]
fn retry_only_occurs_with_safe_idempotent_policy() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let source_agent = AgentId::new();
    let target_agent = AgentId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (source_runtime, source_events) = runtime_for(run_id.clone());
    let (target_runtime, target_events) = runtime_for(run_id.clone());
    let target_router = LocalMessageRouter::new();
    target_router
        .register_agent(target_agent.clone())
        .expect("target registered");
    let receiver = RemoteMessageReceiver::new("instance_target", &target_router);
    let transport = InMemoryRemoteMessageTransport::with_faults(
        &receiver,
        &target_runtime,
        vec![InMemoryRemoteTransportFault::Timeout {
            reason: "first attempt deadline".to_string(),
        }],
    );
    let remote = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id.clone(),
        now,
        RemoteMessageRetryPolicy::Never,
    );

    let error = send_remote_message(&transport, &source_runtime, remote, now)
        .expect_err("non-idempotent timeout is not retried");
    assert!(matches!(error, RemoteMessageTransportError::Timeout { .. }));
    assert!(target_router
        .inbox(&target_agent, &run_id)
        .expect("inbox")
        .is_empty());

    let transport = InMemoryRemoteMessageTransport::with_faults(
        &receiver,
        &target_runtime,
        vec![InMemoryRemoteTransportFault::Timeout {
            reason: "first attempt deadline".to_string(),
        }],
    );
    let remote = remote_envelope(
        source_agent,
        target_agent.clone(),
        run_id.clone(),
        tenant_id,
        now,
        RemoteMessageRetryPolicy::Idempotent {
            max_attempts: 2,
            idempotency_key: "remote-message-key".to_string(),
        },
    );
    let message_id = remote.message().message_id.clone();

    let delivered = send_remote_message(&transport, &source_runtime, remote, now)
        .expect("idempotent retry delivers on second attempt");
    assert_eq!(delivered.message.message_id, message_id);

    let source_recorded = source_events.lock().expect("source events");
    assert!(source_recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RemoteMessageTimedOut { .. })));
    let sent_attempts = source_recorded
        .iter()
        .filter_map(|event| match &event.kind {
            TraceEventKind::RemoteMessageSent { remote_message } => Some(remote_message.attempt),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sent_attempts, vec![1, 1, 2]);

    let target_recorded = target_events.lock().expect("target events");
    assert!(target_recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RemoteMessageDelivered { .. })));
}
