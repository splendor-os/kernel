use super::*;
use crate::AGENT_ISOLATION_LEDGER_SOURCE;
use crate::{KernelRuntimeConfig, TraceSink};
use splendor_types::{
    DelegatedAuthority, Message, MessageSchemaVersion, MessageTraceLinks, TaskRequest, TenantId,
    TraceEvent, TASK_REQUEST_SCHEMA,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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

struct FailingSink;

impl TraceSink for FailingSink {
    fn record(&self, _event: &TraceEvent) -> Result<(), TraceError> {
        Err(TraceError::IntegrityLock)
    }
}

struct FailOnSecondSink {
    events: Arc<Mutex<Vec<TraceEvent>>>,
    attempts: AtomicUsize,
}

impl TraceSink for FailOnSecondSink {
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            self.events.lock().expect("events lock").push(event.clone());
            return Ok(());
        }
        Err(TraceError::IntegrityLock)
    }
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

fn envelope(
    source_agent_id: AgentId,
    target_agent_id: AgentId,
    run_id: RunId,
    sequence: u64,
    created_at: OffsetDateTime,
) -> MessageEnvelope {
    let causal_parent = TraceEventId::from_run_sequence(&run_id, sequence);
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target_agent_id.clone(),
        "forecast",
        DelegatedAuthority::empty(),
    )
    .expect("valid task request");
    let mut payload = serde_json::to_value(task).expect("task payload");
    payload["sequence"] = serde_json::json!(sequence);
    let message = Message::new(
        MessageId::new(),
        source_agent_id,
        target_agent_id,
        run_id,
        TASK_REQUEST_SCHEMA,
        payload,
        Some(causal_parent),
        true,
        created_at,
    )
    .expect("valid message");
    MessageEnvelope::new(message).expect("valid envelope")
}

fn envelope_with_schema(
    source_agent_id: AgentId,
    target_agent_id: AgentId,
    run_id: RunId,
    schema: &str,
    created_at: OffsetDateTime,
) -> MessageEnvelope {
    let payload = if schema == TASK_REQUEST_SCHEMA {
        serde_json::to_value(
            TaskRequest::new(
                run_id.clone(),
                RunId::new(),
                target_agent_id.clone(),
                "forecast",
                DelegatedAuthority::empty(),
            )
            .expect("valid task request"),
        )
        .expect("task request payload")
    } else {
        serde_json::json!({"task": "forecast"})
    };
    let message = Message::new(
        MessageId::new(),
        source_agent_id,
        target_agent_id,
        run_id,
        schema,
        payload,
        None,
        true,
        created_at,
    )
    .expect("valid message");
    MessageEnvelope::new(message).expect("valid envelope")
}

fn allow_messages(router: &LocalMessageRouter, source: &AgentId, recipients: &[AgentId]) {
    router
        .register_agent_with_policy(
            source.clone(),
            AgentIsolationPolicy {
                allowed_message_schemas: vec![TASK_REQUEST_SCHEMA.to_string()],
                allowed_message_recipients: recipients.to_vec(),
                ..AgentIsolationPolicy::default()
            },
        )
        .expect("source message policy");
}

fn invalid_schema_envelope(
    source_agent_id: AgentId,
    target_agent_id: AgentId,
    run_id: RunId,
    created_at: OffsetDateTime,
) -> MessageEnvelope {
    let message = Message {
        message_id: MessageId::new(),
        source_agent_id,
        target_agent_id,
        run_id,
        schema: "splendor.message.task_request.v2".to_string(),
        payload: serde_json::json!({"task": "forecast"}),
        causal_parent: None,
        requires_response: false,
        created_at,
    };
    MessageEnvelope {
        message,
        schema_version: MessageSchemaVersion::V1,
        delivery_status: MessageDeliveryStatus::Pending,
        trace_links: MessageTraceLinks::default(),
    }
}

