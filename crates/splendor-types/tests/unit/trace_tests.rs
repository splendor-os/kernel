use super::*;
use crate::{
    AgentId, AuditAttribution, ClientPrincipal, DelegatedAuthority, LocalDelegationTraceContext,
    Message, MessageId, MessageTraceContext, Percept, PerceptProvenance, SideEffectClass,
    TaskFailure, TaskRequest, TASK_REQUEST_SCHEMA,
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
    let target = AgentId::new();
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target.clone(),
        "forecast",
        DelegatedAuthority::empty(),
    )
    .expect("task request");
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        target,
        run_id.clone(),
        TASK_REQUEST_SCHEMA,
        serde_json::to_value(task).expect("task payload"),
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
    let target = AgentId::new();
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target.clone(),
        "forecast",
        DelegatedAuthority::empty(),
    )
    .expect("task request");
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        target,
        run_id.clone(),
        TASK_REQUEST_SCHEMA,
        serde_json::to_value(task).expect("task payload"),
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
fn local_delegation_trace_events_round_trip() {
    let parent_run_id = RunId::new();
    let child_run_id = RunId::new();
    let context = LocalDelegationTraceContext {
        parent_run_id: parent_run_id.clone(),
        child_run_id: child_run_id.clone(),
        parent_trace_id: Some(TraceId::from_run_sequence(&parent_run_id, 4)),
        request_message_id: Some(MessageId::new()),
        response_message_id: Some(MessageId::new()),
        source_agent_id: AgentId::new(),
        target_agent_id: AgentId::new(),
        objective: "summarize ledger".to_string(),
    };
    let failure = TaskFailure::new("child_failed", "specialist failed", false)
        .with_trace_id(TraceId::from_run_sequence(&child_run_id, 2));
    let events = vec![
        TraceEventKind::DelegationRequested {
            delegation: context.clone(),
        },
        TraceEventKind::DelegationRejected {
            delegation: context.clone(),
            reason: "parent_run_cancelled".to_string(),
        },
        TraceEventKind::ParentRunCancelled {
            parent_run_id: parent_run_id.clone(),
            agent_id: context.source_agent_id.clone(),
            reason: "operator".to_string(),
        },
        TraceEventKind::ChildRunStarted {
            delegation: context.clone(),
        },
        TraceEventKind::ChildRunCompleted {
            delegation: context.clone(),
        },
        TraceEventKind::ChildRunFailed {
            delegation: context,
            failure,
        },
    ];

    for (sequence, kind) in events.into_iter().enumerate() {
        let event = TraceEvent::new(
            parent_run_id.clone(),
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
fn child_run_link_trace_event_round_trips_with_source_message() {
    let parent_run_id = RunId::new();
    let child_run_id = RunId::new();
    let parent_agent_id = AgentId::new();
    let child_agent_id = AgentId::new();
    let causal_parent = TraceId::from_run_sequence(&parent_run_id, 3);
    let source_message_id = MessageId::new();

    let event = TraceEvent::new(
        parent_run_id.clone(),
        4,
        OffsetDateTime::now_utc(),
        TraceEventKind::ChildRunLinked {
            parent_run_id: parent_run_id.clone(),
            child_run_id: child_run_id.clone(),
            parent_agent_id: parent_agent_id.clone(),
            child_agent_id: child_agent_id.clone(),
            causal_parent: Some(causal_parent.clone()),
            source_message_id: Some(source_message_id.clone()),
        },
    );

    let payload = serde_json::to_vec(&event).expect("serialize");
    let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, event);

    match decoded.kind {
        TraceEventKind::ChildRunLinked {
            parent_run_id: decoded_parent_run,
            child_run_id: decoded_child_run,
            parent_agent_id: decoded_parent_agent,
            child_agent_id: decoded_child_agent,
            causal_parent: decoded_causal_parent,
            source_message_id: decoded_source_message,
        } => {
            assert_eq!(decoded_parent_run, parent_run_id);
            assert_eq!(decoded_child_run, child_run_id);
            assert_eq!(decoded_parent_agent, parent_agent_id);
            assert_eq!(decoded_child_agent, child_agent_id);
            assert_eq!(decoded_causal_parent, Some(causal_parent));
            assert_eq!(decoded_source_message, Some(source_message_id));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn daemon_audit_trace_event_preserves_caller_attribution() {
    let run_id = RunId::new();
    let audit = AuditAttribution {
        principal: ClientPrincipal::new("app_test", "client_test"),
        credential_id: Some("cred_test".to_string()),
        requested_at: OffsetDateTime::now_utc(),
    };
    let event = TraceEvent::new(
        run_id,
        9,
        OffsetDateTime::now_utc(),
        TraceEventKind::DaemonAudit {
            endpoint: "splendor.runs.create".to_string(),
            audit: audit.clone(),
        },
    );

    let payload = serde_json::to_vec(&event).expect("serialize");
    let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, event);

    match decoded.kind {
        TraceEventKind::DaemonAudit {
            endpoint,
            audit: decoded_audit,
        } => {
            assert_eq!(endpoint, "splendor.runs.create");
            assert_eq!(decoded_audit, audit);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
