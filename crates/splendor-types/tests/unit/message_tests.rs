use super::*;
use uuid::Uuid;

fn valid_message() -> Message {
    let source = AgentId::new();
    let target = AgentId::new();
    let run_id = RunId::new();
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target.clone(),
        "forecast revenue for Q3",
        DelegatedAuthority {
            allowed_actions: vec!["query".to_string()],
            allowed_adapters: vec!["sql".to_string()],
            allowed_permissions: vec!["finance.read".to_string()],
        },
    )
    .expect("valid task request");
    Message::new(
        MessageId::new(),
        source,
        target,
        run_id,
        TASK_REQUEST_SCHEMA,
        serde_json::to_value(task).expect("task json"),
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
        "schema": TASK_REQUEST_SCHEMA,
        "payload": {"objective": "forecast"},
        "causal_parent": null,
        "requires_response": true
    });
    let decoded = serde_json::from_value::<Message>(serialized_without_timestamp);
    assert!(decoded.is_err(), "created_at must be present");
}

#[test]
fn task_request_schema_requires_explicit_target_and_scoped_objective() {
    let parent_run_id = RunId::new();
    let child_run_id = RunId::new();
    let target_agent_id = AgentId::new();
    let request = TaskRequest::new(
        parent_run_id.clone(),
        child_run_id,
        target_agent_id.clone(),
        "summarize ledger",
        DelegatedAuthority::empty(),
    )
    .expect("valid task request");
    assert_eq!(request.parent_run_id, parent_run_id);
    assert_eq!(request.target_agent_id, target_agent_id);

    let missing_objective = TaskRequest::new(
        RunId::new(),
        RunId::new(),
        AgentId::new(),
        "  ",
        DelegatedAuthority::empty(),
    )
    .expect_err("objective required");
    assert!(missing_objective
        .to_string()
        .contains("objective is required"));

    let same_run = RunId::new();
    let same_child = TaskRequest::new(
        same_run.clone(),
        same_run,
        AgentId::new(),
        "summarize ledger",
        DelegatedAuthority::empty(),
    )
    .expect_err("child run must differ");
    assert!(same_child
        .to_string()
        .contains("child_run_id must be distinct"));
}

#[test]
fn task_request_payload_must_match_message_run_and_target() {
    let source = AgentId::new();
    let target = AgentId::new();
    let wrong_target = AgentId::new();
    let run_id = RunId::new();
    let request = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        wrong_target,
        "summarize ledger",
        DelegatedAuthority::empty(),
    )
    .expect("valid standalone request");

    let message = Message {
        message_id: MessageId::new(),
        source_agent_id: source,
        target_agent_id: target,
        run_id,
        schema: TASK_REQUEST_SCHEMA.to_string(),
        payload: serde_json::to_value(request).expect("payload"),
        causal_parent: None,
        requires_response: true,
        created_at: OffsetDateTime::now_utc(),
    };

    let error = message.validate().expect_err("target mismatch denied");
    assert!(error
        .to_string()
        .contains("target_agent_id must match message target_agent_id"));
}

#[test]
fn task_response_schema_requires_structured_failure() {
    let parent_run_id = RunId::new();
    let child_run_id = RunId::new();
    let failure = TaskFailure::new("child_failed", "specialist failed", false);
    let response = TaskResponse::new(
        parent_run_id.clone(),
        child_run_id.clone(),
        TaskResponseStatus::Failed,
        None,
        Some(failure.clone()),
    )
    .expect("failed response is structured");
    assert_eq!(response.failure, Some(failure));

    let missing_failure = TaskResponse::new(
        parent_run_id,
        child_run_id,
        TaskResponseStatus::Failed,
        None,
        None,
    )
    .expect_err("failure required");
    assert!(missing_failure
        .to_string()
        .contains("task_response requires failure"));

    let completed_with_failure = TaskResponse::new(
        RunId::new(),
        RunId::new(),
        TaskResponseStatus::Completed,
        Some(serde_json::json!({"ok": true})),
        Some(TaskFailure::new("unexpected", "unexpected failure", false)),
    )
    .expect_err("completed cannot include failure");
    assert!(completed_with_failure
        .to_string()
        .contains("completed task_response must not include failure"));

    let blank_failure = TaskResponse::new(
        RunId::new(),
        RunId::new(),
        TaskResponseStatus::Denied,
        None,
        Some(TaskFailure::new(" ", " ", false)),
    )
    .expect_err("failure code and reason required");
    assert!(blank_failure
        .to_string()
        .contains("failure code and reason are required"));
}