#[test]
fn routes_message_only_to_target_and_traces_delivery_and_consumption() {
    let source = AgentId::new();
    let target = AgentId::new();
    let unrelated = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");
    router.register_agent(unrelated.clone()).expect("unrelated");

    let submitted = envelope(source.clone(), target.clone(), run_id.clone(), 7, now);
    let causal_parent = submitted.message.causal_parent.clone();
    let message_id = submitted.message.message_id.clone();
    let delivered = router.send_at(&runtime, submitted, now).expect("delivered");

    assert_eq!(delivered.delivery_status, MessageDeliveryStatus::Delivered);
    assert!(delivered.trace_links.queued_trace_id.is_some());
    assert!(delivered.trace_links.delivered_trace_id.is_some());
    assert!(router
        .inbox(&source, &run_id)
        .expect("source inbox")
        .is_empty());
    assert!(router
        .inbox(&unrelated, &run_id)
        .expect("unrelated inbox")
        .is_empty());

    let target_inbox = router.inbox(&target, &run_id).expect("target inbox");
    assert_eq!(target_inbox.len(), 1);
    assert_eq!(target_inbox[0].message.message_id, message_id);
    assert_eq!(
        target_inbox[0].delivery_status,
        MessageDeliveryStatus::Delivered
    );

    let consumed = router
        .consume_next_at(&runtime, &target, &run_id, now)
        .expect("consume")
        .expect("message present");
    assert_eq!(consumed.delivery_status, MessageDeliveryStatus::Consumed);
    assert_eq!(consumed.message.message_id, message_id);
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 3);
    assert_eq!(recorded[0].run_id, run_id);
    assert!(matches!(
        recorded[0].kind,
        TraceEventKind::MessageQueued { .. }
    ));
    assert!(matches!(
        recorded[1].kind,
        TraceEventKind::MessageDelivered { .. }
    ));
    assert!(matches!(
        recorded[2].kind,
        TraceEventKind::MessageConsumed { .. }
    ));
    for event in recorded.iter() {
        match &event.kind {
            TraceEventKind::MessageQueued { message }
            | TraceEventKind::MessageDelivered { message }
            | TraceEventKind::MessageConsumed { message } => {
                assert_eq!(message.message_id, message_id);
                assert_eq!(message.source_agent_id, source);
                assert_eq!(message.target_agent_id, target);
                assert_eq!(message.causal_parent, causal_parent);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}

#[test]
fn remote_inbound_delivers_without_local_source_outbox_and_keeps_hook_trace_linked() {
    let remote_source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    router.register_agent(target.clone()).expect("target");

    let first = router
        .deliver_remote_inbound_at(
            &runtime,
            envelope(
                remote_source.clone(),
                target.clone(),
                run_id.clone(),
                1,
                now,
            ),
            now,
        )
        .expect("remote inbound delivery");
    assert_eq!(first.delivery_status, MessageDeliveryStatus::Delivered);
    assert!(first.trace_links.queued_trace_id.is_none());
    assert!(first.trace_links.delivered_trace_id.is_some());

    let hook_observation = Arc::new(Mutex::new(None));
    let hook_observation_for_closure = Arc::clone(&hook_observation);
    let second = router
        .deliver_remote_inbound_with_before_enqueue_at(
            &runtime,
            envelope(
                remote_source.clone(),
                target.clone(),
                run_id.clone(),
                2,
                now,
            ),
            now,
            move |delivered| {
                *hook_observation_for_closure
                    .lock()
                    .expect("hook observation lock") = Some((
                    delivered.delivery_status,
                    delivered.trace_links.delivered_trace_id.clone(),
                    delivered.trace_links.queued_trace_id.clone(),
                ));
                Ok(())
            },
        )
        .expect("remote inbound delivery with hook");
    assert_eq!(second.delivery_status, MessageDeliveryStatus::Delivered);

    let observed = hook_observation
        .lock()
        .expect("hook observation lock")
        .clone()
        .expect("hook ran");
    assert_eq!(observed.0, MessageDeliveryStatus::Delivered);
    assert!(observed.1.is_some());
    assert!(observed.2.is_none());

    let target_inbox = router.inbox(&target, &run_id).expect("target inbox");
    assert_eq!(target_inbox.len(), 2);
    assert_eq!(target_inbox[0].message.message_id, first.message.message_id);
    assert_eq!(
        target_inbox[1].message.message_id,
        second.message.message_id
    );

    let outbox_error = router
        .outbox(&remote_source, &run_id)
        .expect_err("remote source is not a local outbox owner");
    assert!(matches!(outbox_error, MessageRouterError::UnknownAgent(id) if id == remote_source));

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 2);
    assert!(recorded
        .iter()
        .all(|event| matches!(event.kind, TraceEventKind::MessageDelivered { .. })));
    assert!(!recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::MessageQueued { .. })));
}

