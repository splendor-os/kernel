use super::*;
use splendor_types::{AgentId, QuotaUsage, SideEffectClass, TenantId};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use super::combine_verifications;
use super::InvariantEvaluator;

fn block_on<F: Future>(mut future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(raw_waker()) };
    let mut context = Context::from_waker(&waker);
    let mut future = unsafe { Pin::new_unchecked(&mut future) };
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

fn raw_waker() -> RawWaker {
    fn no_op(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        raw_waker()
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
    RawWaker::new(std::ptr::null(), &VTABLE)
}

fn sample_action() -> ActionRequest {
    ActionRequest {
        action_id: ActionId::default(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        action: Action {
            name: "noop".to_string(),
            params: serde_json::json!({"ok": true}),
            side_effect_class: SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: vec!["test".to_string()],
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        },
        adapter: None,
        quota_usage: QuotaUsage::single_action(),
        satisfied_preconditions: Vec::new(),
        requested_at: OffsetDateTime::now_utc(),
    }
}

#[test]
fn unimplemented_gateway_denies_sync_and_async() {
    let gateway = UnimplementedGateway;
    let result = ActionGateway::submit(&gateway, sample_action());
    assert!(matches!(result, Err(GatewayError::Unimplemented)));

    let async_result = block_on(AsyncActionGateway::submit(&gateway, sample_action()));
    assert!(matches!(async_result, Err(GatewayError::Unimplemented)));
}

#[test]
fn gateway_error_strings_include_details() {
    let verification = GatewayError::VerificationFailed("quota".to_string());
    let adapter = GatewayError::AdapterFailed("timeout".to_string());
    assert!(verification.to_string().contains("quota"));
    assert!(adapter.to_string().contains("timeout"));
}

#[derive(Clone)]
struct TestTenantAccess {
    policy: VerificationResult,
    quota: VerificationResult,
}

#[derive(Clone)]
struct AgentScopedAccess {
    allowed_agent: AgentId,
}

impl TenantAccess for AgentScopedAccess {
    fn verify_policy(
        &self,
        _tenant_id: &TenantId,
        agent_id: &AgentId,
        _action: &Action,
        _adapter: Option<&str>,
    ) -> VerificationResult {
        if agent_id == &self.allowed_agent {
            VerificationResult::allow()
        } else {
            VerificationResult {
                allowed: false,
                reasons: vec!["agent_permission_denied".to_string()],
                artifacts: serde_json::json!({
                    "source": "agent_isolation_ledger",
                    "agent_id": agent_id.to_string(),
                }),
            }
        }
    }

    fn verify_quota(
        &self,
        _tenant_id: &TenantId,
        _agent_id: &AgentId,
        _usage: QuotaUsage,
    ) -> VerificationResult {
        VerificationResult::allow()
    }
}

#[derive(Clone)]
struct CountingQuotaAccess {
    quota_calls: Arc<AtomicUsize>,
}

impl TenantAccess for CountingQuotaAccess {
    fn verify_policy(
        &self,
        _tenant_id: &TenantId,
        _agent_id: &AgentId,
        _action: &Action,
        _adapter: Option<&str>,
    ) -> VerificationResult {
        VerificationResult::deny("policy")
    }

    fn verify_quota(
        &self,
        _tenant_id: &TenantId,
        _agent_id: &AgentId,
        _usage: QuotaUsage,
    ) -> VerificationResult {
        self.quota_calls.fetch_add(1, Ordering::SeqCst);
        VerificationResult::allow()
    }
}

impl TenantAccess for TestTenantAccess {
    fn verify_policy(
        &self,
        _tenant_id: &TenantId,
        _agent_id: &AgentId,
        _action: &Action,
        _adapter: Option<&str>,
    ) -> VerificationResult {
        self.policy.clone()
    }

    fn verify_quota(
        &self,
        _tenant_id: &TenantId,
        _agent_id: &AgentId,
        _usage: QuotaUsage,
    ) -> VerificationResult {
        self.quota.clone()
    }
}

#[derive(Default)]
struct CountingAdapter {
    calls: std::sync::Mutex<u32>,
    satisfied: Vec<String>,
}

impl ActionAdapter for CountingAdapter {
    fn execute(&self, _action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
        *self.calls.lock().expect("calls lock") += 1;
        Ok(AdapterResult {
            output: serde_json::json!({"ok": true}),
            satisfied_postconditions: self.satisfied.clone(),
        })
    }
}

fn base_request() -> ActionRequest {
    ActionRequest {
        action_id: ActionId::default(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        action: Action {
            name: "noop".to_string(),
            params: serde_json::json!({"ok": true}),
            side_effect_class: SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: vec![],
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        },
        adapter: None,
        quota_usage: QuotaUsage::single_action(),
        satisfied_preconditions: Vec::new(),
        requested_at: OffsetDateTime::now_utc(),
    }
}

#[test]
fn verified_gateway_denies_on_policy_failure() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::deny("policy"),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let outcome = gateway.submit(base_request()).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(!outcome.verification.allowed);
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn verified_gateway_does_not_consume_quota_when_policy_denies() {
    let quota_calls = Arc::new(AtomicUsize::new(0));
    let tenant_access = Arc::new(CountingQuotaAccess {
        quota_calls: Arc::clone(&quota_calls),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let outcome = gateway.submit(base_request()).expect("outcome");

    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert_eq!(quota_calls.load(Ordering::SeqCst), 0);
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn verified_gateway_passes_agent_id_to_policy_and_denies_laundering() {
    let allowed_agent = AgentId::new();
    let denied_agent = AgentId::new();
    let tenant_access = Arc::new(AgentScopedAccess {
        allowed_agent: allowed_agent.clone(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let mut request = base_request();
    request.agent_id = denied_agent.clone();
    request.action.required_permissions = vec!["agent:allowed".to_string()];
    let outcome = gateway.submit(request).expect("outcome");

    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(outcome
        .verification
        .reasons
        .contains(&"agent_permission_denied".to_string()));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
    assert_eq!(
        outcome.verification.artifacts["policy"]["agent_id"].as_str(),
        Some(denied_agent.to_string().as_str())
    );

    let mut allowed_request = base_request();
    allowed_request.agent_id = allowed_agent;
    let allowed = gateway.submit(allowed_request).expect("allowed outcome");
    assert!(matches!(allowed.status, ActionStatus::Executed));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 1);
}

#[test]
fn verified_gateway_denies_on_quota_failure() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::deny("quota"),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let outcome = gateway.submit(base_request()).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(outcome.verification.reasons.contains(&"quota".to_string()));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn verified_gateway_denies_on_precondition_failure() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let mut request = base_request();
    request.action.preconditions = vec!["ready".to_string()];
    request.satisfied_preconditions = Vec::new();

    let outcome = gateway.submit(request).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(!outcome.verification.allowed);
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn verified_gateway_denies_adapter_mismatch() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let mut request = base_request();
    request.adapter = Some("different".to_string());

    let outcome = gateway.submit(request).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(outcome
        .verification
        .reasons
        .contains(&"adapter_mismatch".to_string()));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn verified_gateway_reports_postcondition_failure() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter {
        satisfied: Vec::new(),
        ..CountingAdapter::default()
    });
    gateway.register_adapter("noop", "adapter", adapter);

    let mut request = base_request();
    request.action.postconditions = vec!["done".to_string()];

    let outcome = gateway.submit(request).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Failed));
    assert!(outcome.post_verification.is_some());
    assert!(!outcome.post_verification.expect("post").allowed);
}

#[test]
fn verified_gateway_executes_when_checks_pass() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let outcome = gateway.submit(base_request()).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Executed));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 1);
    assert!(outcome.output.is_some());
}

