use super::*;
use splendor_types::{
    AgentId, CircuitBreaker, CircuitBreakerId, CircuitBreakerScope, FleetId, InstanceId, NodeId,
    QuotaUsage, RunId, RuntimeIdentityContext, SideEffectClass, TenantId,
};
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
        run_id: RunId::new(),
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
        run_id: RunId::new(),
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
fn verified_gateway_denies_invalid_identity_before_adapter_execution() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());

    let mut request = base_request();
    request.run_id = RunId::from(uuid::Uuid::nil());

    let outcome = gateway.submit(request).expect("outcome");
    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(outcome
        .verification
        .reasons
        .contains(&"identity_invalid".to_string()));
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
fn tripped_adapter_breaker_denies_before_adapter_execution() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());
    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_adapter").expect("id"),
        CircuitBreakerScope::Adapter("adapter".to_string()),
        "adapter disabled",
        OffsetDateTime::now_utc(),
    )
    .expect("breaker");
    gateway
        .set_circuit_breaker_evaluator(Arc::new(StaticCircuitBreakerEvaluator::new(vec![breaker])));

    let outcome = gateway.submit(base_request()).expect("outcome");

    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert!(outcome
        .verification
        .reasons
        .contains(&"circuit_breaker_tripped".to_string()));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
    assert_eq!(
        outcome.verification.artifacts["circuit_breaker"]["circuit_breaker"]["scope"].as_str(),
        Some("adapter")
    );
}

#[test]
fn tenant_breaker_denies_only_matching_tenant() {
    let denied_tenant = TenantId::new();
    let allowed_tenant = TenantId::new();
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());
    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_tenant").expect("id"),
        CircuitBreakerScope::Tenant(denied_tenant.clone()),
        "tenant hold",
        OffsetDateTime::now_utc(),
    )
    .expect("breaker");
    gateway
        .set_circuit_breaker_evaluator(Arc::new(StaticCircuitBreakerEvaluator::new(vec![breaker])));

    let mut denied = base_request();
    denied.tenant_id = denied_tenant;
    let denied_outcome = gateway.submit(denied).expect("denied outcome");
    assert!(matches!(denied_outcome.status, ActionStatus::Denied));

    let mut allowed = base_request();
    allowed.tenant_id = allowed_tenant;
    let allowed_outcome = gateway.submit(allowed).expect("allowed outcome");
    assert!(matches!(allowed_outcome.status, ActionStatus::Executed));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 1);
}

#[test]
fn action_class_breaker_denies_matching_side_effect_class() {
    let tenant_access = Arc::new(TestTenantAccess {
        policy: VerificationResult::allow(),
        quota: VerificationResult::allow(),
    });
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    let adapter = Arc::new(CountingAdapter::default());
    gateway.register_adapter("noop", "adapter", adapter.clone());
    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_filesystem").expect("id"),
        CircuitBreakerScope::ActionClass(SideEffectClass::Filesystem),
        "filesystem disabled",
        OffsetDateTime::now_utc(),
    )
    .expect("breaker");
    gateway
        .set_circuit_breaker_evaluator(Arc::new(StaticCircuitBreakerEvaluator::new(vec![breaker])));

    let mut request = base_request();
    request.action.side_effect_class = SideEffectClass::Filesystem;
    let outcome = gateway.submit(request).expect("outcome");

    assert!(matches!(outcome.status, ActionStatus::Denied));
    assert_eq!(*adapter.calls.lock().expect("calls lock"), 0);
}

#[test]
fn node_and_instance_breakers_gate_runtime_admission() {
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let evaluator = StaticCircuitBreakerEvaluator::new(vec![
        CircuitBreaker::tripped(
            CircuitBreakerId::try_new("cb_node").expect("id"),
            CircuitBreakerScope::Node(node_id.clone()),
            "node unhealthy",
            OffsetDateTime::now_utc(),
        )
        .expect("node breaker"),
        CircuitBreaker::tripped(
            CircuitBreakerId::try_new("cb_instance").expect("id"),
            CircuitBreakerScope::Instance(instance_id.clone()),
            "instance draining",
            OffsetDateTime::now_utc(),
        )
        .expect("instance breaker"),
    ]);
    let matching_runtime = RuntimeIdentityContext {
        node_id: Some(node_id),
        instance_id: Some(instance_id),
        ..RuntimeIdentityContext::default()
    };
    let unrelated_runtime = RuntimeIdentityContext {
        node_id: Some(NodeId::new()),
        instance_id: Some(InstanceId::new()),
        ..RuntimeIdentityContext::default()
    };
    let unknown_runtime = RuntimeIdentityContext::default();

    assert!(
        !evaluator
            .verify_runtime_admission(&matching_runtime)
            .allowed
    );
    assert!(
        evaluator
            .verify_runtime_admission(&unrelated_runtime)
            .allowed
    );
    let unknown = evaluator.verify_runtime_admission(&unknown_runtime);
    assert!(!unknown.allowed);
    assert!(unknown
        .reasons
        .contains(&"circuit_breaker_scope_unknown".to_string()));
}

