use super::*;
use uuid::Uuid;

fn valid_message() -> Message {
    Message::new(
        MessageId::new(),
        AgentId::new(),
        AgentId::new(),
        RunId::new(),
        "splendor.message.task_request.v1",
        serde_json::json!({
            "task": "forecast revenue for Q3",
            "input_ref": "dataset:finance.revenue_monthly_v4"
        }),
        Some(TraceId::new()),
        true,
        OffsetDateTime::now_utc(),
    )
    .expect("valid message")
}

#[test]
fn message_requires_all_identity_scope_fields() {
    let message = valid_message();

    let mut missing_message_id = message.clone();
    missing_message_id.message_id = MessageId::from(Uuid::nil());
    assert_eq!(
        missing_message_id.validate(),
        Err(MessageValidationError::MissingMessageId)
    );

    let mut missing_source = message.clone();
    missing_source.source_agent_id = AgentId::from(Uuid::nil());
    assert_eq!(
        missing_source.validate(),
        Err(MessageValidationError::MissingSourceAgentId)
    );

    let mut missing_target = message.clone();
    missing_target.target_agent_id = AgentId::from(Uuid::nil());
    assert_eq!(
        missing_target.validate(),
        Err(MessageValidationError::MissingTargetAgentId)
    );

    let mut missing_run = message;
    missing_run.run_id = RunId::from(Uuid::nil());
    assert_eq!(
        missing_run.validate(),
        Err(MessageValidationError::MissingRunId)
    );
}

#[test]
fn message_requires_schema_payload_and_timestamp() {
    let message = valid_message();

    let mut missing_schema = message.clone();
    missing_schema.schema = "  ".to_string();
    assert_eq!(
        missing_schema.validate(),
        Err(MessageValidationError::MissingSchema)
    );

    let mut missing_payload = message.clone();
    missing_payload.payload = serde_json::Value::Null;
    assert_eq!(
        missing_payload.validate(),
        Err(MessageValidationError::MissingPayload)
    );

    let serialized_without_timestamp = serde_json::json!({
        "message_id": MessageId::new(),
        "source_agent_id": AgentId::new(),
        "target_agent_id": AgentId::new(),
        "run_id": RunId::new(),
        "schema": "splendor.message.task_request.v1",
        "payload": {"task": "forecast"},
        "causal_parent": null,
        "requires_response": true
    });
    let decoded = serde_json::from_value::<Message>(serialized_without_timestamp);
    assert!(decoded.is_err(), "created_at must be present");
}

#[test]
fn invalid_schema_versions_are_rejected_before_routing() {
    assert_eq!(
        MessageSchemaVersion::from_schema("splendor.message.task_request"),
        Err(MessageValidationError::InvalidSchemaVersion {
            version: "task_request".to_string()
        })
    );
    assert_eq!(
        MessageSchemaVersion::from_schema("splendor.message.task_request.vx"),
        Err(MessageValidationError::InvalidSchemaVersion {
            version: "vx".to_string()
        })
    );
    assert_eq!(
        MessageSchemaVersion::from_schema("splendor.message.task_request.v2"),
        Err(MessageValidationError::UnsupportedSchemaVersion {
            version: "v2".to_string()
        })
    );
}

#[test]
fn message_envelope_validates_schema_version_and_status() {
    let message = valid_message();
    let envelope = MessageEnvelope::new(message.clone()).expect("valid envelope");

    assert_eq!(envelope.message, message);
    assert_eq!(envelope.schema_version, MessageSchemaVersion::V1);
    assert_eq!(envelope.delivery_status, MessageDeliveryStatus::Pending);
    envelope.validate().expect("envelope remains valid");

    let mut mismatched = envelope;
    mismatched.message.schema = "splendor.message.task_request.v2".to_string();
    assert_eq!(
        mismatched.validate(),
        Err(MessageValidationError::UnsupportedSchemaVersion {
            version: "v2".to_string()
        })
    );
}

#[test]
fn causal_parent_and_trace_links_round_trip_for_replay() {
    let message = valid_message();
    let causal_parent = message
        .causal_parent
        .clone()
        .expect("valid fixture has causal parent");
    let envelope = MessageEnvelope {
        message,
        schema_version: MessageSchemaVersion::V1,
        delivery_status: MessageDeliveryStatus::Queued,
        trace_links: MessageTraceLinks {
            queued_trace_id: Some(TraceId::new()),
            ..MessageTraceLinks::default()
        },
    };

    let payload = serde_json::to_vec(&envelope).expect("serialize envelope");
    let decoded: MessageEnvelope = serde_json::from_slice(&payload).expect("deserialize envelope");

    assert_eq!(decoded, envelope);
    assert_eq!(decoded.message.causal_parent, Some(causal_parent));
    assert!(decoded.trace_links.queued_trace_id.is_some());
}

#[test]
fn payload_validation_failure_is_structured_for_rejection_trace() {
    let message = valid_message();
    let error = message.payload_validation_failed("missing task");

    assert_eq!(
        error,
        MessageValidationError::PayloadValidationFailed {
            schema: "splendor.message.task_request.v1".to_string(),
            reason: "missing task".to_string()
        }
    );
}

#[test]
fn message_schema_is_transport_neutral() {
    let envelope = MessageEnvelope::new(valid_message()).expect("valid envelope");
    let json = serde_json::to_string(&envelope).expect("serialize envelope");

    for forbidden in ["http", "grpc", "nats", "fleet", "broker", "topic", "url"] {
        assert!(
            !json.to_ascii_lowercase().contains(forbidden),
            "message envelope must not contain transport-specific `{forbidden}` fields"
        );
    }
}
