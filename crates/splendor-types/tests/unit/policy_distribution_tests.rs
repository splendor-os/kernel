use super::*;
use time::{Duration, OffsetDateTime};

const KEY_ID: &str = "policy-test-key";
const SECRET: &[u8] = b"policy-test-secret";

fn keyring() -> PolicyBundleKeyring {
    let mut keyring = PolicyBundleKeyring::new();
    keyring
        .insert_shared_secret(KEY_ID, SECRET)
        .expect("insert key");
    keyring
}

fn bundle() -> PolicyBundle {
    let now = OffsetDateTime::now_utc();
    PolicyBundle {
        schema_version: POLICY_BUNDLE_SCHEMA_VERSION.to_string(),
        policy_bundle_id: PolicyBundleId::try_new("pol_test").expect("policy bundle id"),
        version: "2026.05.29".to_string(),
        tenant_id: TenantId::new(),
        agent_id: None,
        issued_at: now - Duration::minutes(1),
        expires_at: now + Duration::hours(1),
        revocation: RevocationStatus::Active,
        degraded_mode: PolicyDegradedMode {
            allow_low_risk_cached: true,
        },
    }
}

fn context(bundle: &PolicyBundle) -> PolicyBundleValidationContext {
    PolicyBundleValidationContext {
        tenant_id: bundle.tenant_id.clone(),
        agent_id: bundle.agent_id.clone(),
        now: OffsetDateTime::now_utc(),
    }
}

#[test]
fn signed_policy_bundle_validates_and_preserves_trace_metadata() {
    let bundle = bundle();
    let envelope = PolicyBundleEnvelope::signed_with_shared_secret(bundle.clone(), KEY_ID, SECRET)
        .expect("signed policy bundle");

    let validated = validate_policy_bundle(&envelope, &context(&bundle), &keyring())
        .expect("validated policy bundle");

    assert_eq!(validated.bundle().policy_bundle_id.as_str(), "pol_test");
    let trace = PolicyBundleTraceContext::from(validated.bundle());
    assert_eq!(trace.policy_bundle_id.as_str(), "pol_test");
    assert_eq!(trace.version, "2026.05.29");
    assert!(trace.degraded_mode.allow_low_risk_cached);
}

#[test]
fn policy_bundle_identity_and_keyring_inputs_validate_fail_closed() {
    assert_eq!(
        PolicyBundleId::try_new("   "),
        Err(PolicyBundleIdError::Empty)
    );

    let id = PolicyBundleId::try_new("pol_owned").expect("policy bundle id");
    let raw: String = id.into();
    assert_eq!(raw, "pol_owned");

    let mut keyring = PolicyBundleKeyring::new();
    let error = keyring
        .insert_shared_secret(" ", SECRET)
        .expect_err("empty key id denied");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");

    let error = keyring
        .insert_shared_secret(KEY_ID, b"")
        .expect_err("empty shared secret denied");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");

    let error = PolicyBundleEnvelope::signed_with_shared_secret(bundle(), KEY_ID, b"")
        .expect_err("empty signing secret denied");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");
}

#[test]
fn unsigned_or_bad_signature_policy_bundle_fails_closed() {
    let bundle = bundle();
    let unsigned = PolicyBundleEnvelope {
        bundle: bundle.clone(),
        signature: None,
    };

    let error = validate_policy_bundle(&unsigned, &context(&bundle), &keyring())
        .expect_err("unsigned bundle denied");
    assert_eq!(error.reason_code(), "unsigned_policy_bundle");

    let mut signed =
        PolicyBundleEnvelope::signed_with_shared_secret(bundle.clone(), KEY_ID, SECRET)
            .expect("signed policy bundle");
    signed.signature.as_mut().expect("signature").signature = "bad".to_string();
    let error = validate_policy_bundle(&signed, &context(&bundle), &keyring())
        .expect_err("bad signature denied");
    assert_eq!(error.reason_code(), "bad_policy_signature");

    let mut empty_key =
        PolicyBundleEnvelope::signed_with_shared_secret(bundle.clone(), KEY_ID, SECRET)
            .expect("signed policy bundle");
    empty_key.signature.as_mut().expect("signature").key_id = " ".to_string();
    let error = validate_policy_bundle(&empty_key, &context(&bundle), &keyring())
        .expect_err("empty signature key denied");
    assert_eq!(error.reason_code(), "unsigned_policy_bundle");

    let mut empty_signature =
        PolicyBundleEnvelope::signed_with_shared_secret(bundle.clone(), KEY_ID, SECRET)
            .expect("signed policy bundle");
    empty_signature
        .signature
        .as_mut()
        .expect("signature")
        .signature = " ".to_string();
    let error = validate_policy_bundle(&empty_signature, &context(&bundle), &keyring())
        .expect_err("empty signature denied");
    assert_eq!(error.reason_code(), "unsigned_policy_bundle");

    let mut unknown_key =
        PolicyBundleEnvelope::signed_with_shared_secret(bundle.clone(), KEY_ID, SECRET)
            .expect("signed policy bundle");
    unknown_key.signature.as_mut().expect("signature").key_id = "unknown-policy-key".to_string();
    let error = validate_policy_bundle(&unknown_key, &context(&bundle), &keyring())
        .expect_err("unknown key denied");
    assert_eq!(error.reason_code(), "unknown_policy_signature_key");
}

