use super::*;

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let payload = serde_json::to_vec(value).expect("serialize");
    let decoded: T = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(&decoded, value);
}

fn approval_test_action() -> Action {
    Action {
        name: "artifact.publish".to_string(),
        params: serde_json::json!({"artifact": "artifact_1"}),
        side_effect_class: SideEffectClass::External,
        cost_estimate: None,
        required_permissions: vec!["artifact.publish".to_string()],
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    }
}

#[test]
fn round_trip_core_types() {
    let run_id = RunId::new();
    let trace_id = TraceEventId::from_run_sequence(&run_id, 1);
    let fleet_id = FleetId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let target_agent_id = AgentId::new();
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target_agent_id.clone(),
        "forecast revenue",
        DelegatedAuthority::empty(),
    )
    .expect("task request");
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        target_agent_id,
        run_id.clone(),
        TASK_REQUEST_SCHEMA,
        serde_json::to_value(task).expect("task payload"),
        Some(trace_id.clone()),
        true,
        time::OffsetDateTime::now_utc(),
    )
    .expect("valid message");
    let message_envelope = MessageEnvelope::new(message.clone()).expect("valid envelope");
    let percept = Percept {
        schema: "sensor".to_string(),
        payload: serde_json::json!({"value": 3}),
        provenance: PerceptProvenance {
            source: "unit".to_string(),
            detail: Some("test".to_string()),
        },
        timestamp: time::OffsetDateTime::now_utc(),
    };
    let action = Action {
        name: "noop".to_string(),
        params: serde_json::json!({"ok": true}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: Some(CostEstimate {
            units: "ms".to_string(),
            amount: 1.0,
        }),
        required_permissions: vec!["read".to_string()],
        preconditions: vec!["ready".to_string()],
        postconditions: vec!["done".to_string()],
    };
    let constraint = Constraint {
        id: "c1".to_string(),
        kind: ConstraintKind::Hard,
        scope: ConstraintScope::Global,
        predicate: "always".to_string(),
        obligation: None,
    };
    let feedback = Feedback {
        kind: "human".to_string(),
        payload: serde_json::json!({"score": 1}),
        recorded_at: time::OffsetDateTime::now_utc(),
    };
    let reward = Reward {
        value: 0.5,
        units: Some("points".to_string()),
        recorded_at: time::OffsetDateTime::now_utc(),
        context: Some(serde_json::json!({"source": "test"})),
    };
    let verification = VerificationResult::allow();
    let quota_usage = QuotaUsage::single_action();
    let mut fleet_telemetry =
        FleetTelemetrySnapshot::new(fleet_id.clone(), time::OffsetDateTime::now_utc());
    fleet_telemetry.nodes.push(NodeTelemetry::from_heartbeat(
        fleet_id,
        node_id.clone(),
        Some(time::OffsetDateTime::now_utc()),
        time::OffsetDateTime::now_utc(),
        time::Duration::seconds(30),
        time::Duration::seconds(120),
        vec![instance_id.clone()],
    ));
    fleet_telemetry.instances.push(InstanceTelemetry::new(
        node_id,
        instance_id,
        "splendor-0.03-dev",
        TelemetryRuntimeMode::Resident,
        vec!["trace.sync".to_string()],
        time::OffsetDateTime::now_utc(),
    ));
    let trace_event = TraceEvent::new(
        run_id.clone(),
        0,
        time::OffsetDateTime::now_utc(),
        TraceEventKind::OutcomeRecorded {
            outcome: serde_json::json!({"ok": true}),
            feedback: Some(feedback.clone()),
            reward: Some(reward.clone()),
        },
    );

    round_trip(&run_id);
    round_trip(&fleet_telemetry.fleet_id);
    round_trip(&fleet_telemetry.nodes[0].node_id);
    round_trip(&fleet_telemetry.instances[0].instance_id);
    round_trip(&message.message_id);
    round_trip(&trace_id);
    round_trip(&SnapshotId::from_bytes(b"snapshot"));
    round_trip(&ContentHash::blake3(b"hash"));
    round_trip(&percept);
    round_trip(&message);
    round_trip(&message_envelope);
    round_trip(&MessageTraceContext::from_message(&message));
    round_trip(&action);
    round_trip(&constraint);
    round_trip(&verification);
    round_trip(&quota_usage);
    round_trip(&fleet_telemetry);
    round_trip(&feedback);
    round_trip(&reward);
    round_trip(&trace_event);
}

#[test]
fn approval_policy_matching_is_scoped_and_evidence_builders_round_trip() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = ActionId::new();
    let action = approval_test_action();
    let scope = ApprovalActionScope {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &action,
        adapter: Some("artifact-store"),
    };

    let mut policy = ApprovalPolicy::new(
        "publish_policy",
        tenant_id.clone(),
        "publishing artifacts requires approval",
    );
    policy.agent_id = Some(agent_id.clone());
    policy.action_name = Some("artifact.publish".to_string());
    policy.adapter = Some("artifact-store".to_string());
    policy.required_permission = Some("artifact.publish".to_string());
    policy.side_effect_class = Some(SideEffectClass::External);
    policy.expires_at = Some(now + time::Duration::minutes(5));
    assert!(policy.matches_action(&scope, now));
    assert!(!policy.is_expired(now));

    let mut wrong_tenant = policy.clone();
    wrong_tenant.tenant_id = TenantId::new();
    assert!(!wrong_tenant.matches_action(&scope, now));

    let mut wrong_agent = policy.clone();
    wrong_agent.agent_id = Some(AgentId::new());
    assert!(!wrong_agent.matches_action(&scope, now));

    let mut wrong_action = policy.clone();
    wrong_action.action_name = Some("artifact.delete".to_string());
    assert!(!wrong_action.matches_action(&scope, now));

    let mut wrong_adapter = policy.clone();
    wrong_adapter.adapter = Some("other-adapter".to_string());
    assert!(!wrong_adapter.matches_action(&scope, now));

    let mut wrong_permission = policy.clone();
    wrong_permission.required_permission = Some("artifact.delete".to_string());
    assert!(!wrong_permission.matches_action(&scope, now));

    let mut wrong_side_effect = policy.clone();
    wrong_side_effect.side_effect_class = Some(SideEffectClass::ReadOnly);
    assert!(!wrong_side_effect.matches_action(&scope, now));

    let mut expired = policy;
    expired.expires_at = Some(now - time::Duration::seconds(1));
    assert!(expired.is_expired(now));

    let trace_event_id = TraceEventId::from_run_sequence(&run_id, 7);
    let evidence = ApprovalEvidence::new(
        ApprovalId::new(),
        tenant_id,
        agent_id,
        run_id,
        ApprovalDecision::Granted,
        now + time::Duration::minutes(10),
    )
    .with_action_name("artifact.publish")
    .with_adapter("artifact-store")
    .with_trace_event(trace_event_id.clone());
    assert_eq!(evidence.action_name.as_deref(), Some("artifact.publish"));
    assert_eq!(evidence.adapter.as_deref(), Some("artifact-store"));
    assert_eq!(evidence.trace_event_id.as_ref(), Some(&trace_event_id));
    round_trip(&evidence);
}
