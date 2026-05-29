use super::*;
use crate::{
    ActionId, AgentId, ApprovalId, AuditAttribution, CircuitBreakerId, ClientPrincipal,
    DelegatedAuthority, EndpointScope, EscalationId, GovernanceExtensions, GovernanceIssuer,
    GovernanceObjectRef, GovernanceScope, GovernanceState, GovernanceTraceLink,
    GovernanceTransition, GovernanceTransitionError, InterventionId, KillSwitchId,
    LocalDelegationTraceContext, Message, MessageEnvelope, MessageId, MessageTraceContext, Percept,
    PerceptProvenance, RemoteMessageEnvelope, RemoteMessageRetryPolicy, RemoteMessageTraceContext,
    RevocationStatus, SideEffectClass, SnapshotId, StateHandoffTraceContext, StateReferenceMode,
    TaskFailure, TaskRequest, TenantId, TraceId, WorkOrderAuthorization, WorkOrderSignature,
    TASK_REQUEST_SCHEMA,
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
fn remote_message_trace_events_round_trip_with_causal_linkage() {
    let run_id = RunId::new();
    let source = AgentId::new();
    let target = AgentId::new();
    let tenant_id = TenantId::new();
    let causal_parent = TraceEventId::from_run_sequence(&run_id, 9);
    let now = OffsetDateTime::now_utc();
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target.clone(),
        "summarize",
        DelegatedAuthority::empty(),
    )
    .expect("valid task request");
    let message = Message::new(
        MessageId::new(),
        source,
        target.clone(),
        run_id.clone(),
        TASK_REQUEST_SCHEMA,
        serde_json::to_value(task).expect("task payload"),
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

#[test]
fn governance_trace_events_round_trip_and_apply_scope_identity() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let action_id = ActionId::new();
    let now = OffsetDateTime::now_utc();
    let scope = GovernanceScope::Action {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        run_id: run_id.clone(),
        action_id: action_id.clone(),
    };
    let issuer = GovernanceIssuer::new("operator", "daemon").expect("issuer");
    let trace = GovernanceTraceLink::new(
        TraceEventId::from_run_sequence(&run_id, 20),
        Some(run_id.clone()),
    );
    let transition = |object, from, to, sequence| {
        GovernanceTransition::try_new(
            object,
            scope.clone(),
            from,
            to,
            now,
            "governance transition",
            issuer.clone(),
            GovernanceTraceLink::new(
                TraceEventId::from_run_sequence(&run_id, sequence),
                Some(run_id.clone()),
            ),
            GovernanceExtensions::new(),
        )
        .expect("transition")
    };

    let invalid = GovernanceTransition::try_new(
        GovernanceObjectRef::Approval {
            approval_id: ApprovalId::new(),
        },
        scope.clone(),
        Some(GovernanceState::Denied),
        GovernanceState::Granted,
        now,
        "invalid transition",
        issuer.clone(),
        trace,
        GovernanceExtensions::new(),
    )
    .expect_err("invalid transition");
    let GovernanceTransitionError::Rejected(rejection) = invalid else {
        panic!("expected transition rejection");
    };

    let events = vec![
        TraceEventKind::ApprovalRequested {
            transition: transition(
                GovernanceObjectRef::Approval {
                    approval_id: ApprovalId::new(),
                },
                None,
                GovernanceState::Requested,
                21,
            ),
        },
        TraceEventKind::ApprovalGranted {
            transition: transition(
                GovernanceObjectRef::Approval {
                    approval_id: ApprovalId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Granted,
                22,
            ),
        },
        TraceEventKind::ApprovalDenied {
            transition: transition(
                GovernanceObjectRef::Approval {
                    approval_id: ApprovalId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Denied,
                23,
            ),
        },
        TraceEventKind::ApprovalExpired {
            transition: transition(
                GovernanceObjectRef::Approval {
                    approval_id: ApprovalId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Expired,
                24,
            ),
        },
        TraceEventKind::ApprovalRevoked {
            transition: transition(
                GovernanceObjectRef::Approval {
                    approval_id: ApprovalId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Revoked,
                25,
            ),
        },
        TraceEventKind::EscalationOpened {
            transition: transition(
                GovernanceObjectRef::Escalation {
                    escalation_id: EscalationId::new(),
                },
                None,
                GovernanceState::Open,
                26,
            ),
        },
        TraceEventKind::EscalationResolved {
            transition: transition(
                GovernanceObjectRef::Escalation {
                    escalation_id: EscalationId::new(),
                },
                Some(GovernanceState::Open),
                GovernanceState::Resolved,
                27,
            ),
        },
        TraceEventKind::EscalationExpired {
            transition: transition(
                GovernanceObjectRef::Escalation {
                    escalation_id: EscalationId::new(),
                },
                Some(GovernanceState::Open),
                GovernanceState::Expired,
                28,
            ),
        },
        TraceEventKind::EscalationRevoked {
            transition: transition(
                GovernanceObjectRef::Escalation {
                    escalation_id: EscalationId::new(),
                },
                Some(GovernanceState::Open),
                GovernanceState::Revoked,
                29,
            ),
        },
        TraceEventKind::InterventionRequested {
            transition: transition(
                GovernanceObjectRef::Intervention {
                    intervention_id: InterventionId::new(),
                },
                None,
                GovernanceState::Requested,
                30,
            ),
        },
        TraceEventKind::InterventionResolved {
            transition: transition(
                GovernanceObjectRef::Intervention {
                    intervention_id: InterventionId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Resolved,
                31,
            ),
        },
        TraceEventKind::InterventionCancelled {
            transition: transition(
                GovernanceObjectRef::Intervention {
                    intervention_id: InterventionId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Cancelled,
                32,
            ),
        },
        TraceEventKind::InterventionExpired {
            transition: transition(
                GovernanceObjectRef::Intervention {
                    intervention_id: InterventionId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Expired,
                33,
            ),
        },
        TraceEventKind::InterventionRevoked {
            transition: transition(
                GovernanceObjectRef::Intervention {
                    intervention_id: InterventionId::new(),
                },
                Some(GovernanceState::Requested),
                GovernanceState::Revoked,
                34,
            ),
        },
        TraceEventKind::CircuitBreakerTripped {
            transition: transition(
                GovernanceObjectRef::CircuitBreaker {
                    circuit_breaker_id: CircuitBreakerId::new(),
                },
                None,
                GovernanceState::Active,
                35,
            ),
        },
        TraceEventKind::CircuitBreakerCleared {
            transition: transition(
                GovernanceObjectRef::CircuitBreaker {
                    circuit_breaker_id: CircuitBreakerId::new(),
                },
                Some(GovernanceState::Active),
                GovernanceState::Cleared,
                36,
            ),
        },
        TraceEventKind::CircuitBreakerExpired {
            transition: transition(
                GovernanceObjectRef::CircuitBreaker {
                    circuit_breaker_id: CircuitBreakerId::new(),
                },
                Some(GovernanceState::Active),
                GovernanceState::Expired,
                37,
            ),
        },
        TraceEventKind::CircuitBreakerRevoked {
            transition: transition(
                GovernanceObjectRef::CircuitBreaker {
                    circuit_breaker_id: CircuitBreakerId::new(),
                },
                Some(GovernanceState::Active),
                GovernanceState::Revoked,
                38,
            ),
        },
        TraceEventKind::KillSwitchActivated {
            transition: transition(
                GovernanceObjectRef::KillSwitch {
                    kill_switch_id: KillSwitchId::new(),
                },
                None,
                GovernanceState::Active,
                39,
            ),
        },
        TraceEventKind::KillSwitchCleared {
            transition: transition(
                GovernanceObjectRef::KillSwitch {
                    kill_switch_id: KillSwitchId::new(),
                },
                Some(GovernanceState::Active),
                GovernanceState::Cleared,
                40,
            ),
        },
        TraceEventKind::KillSwitchExpired {
            transition: transition(
                GovernanceObjectRef::KillSwitch {
                    kill_switch_id: KillSwitchId::new(),
                },
                Some(GovernanceState::Active),
                GovernanceState::Expired,
                41,
            ),
        },
        TraceEventKind::KillSwitchRevoked {
            transition: transition(
                GovernanceObjectRef::KillSwitch {
                    kill_switch_id: KillSwitchId::new(),
                },
                Some(GovernanceState::Active),
                GovernanceState::Revoked,
                42,
            ),
        },
        TraceEventKind::GovernanceTransitionRejected {
            rejection: *rejection,
        },
    ];

    for (offset, kind) in events.into_iter().enumerate() {
        let event = TraceEvent::new(run_id.clone(), 50 + offset as u64, now, kind);
        assert_eq!(event.identity.tenant_id, Some(tenant_id.clone()));
        assert_eq!(event.identity.agent_id, Some(agent_id.clone()));
        assert_eq!(event.identity.action_id, Some(action_id.clone()));
        let payload = serde_json::to_vec(&event).expect("serialize");
        let decoded: TraceEvent = serde_json::from_slice(&payload).expect("deserialize");
        assert_eq!(decoded, event);
    }
}

#[test]
fn governance_trace_event_rejects_scope_run_mismatch_with_explicit_identity() {
    let trace_run_id = RunId::new();
    let scoped_run_id = RunId::new();
    let now = OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let transition = GovernanceTransition::try_new(
        GovernanceObjectRef::CircuitBreaker {
            circuit_breaker_id: CircuitBreakerId::new(),
        },
        GovernanceScope::Run {
            tenant_id,
            agent_id,
            run_id: scoped_run_id.clone(),
        },
        None,
        GovernanceState::Active,
        now,
        "run scoped breaker",
        GovernanceIssuer::new("operator", "daemon").expect("issuer"),
        GovernanceTraceLink::new(
            TraceEventId::from_run_sequence(&scoped_run_id, 1),
            Some(scoped_run_id.clone()),
        ),
        GovernanceExtensions::new(),
    )
    .expect("transition");

    let result = TraceEvent::try_new_with_identity(
        TraceIdentityContext::new(trace_run_id.clone()),
        1,
        now,
        TraceEventKind::CircuitBreakerTripped { transition },
    );

    assert!(matches!(
        result,
        Err(IdentityValidationError::Mismatch {
            field: "governance_scope.run_id",
            expected,
            actual,
        }) if expected == trace_run_id.to_string() && actual == scoped_run_id.to_string()
    ));
}
