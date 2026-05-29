use super::*;
use splendor_gateway::{ActionGateway, ActionId, ActionOutcome, ActionRequest, GatewayError};
use splendor_types::{
    Action, AgentId, PolicyBundle, PolicyBundleId, PolicyDegradedMode, QuotaUsage,
    RevocationStatus, RunId, SideEffectClass, TenantId,
};
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

struct CountingGateway {
    calls: Arc<Mutex<u32>>,
}

impl ActionGateway for CountingGateway {
    fn submit(&self, request: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        *self.calls.lock().expect("calls lock") += 1;
        Ok(ActionOutcome {
            action_id: request.action_id,
            status: ActionStatus::Executed,
            verification: VerificationResult::allow(),
            post_verification: Some(VerificationResult::allow()),
            output: Some(serde_json::json!({"ok": true})),
            error: None,
            completed_at: OffsetDateTime::now_utc(),
        })
    }
}

fn bundle(expires_at: OffsetDateTime, allow_low_risk_cached: bool) -> PolicyBundle {
    PolicyBundle {
        schema_version: splendor_types::POLICY_BUNDLE_SCHEMA_VERSION.to_string(),
        policy_bundle_id: PolicyBundleId::try_new("pol_cache").expect("policy id"),
        version: "v1".to_string(),
        tenant_id: TenantId::new(),
        agent_id: None,
        issued_at: expires_at - Duration::hours(1),
        expires_at,
        revocation: RevocationStatus::Active,
        degraded_mode: PolicyDegradedMode {
            allow_low_risk_cached,
        },
    }
}

fn request(side_effect_class: SideEffectClass) -> ActionRequest {
    ActionRequest {
        action_id: ActionId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id: RunId::new(),
        action: Action {
            name: "file.write".to_string(),
            params: serde_json::json!({}),
            side_effect_class,
            cost_estimate: None,
            required_permissions: vec![],
            preconditions: vec![],
            postconditions: vec![],
        },
        adapter: None,
        quota_usage: QuotaUsage::single_action(),
        satisfied_preconditions: vec![],
        requested_at: OffsetDateTime::now_utc(),
        approval_evidence: None,
    }
}

#[test]
fn non_enforced_empty_cache_allows_policy_and_gateway_forwarding() {
    let cache = PolicyCache::new(PolicyCacheConfig {
        enforcement_required: false,
    });

    let decision = cache.verify_policy_invocation("legacy", OffsetDateTime::now_utc());
    assert!(decision.verification.allowed);
    assert_eq!(decision.trace_event, None);

    let calls = Arc::new(Mutex::new(0));
    let gateway = PolicyDistributionGateway::new(
        Arc::new(CountingGateway {
            calls: calls.clone(),
        }),
        Arc::new(cache),
    );

    let outcome = gateway
        .submit(request(SideEffectClass::External))
        .expect("gateway outcome");

    assert_eq!(outcome.status, ActionStatus::Executed);
    assert_eq!(*calls.lock().expect("calls lock"), 1);
}

#[test]
fn missing_required_policy_fails_closed_before_policy_invocation() {
    let cache = PolicyCache::new(PolicyCacheConfig {
        enforcement_required: true,
    });

    let decision = cache.verify_policy_invocation("static", OffsetDateTime::now_utc());

    assert!(!decision.verification.allowed);
    assert_eq!(decision.verification.reasons, vec!["policy_unavailable"]);
    assert_eq!(decision.trace_event, None);
}

#[test]
fn missing_required_policy_fails_closed_before_inner_gateway() {
    let cache = PolicyCache::new(PolicyCacheConfig {
        enforcement_required: true,
    });
    let calls = Arc::new(Mutex::new(0));
    let gateway = PolicyDistributionGateway::new(
        Arc::new(CountingGateway {
            calls: calls.clone(),
        }),
        Arc::new(cache),
    );

    let outcome = gateway
        .submit(request(SideEffectClass::External))
        .expect("gateway outcome");

    assert_eq!(outcome.status, ActionStatus::Denied);
    assert_eq!(outcome.verification.reasons, vec!["policy_unavailable"]);
    assert_eq!(*calls.lock().expect("calls lock"), 0);
}