#[test]
fn remote_inbound_rejections_trace_without_enqueueing() {
    let remote_source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());

    let invalid_router = LocalMessageRouter::new();
    invalid_router
        .register_agent(target.clone())
        .expect("target");
    let invalid_error = invalid_router
        .deliver_remote_inbound_at(
            &runtime,
            invalid_schema_envelope(remote_source.clone(), target.clone(), run_id.clone(), now),
            now,
        )
        .expect_err("invalid remote message rejected");
    assert!(matches!(
        invalid_error,
        MessageRouterError::InvalidMessage { .. }
    ));
    assert!(invalid_router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let expired_router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_message_age: Some(Duration::seconds(1)),
        ..MessageRouterConfig::default()
    });
    expired_router
        .register_agent(target.clone())
        .expect("target");
    let expired_error = expired_router
        .deliver_remote_inbound_at(
            &runtime,
            envelope(
                remote_source.clone(),
                target.clone(),
                run_id.clone(),
                1,
                OffsetDateTime::UNIX_EPOCH,
            ),
            now,
        )
        .expect_err("expired remote message rejected");
    assert!(matches!(expired_error, MessageRouterError::Expired { .. }));
    assert!(expired_router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let unknown_router = LocalMessageRouter::new();
    let unknown_error = unknown_router
        .deliver_remote_inbound_at(
            &runtime,
            envelope(
                remote_source.clone(),
                target.clone(),
                run_id.clone(),
                2,
                now,
            ),
            now,
        )
        .expect_err("unknown remote target rejected");
    assert!(matches!(unknown_error, MessageRouterError::UnknownTargetAgent(id) if id == target));

    let full_router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_inbox_messages: 0,
        ..MessageRouterConfig::default()
    });
    full_router.register_agent(target.clone()).expect("target");
    let full_error = full_router
        .deliver_remote_inbound_at(
            &runtime,
            envelope(remote_source, target.clone(), run_id.clone(), 3, now),
            now,
        )
        .expect_err("full target inbox rejected");
    assert!(
        matches!(full_error, MessageRouterError::InboxFull { agent_id, limit: 0 } if agent_id == target)
    );
    assert!(full_router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 4);
    assert!(recorded.iter().all(|event| matches!(
        event.kind,
        TraceEventKind::MessageRejected { .. } | TraceEventKind::MessageExpired { .. }
    )));
}

