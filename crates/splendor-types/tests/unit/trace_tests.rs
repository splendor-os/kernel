use super::*;
use crate::{
    AgentId, EndpointScope, Message, MessageEnvelope, MessageId, MessageTraceContext, Percept,
    PerceptProvenance, RemoteMessageEnvelope, RemoteMessageRetryPolicy, RemoteMessageTraceContext,
    RevocationStatus, SideEffectClass, TenantId, WorkOrderAuthorization, WorkOrderSignature,
};

#[test]
fn trace_event_uses_deterministic_trace_id() {
    let run_id = RunId::new();
    let event = TraceEvent::new(
        run_id.clone(),
        5,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    assert_eq!(
        event.trace_event_id,
        TraceEventId::from_run_sequence(&run_id, 5)
    );
    assert_eq!(event.identity.run_id, run_id);
    assert_eq!(event.identity.tick_id, Some(TickId::from(1)));
    let payload = serde_json::to_value(&event).expect("serialize");
    assert!(payload.get("trace_event_id").is_some());
    assert!(payload.get("trace_id").is_none());
    assert_eq!(payload["identity"]["tick_id"], serde_json::json!(1));
}

#[test]
fn trace_event_round_trip() {
    let action = Action {
        name: "noop".to_string(),
        params: serde_json::json!({"ok": true}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: vec!["test".to_string()],
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let percept = Percept {
        schema: "sensor".to_string(),
        payload: serde_json::json!({"value": 1}),
        provenance: PerceptProvenance {
            source: "unit".to_string(),
            detail: None,
        },
        timestamp: OffsetDateTime::now_utc(),
    };
    let event = TraceEvent::new(
        RunId::new(),
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::CandidatesProposed {
            actions: vec![action],
        },
    );
    let percept_event = TraceEvent::new(
        event.run_id.clone(),
        1,
        OffsetDateTime::now_utc(),
        TraceEventKind::PerceptsReceived {
            percepts: vec![percept],
        },
    );
    let payload = serde_json::to_vec(&event).expect("serialize");
    let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, event);

    let payload = serde_json::to_vec(&percept_event).expect("serialize");
    let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, percept_event);
}

#[test]
fn message_rejection_trace_event_preserves_causal_parent() {
    let run_id = RunId::new();
    let causal_parent = TraceEventId::from_run_sequence(&run_id, 3);
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        AgentId::new(),
        run_id.clone(),
        "splendor.message.task_request.v1",
        serde_json::json!({"task": "forecast"}),
        Some(causal_parent.clone()),
        true,
        OffsetDateTime::now_utc(),
    )
    .expect("valid message");

    let event = TraceEvent::new(
        run_id,
        4,
        OffsetDateTime::now_utc(),
        TraceEventKind::MessageRejected {
            message: MessageTraceContext::from_message(&message),
            reason: message
                .payload_validation_failed("missing input_ref")
                .to_string(),
        },
    );

    let payload = serde_json::to_vec(&event).expect("serialize");
    let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, event);

    match decoded.kind {
        TraceEventKind::MessageRejected { message, reason } => {
            assert_eq!(message.causal_parent, Some(causal_parent));
            assert!(reason.contains("payload validation failed"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn message_lifecycle_trace_events_round_trip() {
    let run_id = RunId::new();
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        AgentId::new(),
        run_id.clone(),
        "splendor.message.task_request.v1",
        serde_json::json!({"task": "forecast"}),
        None,
        false,
        OffsetDateTime::now_utc(),
    )
    .expect("valid message");
    let context = MessageTraceContext::from_message(&message);
    let events = vec![
        TraceEventKind::MessageQueued {
            message: context.clone(),
        },
        TraceEventKind::MessageDelivered {
            message: context.clone(),
        },
        TraceEventKind::MessageExpired {
            message: context.clone(),
            reason: Some("ttl exceeded".to_string()),
        },
        TraceEventKind::MessageConsumed { message: context },
    ];

    for (sequence, kind) in events.into_iter().enumerate() {
        let event = TraceEvent::new(
            run_id.clone(),
            sequence as u64,
            OffsetDateTime::now_utc(),
            kind,
        );
        let payload = serde_json::to_vec(&event).expect("serialize");
        let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
        assert_eq!(decoded, event);
    }
}

#[test]
fn remote_message_trace_events_round_trip_with_causal_linkage() {
    let run_id = RunId::new();
    let source = AgentId::new();
    let target = AgentId::new();
    let tenant_id = TenantId::new();
    let causal_parent = TraceId::from_run_sequence(&run_id, 9);
    let now = OffsetDateTime::now_utc();
    let message = Message::new(
        MessageId::new(),
        source,
        target.clone(),
        run_id.clone(),
        "splendor.message.task_request.v1",
        serde_json::json!({"task": "summarize"}),
        Some(causal_parent.clone()),
        true,
        now,
    )
    .expect("valid message");
    let message_envelope = MessageEnvelope::new(message).expect("valid envelope");
    let remote = RemoteMessageEnvelope::new(
        tenant_id.clone(),
        "instance_a",
        "instance_b",
        WorkOrderAuthorization {
            work_order_id: "wo_remote_trace".to_string(),
            tenant_id,
            agent_id: target,
            run_id: Some(run_id.clone()),
            allowed_scopes: vec![EndpointScope::MessagesSend],
            signature: Some(WorkOrderSignature {
                key_id: "key".to_string(),
                signature: "sig".to_string(),
            }),
            expires_at: now + time::Duration::hours(1),
            revocation: RevocationStatus::Active,
        },
        message_envelope,
        RemoteMessageRetryPolicy::Idempotent {
            max_attempts: 2,
            idempotency_key: "message-key".to_string(),
        },
        now,
        None,
    )
    .expect("valid remote");
    let context = RemoteMessageTraceContext::from_envelope(&remote);
    let events = vec![
        TraceEventKind::RemoteMessageSent {
            remote_message: context.clone(),
        },
        TraceEventKind::RemoteMessageAccepted {
            remote_message: context.clone(),
        },
        TraceEventKind::RemoteMessageRejected {
            remote_message: context.clone(),
            reason: "wrong target".to_string(),
        },
        TraceEventKind::RemoteMessageDelivered {
            remote_message: context.clone(),
        },
        TraceEventKind::RemoteMessageTimedOut {
            remote_message: context.clone(),
            reason: "deadline exceeded".to_string(),
        },
        TraceEventKind::RemoteMessageDuplicate {
            remote_message: context.clone(),
            reason: "already accepted".to_string(),
        },
        TraceEventKind::RemoteMessageTransportFailed {
            remote_message: context,
            reason: "connection reset".to_string(),
        },
    ];

    for (sequence, kind) in events.into_iter().enumerate() {
        let event = TraceEvent::new(run_id.clone(), sequence as u64, now, kind);
        let payload = serde_json::to_vec(&event).expect("serialize");
        let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
        assert_eq!(decoded, event);
        match decoded.kind {
            TraceEventKind::RemoteMessageSent { remote_message }
            | TraceEventKind::RemoteMessageAccepted { remote_message }
            | TraceEventKind::RemoteMessageDelivered { remote_message }
            | TraceEventKind::RemoteMessageTimedOut { remote_message, .. }
            | TraceEventKind::RemoteMessageDuplicate { remote_message, .. }
            | TraceEventKind::RemoteMessageTransportFailed { remote_message, .. }
            | TraceEventKind::RemoteMessageRejected { remote_message, .. } => {
                assert_eq!(
                    remote_message.message.causal_parent,
                    Some(causal_parent.clone())
                );
                assert_eq!(
                    remote_message.idempotency_key.as_deref(),
                    Some("message-key")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
