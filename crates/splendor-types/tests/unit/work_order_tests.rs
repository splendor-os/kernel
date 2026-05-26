use super::*;
use crate::{RevocationStatus, WorkOrderId};
use time::{Duration, OffsetDateTime};

const KEY_ID: &str = "local-test";
const SECRET: &[u8] = b"local-test-work-order-secret";

fn work_order(now: OffsetDateTime) -> WorkOrder {
    WorkOrder {
        schema_version: WORK_ORDER_SCHEMA_VERSION.to_string(),
        work_order_id: WorkOrderId::try_new("wo_unit").expect("work order id"),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id: Some(RunId::new()),
        objective: "run signed local resident workload".to_string(),
        allowed_actions: vec!["write_file".to_string()],
        allowed_adapters: vec!["filesystem".to_string()],
        allowed_permissions: vec!["fs.write".to_string()],
        data_refs: vec!["dataset:unit".to_string()],
        quotas: WorkOrderQuotaPolicy {
            max_actions_per_tick: Some(1),
            max_filesystem_write_bytes: Some(64),
            ..WorkOrderQuotaPolicy::default()
        },
        placement: WorkOrderPlacement {
            target: "local_resident".to_string(),
            data_locality: Some("local".to_string()),
            requires_gpu: Some(false),
            dedicated_instance: Some(false),
            required_capabilities: vec!["filesystem".to_string()],
            max_runtime_ms: Some(30_000),
        },
        issued_at: now - Duration::minutes(1),
        expires_at: now + Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn keyring() -> WorkOrderKeyring {
    let mut keyring = WorkOrderKeyring::new();
    keyring
        .insert_shared_secret(KEY_ID, SECRET)
        .expect("insert key");
    keyring
}

fn context(order: &WorkOrder, now: OffsetDateTime) -> WorkOrderValidationContext {
    WorkOrderValidationContext {
        tenant_id: order.tenant_id.clone(),
        agent_id: order.agent_id.clone(),
        run_id: order.run_id.clone(),
        expected_placement_target: Some(order.placement.target.clone()),
        now,
    }
}

#[test]
fn signed_work_order_validates_and_round_trips() {
    let now = OffsetDateTime::now_utc();
    let order = work_order(now);
    let envelope = WorkOrderEnvelope::signed_with_shared_secret(order.clone(), KEY_ID, SECRET)
        .expect("signed envelope");

    let payload = serde_json::to_value(&envelope).expect("serialize envelope");
    assert!(payload.get("signature").is_some());
    assert_eq!(
        payload.get("work_order_id").and_then(|v| v.as_str()),
        Some("wo_unit")
    );
    let signed_payload = serde_json::to_value(&order).expect("serialize payload");
    assert!(signed_payload.get("signature").is_none());

    let decoded: WorkOrderEnvelope = serde_json::from_value(payload).expect("deserialize");
    let decision = validate_work_order(&decoded, &context(&order, now), &keyring())
        .expect("work order validates");

    assert_eq!(decision.work_order().work_order_id.as_str(), "wo_unit");
    assert_eq!(decision.work_order().allowed_actions, vec!["write_file"]);
}

#[test]
fn unsigned_unknown_key_and_bad_signature_fail_closed() {
    let now = OffsetDateTime::now_utc();
    let order = work_order(now);
    let context = context(&order, now);

    let unsigned = WorkOrderEnvelope {
        work_order: order.clone(),
        signature: None,
    };
    assert_eq!(
        validate_work_order(&unsigned, &context, &keyring()),
        Err(WorkOrderValidationError::Unsigned)
    );

    let unknown_key =
        WorkOrderEnvelope::signed_with_shared_secret(order.clone(), "unknown", b"secret")
            .expect("signed");
    assert_eq!(
        validate_work_order(&unknown_key, &context, &keyring()),
        Err(WorkOrderValidationError::UnknownKey {
            key_id: "unknown".to_string()
        })
    );

    let mut bad_signature =
        WorkOrderEnvelope::signed_with_shared_secret(order.clone(), KEY_ID, SECRET)
            .expect("signed");
    bad_signature.signature.as_mut().unwrap().signature = "bad".to_string();
    assert_eq!(
        validate_work_order(&bad_signature, &context, &keyring()),
        Err(WorkOrderValidationError::BadSignature)
    );
}

#[test]
fn expired_revoked_malformed_and_incompatible_work_orders_are_rejected() {
    let now = OffsetDateTime::now_utc();
    let mut expired = work_order(now);
    expired.expires_at = now - Duration::seconds(1);
    let expired = WorkOrderEnvelope::signed_with_shared_secret(expired.clone(), KEY_ID, SECRET)
        .expect("signed expired");
    assert_eq!(
        validate_work_order(&expired, &context(&expired.work_order, now), &keyring()),
        Err(WorkOrderValidationError::Expired)
    );

    let mut revoked = work_order(now);
    revoked.revocation = RevocationStatus::Revoked {
        reason: "operator".to_string(),
    };
    let revoked = WorkOrderEnvelope::signed_with_shared_secret(revoked.clone(), KEY_ID, SECRET)
        .expect("signed revoked");
    assert_eq!(
        validate_work_order(&revoked, &context(&revoked.work_order, now), &keyring()),
        Err(WorkOrderValidationError::Revoked {
            reason: "operator".to_string()
        })
    );

    let mut malformed = work_order(now);
    malformed.allowed_actions.clear();
    let malformed = WorkOrderEnvelope {
        work_order: malformed.clone(),
        signature: Some(WorkOrderSignature {
            key_id: KEY_ID.to_string(),
            signature: "unused".to_string(),
        }),
    };
    assert!(matches!(
        validate_work_order(&malformed, &context(&malformed.work_order, now), &keyring()),
        Err(WorkOrderValidationError::Malformed { .. })
    ));

    let order = work_order(now);
    let envelope = WorkOrderEnvelope::signed_with_shared_secret(order.clone(), KEY_ID, SECRET)
        .expect("signed");
    let mut incompatible = context(&order, now);
    incompatible.agent_id = AgentId::new();
    assert_eq!(
        validate_work_order(&envelope, &incompatible, &keyring()),
        Err(WorkOrderValidationError::Incompatible {
            reason: "agent_mismatch".to_string()
        })
    );
}

#[test]
fn malformed_scope_and_context_mismatches_report_stable_codes() {
    let now = OffsetDateTime::now_utc();
    let mut order = work_order(now);
    order.schema_version = "splendor.work_order.v0".to_string();
    assert!(matches!(
        order.signing_payload_bytes(),
        Err(WorkOrderValidationError::Malformed { reason })
            if reason.contains("unsupported_schema_version")
    ));

    let mut empty_objective = work_order(now);
    empty_objective.objective = " ".to_string();
    assert!(matches!(
        empty_objective.signing_payload_bytes(),
        Err(WorkOrderValidationError::Malformed { reason })
            if reason == "empty_objective"
    ));

    let mut blank_permission = work_order(now);
    blank_permission.allowed_permissions.push(" ".to_string());
    assert!(matches!(
        blank_permission.signing_payload_bytes(),
        Err(WorkOrderValidationError::Malformed { reason })
            if reason == "blank_allowed_permissions"
    ));

    let mut blank_data_ref = work_order(now);
    blank_data_ref.data_refs.push("".to_string());
    assert!(matches!(
        blank_data_ref.signing_payload_bytes(),
        Err(WorkOrderValidationError::Malformed { reason })
            if reason == "blank_data_refs"
    ));

    let mut empty_target = work_order(now);
    empty_target.placement.target.clear();
    assert!(matches!(
        empty_target.signing_payload_bytes(),
        Err(WorkOrderValidationError::Malformed { reason })
            if reason == "empty_placement_target"
    ));

    let mut invalid_dates = work_order(now);
    invalid_dates.expires_at = invalid_dates.issued_at;
    assert!(matches!(
        invalid_dates.signing_payload_bytes(),
        Err(WorkOrderValidationError::Malformed { reason })
            if reason == "expires_at_not_after_issued_at"
    ));

    let mut empty_keyring = WorkOrderKeyring::new();
    assert!(matches!(
        empty_keyring.insert_shared_secret("", SECRET),
        Err(WorkOrderValidationError::Malformed { reason }) if reason == "empty_key_id"
    ));
    assert!(matches!(
        empty_keyring.insert_shared_secret(KEY_ID, b""),
        Err(WorkOrderValidationError::Malformed { reason }) if reason == "empty_shared_secret"
    ));

    let order = work_order(now);
    let envelope = WorkOrderEnvelope::signed_with_shared_secret(order.clone(), KEY_ID, SECRET)
        .expect("signed");
    let mut tenant_mismatch = context(&order, now);
    tenant_mismatch.tenant_id = TenantId::new();
    assert_eq!(
        validate_work_order(&envelope, &tenant_mismatch, &keyring()),
        Err(WorkOrderValidationError::Incompatible {
            reason: "tenant_mismatch".to_string()
        })
    );

    let mut run_mismatch = context(&order, now);
    run_mismatch.run_id = Some(RunId::new());
    assert_eq!(
        validate_work_order(&envelope, &run_mismatch, &keyring()),
        Err(WorkOrderValidationError::Incompatible {
            reason: "run_mismatch".to_string()
        })
    );

    let mut placement_mismatch = context(&order, now);
    placement_mismatch.expected_placement_target = Some("other_target".to_string());
    assert_eq!(
        validate_work_order(&envelope, &placement_mismatch, &keyring()),
        Err(WorkOrderValidationError::Incompatible {
            reason: "placement_target_mismatch".to_string()
        })
    );
}

#[test]
fn rejection_trace_does_not_contain_signature_material() {
    let now = OffsetDateTime::now_utc();
    let order = work_order(now);
    let event = crate::TraceEvent::new(
        order.run_id.clone().expect("run id"),
        0,
        now,
        crate::TraceEventKind::WorkOrderRejected {
            work_order_id: Some(order.work_order_id.clone()),
            tenant_id: Some(order.tenant_id.clone()),
            agent_id: Some(order.agent_id.clone()),
            run_id: order.run_id.clone(),
            reason: WorkOrderValidationError::BadSignature
                .reason_code()
                .to_string(),
        },
    );

    let encoded = serde_json::to_string(&event).expect("trace json");
    assert!(encoded.contains("bad_signature"));
    assert!(encoded.contains("wo_unit"));
    assert!(!encoded.contains(std::str::from_utf8(SECRET).unwrap()));
    assert!(!encoded.contains("\"signature\":"));
}