#[test]
fn rejects_unknown_target_with_trace_and_no_delivery() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    router.register_agent(source.clone()).expect("source");

    let submitted = envelope(source.clone(), target.clone(), run_id.clone(), 1, now);
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("unknown target rejected");
    assert!(matches!(error, MessageRouterError::UnknownTargetAgent(id) if id == target));
    assert!(router
        .inbox(&source, &run_id)
        .expect("source inbox")
        .is_empty());
    assert!(router
        .outbox(&source, &run_id)
        .expect("source outbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 1);
    match &recorded[0].kind {
        TraceEventKind::MessageRejected { message, reason } => {
            assert_eq!(message.source_agent_id, source);
            assert!(reason.contains("target agent"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rejects_invalid_schema_before_delivery() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    router.register_agent(source.clone()).expect("source");
    router.register_agent(target.clone()).expect("target");

    let submitted = invalid_schema_envelope(source, target.clone(), run_id.clone(), now);
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("invalid schema rejected");
    assert!(matches!(error, MessageRouterError::InvalidMessage { .. }));
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 1);
    match &recorded[0].kind {
        TraceEventKind::MessageRejected { reason, .. } => {
            assert!(reason.contains("unsupported"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rejects_disallowed_message_schema_with_agent_ledger_trace() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    router
        .register_agent_with_policy(
            source.clone(),
            AgentIsolationPolicy {
                allowed_message_schemas: vec!["splendor.message.approved.v1".to_string()],
                allowed_message_recipients: vec![target.clone()],
                ..AgentIsolationPolicy::default()
            },
        )
        .expect("source policy");
    router.register_agent(target.clone()).expect("target");

    let submitted = envelope(source.clone(), target.clone(), run_id.clone(), 1, now);
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("schema rejected");
    assert!(matches!(
        error,
        MessageRouterError::MessageDenied { agent_id, .. } if agent_id == source
    ));
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 1);
    match &recorded[0].kind {
        TraceEventKind::MessageRejected { message, reason } => {
            assert_eq!(message.source_agent_id, source);
            assert_eq!(message.target_agent_id, target);
            assert!(reason.contains(AGENT_ISOLATION_LEDGER_SOURCE));
            assert!(reason.contains("message_schema_not_allowed"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rejects_disallowed_message_recipient_with_agent_ledger_trace() {
    let source = AgentId::new();
    let allowed = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&allowed));
    router.register_agent(allowed).expect("allowed target");
    router.register_agent(target.clone()).expect("target");

    let submitted = envelope_with_schema(
        source.clone(),
        target.clone(),
        run_id.clone(),
        TASK_REQUEST_SCHEMA,
        now,
    );
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("recipient rejected");
    assert!(matches!(
        error,
        MessageRouterError::MessageDenied { agent_id, .. } if agent_id == source
    ));
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 1);
    match &recorded[0].kind {
        TraceEventKind::MessageRejected { message, reason } => {
            assert_eq!(message.source_agent_id, source);
            assert_eq!(message.target_agent_id, target);
            assert!(reason.contains(AGENT_ISOLATION_LEDGER_SOURCE));
            assert!(reason.contains("message_recipient_not_allowed"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rejects_when_target_inbox_quota_is_exceeded() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_inbox_messages: 1,
        ..MessageRouterConfig::default()
    });
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    router
        .send_at(
            &runtime,
            envelope(source.clone(), target.clone(), run_id.clone(), 1, now),
            now,
        )
        .expect("first delivery");
    let error = router
        .send_at(
            &runtime,
            envelope(source, target.clone(), run_id.clone(), 2, now),
            now,
        )
        .expect_err("second delivery rejected");

    assert!(
        matches!(error, MessageRouterError::InboxFull { agent_id, limit: 1 } if agent_id == target)
    );
    assert_eq!(
        router.inbox(&target, &run_id).expect("target inbox").len(),
        1
    );
    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 3);
    assert!(matches!(
        recorded[0].kind,
        TraceEventKind::MessageQueued { .. }
    ));
    assert!(matches!(
        recorded[1].kind,
        TraceEventKind::MessageDelivered { .. }
    ));
    assert!(matches!(
        recorded[2].kind,
        TraceEventKind::MessageRejected { .. }
    ));
}

#[test]
fn expires_stale_message_before_delivery() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let created_at = OffsetDateTime::UNIX_EPOCH;
    let now = created_at + Duration::seconds(2);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_message_age: Some(Duration::seconds(1)),
        ..MessageRouterConfig::default()
    });
    router.register_agent(source.clone()).expect("source");
    router.register_agent(target.clone()).expect("target");

    let submitted = envelope(source, target.clone(), run_id.clone(), 1, created_at);
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("expired before delivery");
    assert!(matches!(error, MessageRouterError::Expired { .. }));
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 1);
    match &recorded[0].kind {
        TraceEventKind::MessageExpired { reason, .. } => {
            assert!(reason.as_ref().expect("reason").contains("max_message_age"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn preserves_fifo_order_within_source_target_run_stream() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, _events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    for sequence in 1..=3 {
        router
            .send_at(
                &runtime,
                envelope(
                    source.clone(),
                    target.clone(),
                    run_id.clone(),
                    sequence,
                    now,
                ),
                now,
            )
            .expect("delivery");
    }

    let inbox = router.inbox(&target, &run_id).expect("target inbox");
    let sequences = inbox
        .iter()
        .map(|envelope| {
            envelope.message.payload["sequence"]
                .as_u64()
                .expect("sequence")
        })
        .collect::<Vec<_>>();
    assert_eq!(sequences, vec![1, 2, 3]);
}

#[test]
fn inbox_reads_do_not_mutate_unrelated_agent_mailboxes() {
    let source = AgentId::new();
    let target = AgentId::new();
    let unrelated = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, _events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, &[target.clone(), unrelated.clone()]);
    router.register_agent(target.clone()).expect("target");
    router.register_agent(unrelated.clone()).expect("unrelated");

    router
        .send_at(
            &runtime,
            envelope(source.clone(), target.clone(), run_id.clone(), 1, now),
            now,
        )
        .expect("target delivery");
    router
        .send_at(
            &runtime,
            envelope(source, unrelated.clone(), run_id.clone(), 2, now),
            now,
        )
        .expect("unrelated delivery");

    let before = router.mailbox(&unrelated, &run_id).expect("before");
    let target_inbox = router.inbox(&target, &run_id).expect("target inbox");
    let after = router.mailbox(&unrelated, &run_id).expect("after");

    assert_eq!(target_inbox.len(), 1);
    assert_eq!(before, after);
}

#[test]
fn router_denial_emits_only_rejection_without_policy_or_adapter_trace() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    router.register_agent(source.clone()).expect("source");

    let submitted = envelope(source, target, run_id, 1, now);
    let _ = router
        .send_at(&runtime, submitted, now)
        .expect_err("unknown target rejected");

    let recorded = events.lock().expect("events lock");
    assert_eq!(recorded.len(), 1);
    assert!(matches!(
        recorded[0].kind,
        TraceEventKind::MessageRejected { .. }
    ));
    assert!(!recorded.iter().any(|event| matches!(
        event.kind,
        TraceEventKind::PolicyInvoked { .. }
            | TraceEventKind::PolicyCompleted { .. }
            | TraceEventKind::ActionVerificationStarted { .. }
            | TraceEventKind::ActionVerificationCompleted { .. }
            | TraceEventKind::ActionExecuted { .. }
            | TraceEventKind::ActionDenied { .. }
            | TraceEventKind::ActionFailed { .. }
    )));
}

#[test]
fn trace_failure_fails_closed_without_enqueueing_message() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(FailingSink),
        run_id: Some(run_id.clone()),
        ..KernelRuntimeConfig::default()
    });
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    let submitted = envelope(source, target.clone(), run_id.clone(), 1, now);
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("trace failure");
    assert!(matches!(error, MessageRouterError::Trace(_)));
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());
}

#[test]
fn delivered_trace_failure_fails_closed_without_enqueueing_message() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let events = Arc::new(Mutex::new(Vec::new()));
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(FailOnSecondSink {
            events: Arc::clone(&events),
            attempts: AtomicUsize::new(0),
        }),
        run_id: Some(run_id.clone()),
        ..KernelRuntimeConfig::default()
    });
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    let submitted = envelope(source, target.clone(), run_id.clone(), 1, now);
    let error = router
        .send_at(&runtime, submitted, now)
        .expect_err("delivered trace failure");

    assert!(matches!(error, MessageRouterError::Trace(_)));
    assert!(router
        .inbox(&target, &run_id)
        .expect("target inbox")
        .is_empty());
    assert_eq!(events.lock().expect("events lock").len(), 1);
}

#[test]
fn consume_position_revalidates_expected_message_id() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, _events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");
    router
        .send_at(
            &runtime,
            envelope(source, target.clone(), run_id.clone(), 1, now),
            now,
        )
        .expect("delivery");

    let expected_id = MessageId::new();
    let error = router
        .consume_position(&runtime, &target, &run_id, 0, Some(&expected_id), now)
        .expect_err("wrong expected message");

    assert!(
        matches!(error, MessageRouterError::MessageNotVisible { message_id, .. } if message_id == expected_id)
    );
    assert_eq!(router.inbox(&target, &run_id).expect("inbox").len(), 1);
}