#[test]
fn delegated_authority_denies_missing_action_adapter_and_permission() {
    let authority = DelegatedAuthority {
        allowed_actions: vec!["query".to_string()],
        allowed_adapters: vec!["sql".to_string()],
        allowed_permissions: vec!["finance.read".to_string()],
    };
    let allowed = authority.verify_action("query", Some("sql"), &["finance.read".to_string()]);
    assert!(allowed.allowed);

    let missing_adapter = authority.verify_action("query", None, &["finance.read".to_string()]);
    assert!(!missing_adapter.allowed);
    assert!(missing_adapter
        .reasons
        .contains(&"delegated_adapter_unspecified".to_string()));

    let denied = authority.verify_action(
        "publish",
        Some("artifact"),
        &["artifact.publish".to_string()],
    );
    assert!(!denied.allowed);
    assert!(denied
        .reasons
        .contains(&"delegated_action_not_allowed".to_string()));
    assert!(denied
        .reasons
        .contains(&"delegated_adapter_not_allowed".to_string()));
    assert!(denied
        .reasons
        .contains(&"delegated_permission_denied".to_string()));

    let child = DelegatedAuthority {
        allowed_actions: vec!["query".to_string(), "publish".to_string()],
        allowed_adapters: vec!["sql".to_string()],
        allowed_permissions: vec!["finance.read".to_string()],
    };
    assert!(!child.is_subset_of(&authority));
}

#[test]
fn invalid_schema_versions_are_rejected_before_routing() {
    assert_eq!(MessageSchemaVersion::LATEST.suffix(), "v1");
    assert_eq!(
        MessageSchemaVersion::from_schema("v1"),
        Err(MessageValidationError::MissingSchemaVersion)
    );
    assert_eq!(
        MessageSchemaVersion::from_schema("splendor.message bad.v1"),
        Err(MessageValidationError::InvalidSchemaVersion {
            version: "splendor.message bad.v1".to_string()
        })
    );
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
fn task_request_validation_rejects_missing_ids_and_malformed_payload() {
    let valid = TaskRequest::new(
        RunId::new(),
        RunId::new(),
        AgentId::new(),
        "summarize ledger",
        DelegatedAuthority::empty(),
    )
    .expect("valid request");

    let mut missing_parent = valid.clone();
    missing_parent.parent_run_id = RunId::from(Uuid::nil());
    assert!(missing_parent
        .validate()
        .expect_err("parent id required")
        .to_string()
        .contains("parent_run_id is required"));

    let mut missing_child = valid.clone();
    missing_child.child_run_id = RunId::from(Uuid::nil());
    assert!(missing_child
        .validate()
        .expect_err("child id required")
        .to_string()
        .contains("child_run_id is required"));

    let mut missing_target = valid;
    missing_target.target_agent_id = AgentId::from(Uuid::nil());
    assert!(missing_target
        .validate()
        .expect_err("target id required")
        .to_string()
        .contains("target_agent_id is required"));

    let malformed = TaskRequest::from_payload(&serde_json::json!({"objective": "missing ids"}))
        .expect_err("malformed payload");
    assert!(malformed.to_string().contains("missing field"));
}

#[test]
fn task_response_validation_rejects_missing_ids_and_message_scope_mismatch() {
    let valid = TaskResponse::new(
        RunId::new(),
        RunId::new(),
        TaskResponseStatus::Completed,
        Some(serde_json::json!({"ok": true})),
        None,
    )
    .expect("valid response");

    let mut missing_parent = valid.clone();
    missing_parent.parent_run_id = RunId::from(Uuid::nil());
    assert!(missing_parent
        .validate()
        .expect_err("parent required")
        .to_string()
        .contains("parent_run_id is required"));

    let mut missing_child = valid.clone();
    missing_child.child_run_id = RunId::from(Uuid::nil());
    assert!(missing_child
        .validate()
        .expect_err("child required")
        .to_string()
        .contains("child_run_id is required"));

    let same = TaskResponse::new(
        valid.parent_run_id.clone(),
        valid.parent_run_id.clone(),
        TaskResponseStatus::Completed,
        None,
        None,
    )
    .expect_err("same run denied");
    assert!(same.to_string().contains("child_run_id must be distinct"));

    let message = Message {
        message_id: MessageId::new(),
        source_agent_id: AgentId::new(),
        target_agent_id: AgentId::new(),
        run_id: RunId::new(),
        schema: TASK_RESPONSE_SCHEMA.to_string(),
        payload: serde_json::to_value(valid).expect("response payload"),
        causal_parent: None,
        requires_response: false,
        created_at: OffsetDateTime::now_utc(),
    };
    let error = message.validate().expect_err("run mismatch denied");
    assert!(error
        .to_string()
        .contains("parent_run_id must match message run_id"));
}

#[test]
fn non_task_message_schema_keeps_payload_flexible() {
    let message = Message::new(
        MessageId::new(),
        AgentId::new(),
        AgentId::new(),
        RunId::new(),
        "splendor.message.note.v1",
        serde_json::json!({"freeform": true}),
        None,
        false,
        OffsetDateTime::now_utc(),
    )
    .expect("generic message accepted");

    assert_eq!(message.schema, "splendor.message.note.v1");
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
