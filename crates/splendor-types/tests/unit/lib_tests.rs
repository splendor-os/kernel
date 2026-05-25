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
    let trace_id = TraceId::from_run_sequence(&run_id, 1);
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        AgentId::new(),
        run_id.clone(),
        "splendor.message.task_request.v1",
        serde_json::json!({"task": "forecast revenue"}),
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
    round_trip(&feedback);
    round_trip(&reward);
    round_trip(&trace_event);
}