#[test]
fn default_methods_and_agent_context_registration_work() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::now_utc();
    let (runtime, _events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::default();
    let source_context = AgentContext::new(source.clone(), TenantId::new(), {
        let mut config = crate::AgentRuntimeConfig::default();
        config.isolation.allowed_message_schemas = vec![TASK_REQUEST_SCHEMA.to_string()];
        config.isolation.allowed_message_recipients = vec![target.clone()];
        config
    });
    let target_context = AgentContext::new(
        target.clone(),
        TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    router
        .register_agent_context(&source_context)
        .expect("source context");
    router
        .register_agent_context(&target_context)
        .expect("target context");

    let first = router
        .send(
            &runtime,
            envelope(source.clone(), target.clone(), run_id.clone(), 1, now),
        )
        .expect("send default clock");
    let first_id = first.message.message_id.clone();
    let consumed = router
        .consume(&runtime, &target, &run_id, &first_id)
        .expect("consume specific default clock");
    assert_eq!(consumed.delivery_status, MessageDeliveryStatus::Consumed);

    router
        .send(
            &runtime,
            envelope(source, target.clone(), run_id.clone(), 2, now),
        )
        .expect("second send");
    let consumed = router
        .consume_next(&runtime, &target, &run_id)
        .expect("consume next default clock")
        .expect("message present");
    assert_eq!(consumed.message.payload["sequence"].as_u64(), Some(2));
}

#[test]
fn rejects_unknown_source_with_trace() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    router.register_agent(target.clone()).expect("target");

    let error = router
        .send_at(
            &runtime,
            envelope(source.clone(), target, run_id, 1, now),
            now,
        )
        .expect_err("unknown source rejected");
    assert!(matches!(error, MessageRouterError::UnknownSourceAgent(id) if id == source));
    let recorded = events.lock().expect("events lock");
    assert!(matches!(
        recorded[0].kind,
        TraceEventKind::MessageRejected { .. }
    ));
}