#[test]
fn malformed_policy_bundle_shape_branches_fail_closed() {
    let mut malformed = bundle();
    malformed.schema_version = "splendor.policy_bundle.v0".to_string();
    let envelope = PolicyBundleEnvelope {
        bundle: malformed.clone(),
        signature: None,
    };
    let error = validate_policy_bundle(&envelope, &context(&malformed), &keyring())
        .expect_err("malformed bundle denied before signature trust");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");

    let mut malformed = bundle();
    malformed.policy_bundle_id = PolicyBundleId("   ".to_string());
    let envelope = PolicyBundleEnvelope {
        bundle: malformed.clone(),
        signature: None,
    };
    let error = validate_policy_bundle(&envelope, &context(&malformed), &keyring())
        .expect_err("empty policy id denied before signature trust");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");

    let mut malformed = bundle();
    malformed.version = " ".to_string();
    let envelope = PolicyBundleEnvelope {
        bundle: malformed.clone(),
        signature: None,
    };
    let error = validate_policy_bundle(&envelope, &context(&malformed), &keyring())
        .expect_err("empty policy version denied before signature trust");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");

    let mut malformed = bundle();
    malformed.expires_at = malformed.issued_at;
    let envelope = PolicyBundleEnvelope {
        bundle: malformed.clone(),
        signature: None,
    };
    let error = validate_policy_bundle(&envelope, &context(&malformed), &keyring())
        .expect_err("non-increasing TTL denied before signature trust");
    assert_eq!(error.reason_code(), "malformed_policy_bundle");
}

#[test]
fn expired_revoked_and_wrong_scope_policy_bundles_fail_closed() {
    let mut expired = bundle();
    expired.issued_at = OffsetDateTime::now_utc() - Duration::hours(2);
    expired.expires_at = OffsetDateTime::now_utc() - Duration::hours(1);
    let envelope = PolicyBundleEnvelope::signed_with_shared_secret(expired.clone(), KEY_ID, SECRET)
        .expect("signed expired policy bundle");
    let error = validate_policy_bundle(&envelope, &context(&expired), &keyring())
        .expect_err("expired bundle denied");
    assert_eq!(error.reason_code(), "expired_policy_bundle");

    let mut revoked = bundle();
    revoked.revocation = RevocationStatus::Revoked {
        reason: "operator revoked".to_string(),
    };
    let envelope = PolicyBundleEnvelope::signed_with_shared_secret(revoked.clone(), KEY_ID, SECRET)
        .expect("signed revoked policy bundle");
    let error = validate_policy_bundle(&envelope, &context(&revoked), &keyring())
        .expect_err("revoked bundle denied");
    assert_eq!(error.reason_code(), "revoked_policy_bundle");

    let wrong_context = PolicyBundleValidationContext {
        tenant_id: TenantId::new(),
        agent_id: None,
        now: OffsetDateTime::now_utc(),
    };
    let good = bundle();
    let envelope = PolicyBundleEnvelope::signed_with_shared_secret(good.clone(), KEY_ID, SECRET)
        .expect("signed policy bundle");
    let error = validate_policy_bundle(&envelope, &wrong_context, &keyring())
        .expect_err("wrong tenant denied");
    assert_eq!(error.reason_code(), "incompatible_policy_bundle");

    let mut agent_scoped = bundle();
    agent_scoped.agent_id = Some(AgentId::new());
    let wrong_agent_context = PolicyBundleValidationContext {
        tenant_id: agent_scoped.tenant_id.clone(),
        agent_id: Some(AgentId::new()),
        now: OffsetDateTime::now_utc(),
    };
    let envelope =
        PolicyBundleEnvelope::signed_with_shared_secret(agent_scoped.clone(), KEY_ID, SECRET)
            .expect("signed agent-scoped policy bundle");
    let error = validate_policy_bundle(&envelope, &wrong_agent_context, &keyring())
        .expect_err("wrong agent denied");
    assert_eq!(error.reason_code(), "incompatible_policy_bundle");
}

#[test]
fn policy_bundle_serde_defaults_are_stable_and_trace_safe() {
    let mut value = serde_json::to_value(bundle()).expect("bundle json");
    let object = value.as_object_mut().expect("bundle object");
    object.remove("schema_version");
    object.remove("agent_id");
    object.remove("revocation");
    object.remove("degraded_mode");

    let decoded: PolicyBundle = serde_json::from_value(value).expect("defaulted bundle");

    assert_eq!(decoded.schema_version, POLICY_BUNDLE_SCHEMA_VERSION);
    assert_eq!(decoded.agent_id, None);
    assert_eq!(decoded.revocation, RevocationStatus::Active);
    assert!(!decoded.degraded_mode.allow_low_risk_cached);

    let envelope = PolicyBundleEnvelope::signed_with_shared_secret(decoded.clone(), KEY_ID, SECRET)
        .expect("signed defaulted bundle");
    let validated = validate_policy_bundle(&envelope, &context(&decoded), &keyring())
        .expect("validated defaulted bundle");
    assert_eq!(validated.into_policy_bundle(), decoded);
}
