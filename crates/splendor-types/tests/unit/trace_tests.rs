use super::*;
use crate::{
    AgentId, Message, MessageId, MessageTraceContext, Percept, PerceptProvenance, SideEffectClass,
    SnapshotId, StateHandoffTraceContext, StateReferenceMode, TenantId,
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
    assert_eq!(event.trace_id, TraceId::from_run_sequence(&run_id, 5));
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
    let causal_parent = TraceId::from_run_sequence(&run_id, 3);
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
fn state_handoff_trace_events_round_trip_with_previous_head() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let bytes = b"handoff".to_vec();
    let handoff = StateHandoffTraceContext {
        handoff_id: "handoff_trace".to_string(),
        mode: StateReferenceMode::SnapshotImport,
        tenant_id,
        agent_id,
        run_id: run_id.clone(),
        work_order_id: "wo_handoff".to_string(),
        source_instance_id: Some("source".to_string()),
        receiver_instance_id: Some("receiver".to_string()),
        source_state_node_id: ContentHash::blake3(&bytes).to_string(),
        previous_state_node_id: Some("blake3:previous".to_string()),
        receiver_state_node_id: Some("blake3:receiver".to_string()),
        snapshot_id: Some(SnapshotId::from_bytes(&bytes)),
        source_trace_id: Some(TraceId::from_run_sequence(&run_id, 1)),
    };
    let events = vec![
        TraceEventKind::StateHandoffExported {
            handoff: handoff.clone(),
        },
        TraceEventKind::StateHandoffImported {
            handoff: handoff.clone(),
        },
        TraceEventKind::StateHandoffImportFailed {
            handoff: handoff.clone(),
            reason: "corrupted snapshot".to_string(),
        },
        TraceEventKind::ReadOnlyStateReferenced { handoff },
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