#[test]
fn rejects_when_source_outbox_quota_is_exceeded() {
    let source = AgentId::new();
    let first_target = AgentId::new();
    let second_target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_outbox_messages: 1,
        ..MessageRouterConfig::default()
    });
    allow_messages(
        &router,
        &source,
        &[first_target.clone(), second_target.clone()],
    );
    router
        .register_agent(first_target.clone())
        .expect("first target");
    router
        .register_agent(second_target.clone())
        .expect("second target");

    router
        .send_at(
            &runtime,
            envelope(source.clone(), first_target, run_id.clone(), 1, now),
            now,
        )
        .expect("first send");
    let error = router
        .send_at(
            &runtime,
            envelope(source.clone(), second_target, run_id.clone(), 2, now),
            now,
        )
        .expect_err("outbox full");

    assert!(
        matches!(error, MessageRouterError::OutboxFull { agent_id, limit: 1 } if agent_id == source)
    );
    assert_eq!(router.outbox(&source, &run_id).expect("outbox").len(), 1);
    let recorded = events.lock().expect("events lock");
    assert!(matches!(
        recorded.last().expect("rejection").kind,
        TraceEventKind::MessageRejected { .. }
    ));
}

#[test]
fn consume_next_returns_none_for_empty_run_and_consume_rejects_wrong_message() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, _events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::new();
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    assert!(router
        .consume_next_at(&runtime, &target, &run_id, now)
        .expect("empty consume")
        .is_none());
    router
        .send_at(
            &runtime,
            envelope(source, target.clone(), run_id.clone(), 1, now),
            now,
        )
        .expect("delivery");
    let missing = MessageId::new();
    let error = router
        .consume_at(&runtime, &target, &run_id, &missing, now)
        .expect_err("wrong message id");
    assert!(
        matches!(error, MessageRouterError::MessageNotVisible { message_id, .. } if message_id == missing)
    );
}