#[test]
fn expired_policy_denies_high_risk_but_allows_disconnected_low_risk_when_configured() {
    let now = OffsetDateTime::now_utc();
    let cache = PolicyCache::with_bundle(bundle(now - Duration::minutes(1), true), now);
    cache.set_disconnected(true);

    let denied = cache.verify_policy_action(&request(SideEffectClass::Network), now);
    assert!(!denied.allowed);
    assert_eq!(denied.reasons, vec!["policy_expired"]);

    let allowed = cache.verify_policy_action(&request(SideEffectClass::ReadOnly), now);
    assert!(allowed.allowed);
}

#[test]
fn expired_policy_blocks_policy_invocation_when_not_in_degraded_offline_mode() {
    let now = OffsetDateTime::now_utc();
    let cache = PolicyCache::with_bundle(bundle(now - Duration::minutes(1), false), now);

    let decision = cache.verify_policy_invocation("static", now);

    assert!(!decision.verification.allowed);
    assert_eq!(decision.verification.reasons, vec!["policy_expired"]);
    assert!(matches!(
        decision.trace_event,
        Some(TraceEventKind::PolicyExpired { .. })
    ));
}

#[test]
fn expired_policy_allows_policy_invocation_only_in_disconnected_degraded_mode() {
    let now = OffsetDateTime::now_utc();
    let cache = PolicyCache::with_bundle(bundle(now - Duration::minutes(1), true), now);
    cache.set_disconnected(true);

    let decision = cache.verify_policy_invocation("static", now);

    assert!(decision.verification.allowed);
    assert_eq!(decision.trace_event, None);
}

#[test]
fn revocation_blocks_policy_invocation_and_side_effects() {
    let now = OffsetDateTime::now_utc();
    let cache = PolicyCache::with_bundle(bundle(now + Duration::hours(1), true), now);
    cache.revoke_current("central_revocation");

    let decision = cache.verify_policy_invocation("static", now);
    assert!(!decision.verification.allowed);
    assert_eq!(decision.verification.reasons, vec!["policy_revoked"]);
    assert!(matches!(
        decision.trace_event,
        Some(TraceEventKind::PolicyRevoked { .. })
    ));

    let action = cache.verify_policy_action(&request(SideEffectClass::ReadOnly), now);
    assert!(!action.allowed);
    assert_eq!(action.reasons, vec!["policy_revoked"]);
}

#[test]
fn sync_failure_is_recorded_without_replacing_cached_bundle() {
    let now = OffsetDateTime::now_utc();
    let cache = PolicyCache::with_bundle(bundle(now + Duration::hours(1), true), now);

    let failure = cache.record_sync_failure("central_unavailable", now);
    let snapshot = cache.snapshot();

    assert_eq!(failure.reason, "central_unavailable");
    assert_eq!(
        snapshot.bundle.expect("bundle").policy_bundle_id.as_str(),
        "pol_cache"
    );
    assert_eq!(
        snapshot.last_sync_failure.expect("failure").reason,
        "central_unavailable"
    );
}

#[test]
fn policy_cache_sanitizes_trace_visible_reasons() {
    let now = OffsetDateTime::now_utc();
    let cache = PolicyCache::with_bundle(bundle(now + Duration::hours(1), true), now);

    let failure = cache.record_sync_failure("central failed token=super-secret", now);
    assert_eq!(failure.reason, "policy_reason_redacted");

    cache.revoke_current("operator revoked signature=raw-secret");
    let decision = cache.verify_policy_invocation("static", now);
    let Some(TraceEventKind::PolicyRevoked { reason, .. }) = decision.trace_event else {
        panic!("revocation should emit sanitized trace event");
    };
    assert_eq!(reason, "policy_reason_redacted");

    let action = cache.verify_policy_action(&request(SideEffectClass::External), now);
    assert_eq!(
        action.artifacts["reason"].as_str(),
        Some("policy_reason_redacted")
    );

    let unspecified = PolicyCache::with_bundle(bundle(now + Duration::hours(1), true), now);
    unspecified.revoke_current("   ");
    let decision = unspecified.verify_policy_invocation("static", now);
    let Some(TraceEventKind::PolicyRevoked { reason, .. }) = decision.trace_event else {
        panic!("empty revocation should emit sanitized trace event");
    };
    assert_eq!(reason, "policy_reason_unspecified");
}