#[test]
fn verified_gateway_returns_error_when_adapter_missing() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let gateway = VerifiedActionGateway::new(tenant_access);
    let error = gateway.submit(base_request()).expect_err("missing adapter");
    assert!(matches!(error, GatewayError::AdapterFailed(_)));
}

#[test]
fn verified_gateway_reports_adapter_failure() {
    struct FailingAdapter;

    impl ActionAdapter for FailingAdapter {
        fn execute(&self, _action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
            Err(AdapterError::Failed("boom".to_string()))
        }
    }

    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("noop", "adapter", Arc::new(FailingAdapter));

    let outcome = gateway.submit(base_request()).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Failed));
    assert!(outcome.error.unwrap_or_default().contains("boom"));
    assert!(outcome.output.is_none());
    assert!(outcome.post_verification.is_none());
}

#[test]
fn verified_gateway_allows_when_preconditions_satisfied() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let mut request = base_request();
    request.action.preconditions = vec!["ready".to_string()];
    request.satisfied_preconditions = vec!["ready".to_string()];

    let outcome = gateway.submit(request).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Executed));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 1);
}

#[test]
fn verified_gateway_denies_with_empty_reasons() {
    struct EmptyReasonInvariant;

    impl InvariantEvaluator for EmptyReasonInvariant {
        fn verify_pre(
            &self,
            _action: &Action,
            _satisfied_preconditions: &[String],
        ) -> VerificationResult {
            VerificationResult {
                allowed: false,
                reasons: Vec::new(),
                artifacts: serde_json::json!({"detail": "missing"}),
            }
        }

        fn verify_post(
            &self,
            _action: &Action,
            _satisfied_postconditions: &[String],
        ) -> VerificationResult {
            VerificationResult::allow()
        }
    }

    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());
    gateway.set_invariant_evaluator(Arc::new(EmptyReasonInvariant));

    let outcome = gateway.submit(base_request()).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert_eq!(outcome.error.as_deref(), Some("verification denied"));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn combine_verifications_denies_when_reasons_empty() {
    let result = combine_verifications([
        (
            "policy",
            VerificationResult {
                allowed: false,
                reasons: Vec::new(),
                artifacts: serde_json::json!({"detail": "missing"}),
            },
        ),
        ("quota", VerificationResult::allow()),
    ]);

    assert!(!result.allowed);
    assert!(result.reasons.is_empty());
    assert!(result.artifacts["policy"].is_object());
}