#[test]
fn expires_delivered_message_before_consumption_and_updates_outbox() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let created_at = OffsetDateTime::UNIX_EPOCH;
    let delivered_at = created_at + Duration::milliseconds(500);
    let consume_at = created_at + Duration::seconds(2);
    let (runtime, events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_message_age: Some(Duration::seconds(1)),
        ..MessageRouterConfig::default()
    });
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    let delivered = router
        .send_at(
            &runtime,
            envelope(
                source.clone(),
                target.clone(),
                run_id.clone(),
                1,
                created_at,
            ),
            delivered_at,
        )
        .expect("fresh delivery");
    let message_id = delivered.message.message_id.clone();
    let error = router
        .consume_at(&runtime, &target, &run_id, &message_id, consume_at)
        .expect_err("expired on consume");
    assert!(
        matches!(error, MessageRouterError::Expired { message_id: id, .. } if id == message_id)
    );
    assert!(router.inbox(&target, &run_id).expect("inbox").is_empty());
    let outbox = router.outbox(&source, &run_id).expect("outbox");
    assert_eq!(outbox[0].delivery_status, MessageDeliveryStatus::Expired);
    assert!(outbox[0].trace_links.expired_trace_id.is_some());
    let recorded = events.lock().expect("events lock");
    assert!(matches!(
        recorded.last().expect("expired").kind,
        TraceEventKind::MessageExpired { .. }
    ));
}

#[test]
fn fresh_message_with_ttl_does_not_expire() {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let created_at = OffsetDateTime::UNIX_EPOCH;
    let now = created_at + Duration::milliseconds(250);
    let (runtime, _events) = runtime_for(run_id.clone());
    let router = LocalMessageRouter::with_config(MessageRouterConfig {
        max_message_age: Some(Duration::seconds(1)),
        ..MessageRouterConfig::default()
    });
    allow_messages(&router, &source, std::slice::from_ref(&target));
    router.register_agent(target.clone()).expect("target");

    let delivered = router
        .send_at(
            &runtime,
            envelope(source, target, run_id, 1, created_at),
            now,
        )
        .expect("fresh delivery");
    assert_eq!(delivered.delivery_status, MessageDeliveryStatus::Delivered);
}

#[test]
fn rejects_trace_run_mismatch_without_enqueuing() {
    let source = AgentId::new();
    let target = AgentId::new();
    let message_run_id = RunId::new();
    let runtime_run_id = RunId::new();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
    let (runtime, _events) = runtime_for(runtime_run_id.clone());
    let router = LocalMessageRouter::new();
    router.register_agent(source.clone()).expect("source");
    router.register_agent(target.clone()).expect("target");

    let error = router
        .send_at(
            &runtime,
            envelope(source, target.clone(), message_run_id.clone(), 1, now),
            now,
        )
        .expect_err("trace run mismatch");
    assert!(
        matches!(error, MessageRouterError::TraceRunMismatch { runtime_run_id: runtime, message_run_id: message } if runtime == runtime_run_id && message == message_run_id)
    );
    assert!(router
        .inbox(&target, &message_run_id)
        .expect("target inbox")
        .is_empty());
}
