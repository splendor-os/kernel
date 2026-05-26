use super::*;

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let payload = serde_json::to_vec(value).expect("serialize");
    let decoded: T = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(&decoded, value);
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
