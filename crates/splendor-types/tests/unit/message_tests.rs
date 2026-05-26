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
        Some(TraceEventId::new()),
        true,
        OffsetDateTime::now_utc(),
    )
    .expect("valid message")
}

fn signed_remote_work_order(
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: RunId,
    now: OffsetDateTime,
) -> WorkOrderAuthorization {
    WorkOrderAuthorization {
        work_order_id: "wo_remote_test".to_string(),
        tenant_id,
        agent_id,
        run_id: Some(run_id),
        allowed_scopes: vec![EndpointScope::MessagesSend],
        signature: Some(crate::WorkOrderSignature {
            key_id: "key_remote".to_string(),
            signature: "sig_remote".to_string(),
        }),
        expires_at: now + time::Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn valid_remote_envelope(now: OffsetDateTime) -> RemoteMessageEnvelope {
    let message = valid_message();
    let tenant_id = TenantId::new();
    let work_order = signed_remote_work_order(
        tenant_id.clone(),
        message.target_agent_id.clone(),
        message.run_id.clone(),
        now,
    );
    let envelope = MessageEnvelope::new(message).expect("valid local envelope");
    RemoteMessageEnvelope::new(
        tenant_id,
        "instance_source",
        "instance_target",
        work_order,
        envelope,
        RemoteMessageRetryPolicy::Never,
        now,
        Some(now + time::Duration::minutes(5)),
    )
    .expect("valid remote envelope")
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
            queued_trace_id: Some(TraceEventId::new()),
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

#[test]
fn remote_message_envelope_wraps_canonical_message_without_mutating_payload() {
    let now = OffsetDateTime::now_utc();
    let remote = valid_remote_envelope(now);
    let canonical_before = serde_json::to_value(&remote.message_envelope.message)
        .expect("canonical message serializes");

    let payload = serde_json::to_vec(&remote).expect("serialize remote envelope");
    let decoded: RemoteMessageEnvelope =
        serde_json::from_slice(&payload).expect("deserialize remote envelope");

    decoded.validate_at(now).expect("decoded remains valid");
    assert_eq!(decoded.message_envelope, remote.message_envelope);
    assert_eq!(
        serde_json::to_value(&decoded.message_envelope.message).expect("canonical message"),
        canonical_before
    );
    assert_eq!(
        decoded.remote_schema_version,
        RemoteMessageEnvelopeVersion::V1
    );
}

#[test]
fn remote_message_validation_rejects_unsigned_expired_or_incompatible_work_order() {
    let now = OffsetDateTime::now_utc();
    let remote = valid_remote_envelope(now);

    let mut unsigned = remote.clone();
    unsigned.work_order.signature = None;
    assert_eq!(
        unsigned.validate_at(now),
        Err(RemoteMessageValidationError::UnsignedWorkOrder)
    );

    let mut expired = remote.clone();
    expired.work_order.expires_at = now;
    assert_eq!(
        expired.validate_at(now),
        Err(RemoteMessageValidationError::ExpiredWorkOrder)
    );

    let mut wrong_agent = remote.clone();
    wrong_agent.work_order.agent_id = AgentId::new();
    assert_eq!(
        wrong_agent.validate_at(now),
        Err(RemoteMessageValidationError::IncompatibleWorkOrder)
    );

    let mut missing_scope = remote;
    missing_scope.work_order.allowed_scopes = vec![EndpointScope::RunsCreate];
    assert_eq!(
        missing_scope.validate_at(now),
        Err(RemoteMessageValidationError::IncompatibleWorkOrder)
    );
}

#[test]
fn remote_retry_requires_explicit_idempotency_marker() {
    let now = OffsetDateTime::now_utc();
    let mut remote = valid_remote_envelope(now);

    assert!(!remote.can_retry_after_current_attempt());

    remote.retry_policy = RemoteMessageRetryPolicy::Idempotent {
        max_attempts: 1,
        idempotency_key: "msg-key".to_string(),
    };
    assert_eq!(
        remote.validate_at(now),
        Err(RemoteMessageValidationError::InvalidRetryPolicy)
    );

    remote.retry_policy = RemoteMessageRetryPolicy::Idempotent {
        max_attempts: 2,
        idempotency_key: " ".to_string(),
    };
    assert_eq!(
        remote.validate_at(now),
        Err(RemoteMessageValidationError::MissingIdempotencyKey)
    );

    remote.retry_policy = RemoteMessageRetryPolicy::Idempotent {
        max_attempts: 2,
        idempotency_key: "msg-key".to_string(),
    };
    remote.validate_at(now).expect("idempotent retry is valid");
    assert!(remote.can_retry_after_current_attempt());

    remote.attempt = 2;
    assert!(!remote.can_retry_after_current_attempt());
}

#[test]
fn remote_message_validation_rejects_identity_expiry_and_revocation_failures() {
    let now = OffsetDateTime::now_utc();
    let remote = valid_remote_envelope(now);

    let mut missing_tenant = remote.clone();
    missing_tenant.tenant_id = TenantId::from(Uuid::nil());
    assert_eq!(
        missing_tenant.validate_at(now),
        Err(RemoteMessageValidationError::MissingTenantId)
    );

    let mut missing_source_instance = remote.clone();
    missing_source_instance.source_instance_id = " ".to_string();
    assert_eq!(
        missing_source_instance.validate_at(now),
        Err(RemoteMessageValidationError::MissingSourceInstanceId)
    );

    let mut missing_target_instance = remote.clone();
    missing_target_instance.target_instance_id = "".to_string();
    assert_eq!(
        missing_target_instance.validate_at(now),
        Err(RemoteMessageValidationError::MissingTargetInstanceId)
    );

    let mut same_instance = remote.clone();
    same_instance.target_instance_id = same_instance.source_instance_id.clone();
    assert_eq!(
        same_instance.validate_at(now),
        Err(RemoteMessageValidationError::SameSourceAndTargetInstance)
    );

    let mut invalid_attempt = remote.clone();
    invalid_attempt.attempt = 0;
    assert_eq!(
        invalid_attempt.validate_at(now),
        Err(RemoteMessageValidationError::InvalidAttempt)
    );

    let mut expired_envelope = remote.clone();
    expired_envelope.expires_at = Some(now);
    assert_eq!(
        expired_envelope.validate_at(now),
        Err(RemoteMessageValidationError::ExpiredEnvelope)
    );

    let mut revoked = remote;
    revoked.work_order.revocation = RevocationStatus::Revoked {
        reason: "operator".to_string(),
    };
    assert_eq!(
        revoked.validate_at(now),
        Err(RemoteMessageValidationError::RevokedWorkOrder {
            reason: "operator".to_string()
        })
    );
}