#[test]
fn static_evaluator_covers_all_supported_scopes() {
    let fleet_id = FleetId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let runtime = RuntimeIdentityContext {
        fleet_id: Some(fleet_id.clone()),
        node_id: Some(node_id.clone()),
        instance_id: Some(instance_id.clone()),
        tenant_id: None,
        agent_id: None,
    };
    let mut request = base_request();
    request.tenant_id = tenant_id.clone();
    request.agent_id = agent_id.clone();
    request.action.name = "write_file".to_string();
    request.action.side_effect_class = SideEffectClass::Filesystem;

    let scopes = vec![
        CircuitBreakerScope::Global,
        CircuitBreakerScope::Fleet(fleet_id),
        CircuitBreakerScope::Node(node_id),
        CircuitBreakerScope::Instance(instance_id),
        CircuitBreakerScope::Tenant(tenant_id),
        CircuitBreakerScope::Agent(agent_id),
        CircuitBreakerScope::Adapter("filesystem".to_string()),
        CircuitBreakerScope::Action("write_file".to_string()),
        CircuitBreakerScope::ActionClass(SideEffectClass::Filesystem),
    ];

    for (index, scope) in scopes.into_iter().enumerate() {
        let breaker = CircuitBreaker::tripped(
            CircuitBreakerId::try_new(format!("cb_scope_{index}")).expect("id"),
            scope,
            "scope disabled",
            OffsetDateTime::now_utc(),
        )
        .expect("breaker");
        let evaluator = StaticCircuitBreakerEvaluator::new(vec![breaker]);
        let result = evaluator.verify_action(&request, Some("filesystem"), &runtime);
        assert!(!result.allowed, "scope {index} should deny");
    }
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

#[test]
fn action_request_identity_validation_reports_each_nil_id() {
    let mut request = base_request();
    request.action_id = ActionId::from(uuid::Uuid::nil());
    let error = request.validate_identity().expect_err("nil action id");
    assert!(matches!(
        error,
        IdentityValidationError::Missing { field: "action_id" }
    ));

    let mut request = base_request();
    request.tenant_id = TenantId::from(uuid::Uuid::nil());
    let error = request.validate_identity().expect_err("nil tenant id");
    assert!(matches!(
        error,
        IdentityValidationError::Missing { field: "tenant_id" }
    ));

    let mut request = base_request();
    request.agent_id = AgentId::from(uuid::Uuid::nil());
    let error = request.validate_identity().expect_err("nil agent id");
    assert!(matches!(
        error,
        IdentityValidationError::Missing { field: "agent_id" }
    ));
}

#[test]
fn breaker_runtime_and_scope_helpers_cover_mismatch_and_unknown_branches() {
    let noop = NoopCircuitBreakerEvaluator;
    assert!(
        noop.verify_runtime_admission(&RuntimeIdentityContext::default())
            .allowed
    );

    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_fleet").expect("id"),
        CircuitBreakerScope::Fleet(FleetId::new()),
        "fleet disabled",
        OffsetDateTime::now_utc(),
    )
    .expect("breaker");
    let evaluator = StaticCircuitBreakerEvaluator::new(vec![breaker.clone()]);
    assert_eq!(evaluator.breakers().len(), 1);
    assert!(
        evaluator
            .verify_runtime_admission(&RuntimeIdentityContext {
                fleet_id: Some(FleetId::new()),
                ..RuntimeIdentityContext::default()
            })
            .allowed
    );
    let missing = evaluator.verify_runtime_admission(&RuntimeIdentityContext::default());
    assert!(!missing.allowed);
    assert_eq!(missing.artifacts["missing_identity"], "fleet_id");

    let node_scope = CircuitBreakerScope::Node(NodeId::new());
    assert_eq!(
        breaker_scope_matches(
            &node_scope,
            &RuntimeIdentityContext {
                node_id: Some(NodeId::new()),
                ..RuntimeIdentityContext::default()
            },
            None,
            None,
            true,
        ),
        BreakerScopeMatch::DoesNotMatch
    );
    assert_eq!(
        breaker_scope_matches(
            &node_scope,
            &RuntimeIdentityContext::default(),
            None,
            None,
            true,
        ),
        BreakerScopeMatch::Unknown("node_id")
    );

    let instance_scope = CircuitBreakerScope::Instance(InstanceId::new());
    assert_eq!(
        breaker_scope_matches(
            &instance_scope,
            &RuntimeIdentityContext {
                instance_id: Some(InstanceId::new()),
                ..RuntimeIdentityContext::default()
            },
            None,
            None,
            true,
        ),
        BreakerScopeMatch::DoesNotMatch
    );
    assert_eq!(
        breaker_scope_matches(
            &instance_scope,
            &RuntimeIdentityContext::default(),
            None,
            None,
            true,
        ),
        BreakerScopeMatch::Unknown("instance_id")
    );

    for scope in [
        CircuitBreakerScope::Tenant(TenantId::new()),
        CircuitBreakerScope::Agent(AgentId::new()),
        CircuitBreakerScope::Adapter("filesystem".to_string()),
        CircuitBreakerScope::Action("write_file".to_string()),
        CircuitBreakerScope::ActionClass(SideEffectClass::Filesystem),
    ] {
        assert_eq!(
            breaker_scope_matches(&scope, &RuntimeIdentityContext::default(), None, None, true,),
            BreakerScopeMatch::DoesNotMatch
        );
        assert!(matches!(
            breaker_scope_matches(
                &scope,
                &RuntimeIdentityContext::default(),
                None,
                None,
                false
            ),
            BreakerScopeMatch::Unknown(_)
        ));
    }
}

#[test]
fn attach_request_context_wraps_scalar_verifier_artifact() {
    let request = base_request();
    let mut verification = VerificationResult {
        allowed: false,
        reasons: vec!["scalar_artifact".to_string()],
        artifacts: serde_json::json!("scalar detail"),
    };

    attach_request_context(&mut verification, &request);

    assert_eq!(verification.artifacts["detail"], "scalar detail");
    assert_eq!(
        verification.artifacts["context"]["source"],
        "gateway_verifier_chain"
    );
}
