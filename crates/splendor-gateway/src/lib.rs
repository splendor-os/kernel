//! # Action Gateway Primitives
//!
//! The gateway mediates all side-effectful operations by wrapping actions,
//! capturing outcomes, and surfacing errors back to the kernel. The traits and
//! request/response types define the contract that later adapters implement.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_gateway::{ActionGateway, ActionRequest, UnimplementedGateway};
//! use splendor_types::{Action, SideEffectClass};
//! use time::OffsetDateTime;
//!
//! let gateway = UnimplementedGateway::default();
//! let request = ActionRequest {
//!     action_id: Default::default(),
//!     tenant_id: splendor_types::TenantId::new(),
//!     agent_id: splendor_types::AgentId::new(),
//!     run_id: splendor_types::RunId::new(),
//!     action: Action {
//!         name: "noop".into(),
//!         params: serde_json::json!({}),
//!         side_effect_class: SideEffectClass::ReadOnly,
//!         cost_estimate: None,
//!         required_permissions: vec![],
//!         preconditions: vec![],
//!         postconditions: vec![],
//!     },
//!     adapter: None,
//!     quota_usage: splendor_types::QuotaUsage::single_action(),
//!     satisfied_preconditions: vec![],
//!     requested_at: OffsetDateTime::now_utc(),
//! };
//! assert!(ActionGateway::submit(&gateway, request).is_err());
//! ```

use serde::{Deserialize, Serialize};
use splendor_types::{
    Action, AgentId, CircuitBreaker, CircuitBreakerScope, IdentityValidationError, QuotaUsage,
    RunId, RuntimeIdentityContext, TenantId, VerificationResult,
};
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::sync::Arc;
use time::OffsetDateTime;

pub use splendor_types::ActionId;

/// Request payload submitted to the action gateway.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionRequest {
    /// Unique action identifier assigned by the kernel.
    pub action_id: ActionId,
    /// Tenant identifier that owns the action.
    pub tenant_id: TenantId,
    /// Agent identifier that submitted the action.
    pub agent_id: AgentId,
    /// Run identifier that scopes the action and its trace events.
    pub run_id: RunId,
    /// Action details to execute.
    pub action: Action,
    /// Adapter identifier requested for this action.
    pub adapter: Option<String>,
    /// Quota usage estimate for this action.
    pub quota_usage: QuotaUsage,
    /// Preconditions satisfied by the current state.
    pub satisfied_preconditions: Vec<String>,
    /// Timestamp when the action was requested.
    pub requested_at: OffsetDateTime,
}

impl ActionRequest {
    /// Validates action, tenant, agent, and run identities before adapter execution.
    pub fn validate_identity(&self) -> Result<(), IdentityValidationError> {
        if self.action_id.is_nil() {
            return Err(IdentityValidationError::Missing { field: "action_id" });
        }
        if self.tenant_id.is_nil() {
            return Err(IdentityValidationError::Missing { field: "tenant_id" });
        }
        if self.agent_id.is_nil() {
            return Err(IdentityValidationError::Missing { field: "agent_id" });
        }
        if self.run_id.is_nil() {
            return Err(IdentityValidationError::Missing { field: "run_id" });
        }
        Ok(())
    }
}

/// Outcome captured after verification and execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionOutcome {
    /// Identifier of the action that completed.
    pub action_id: ActionId,
    /// Final status recorded by the gateway.
    pub status: ActionStatus,
    /// Verification result from the pre-execution pipeline.
    pub verification: VerificationResult,
    /// Optional post-execution verification result.
    pub post_verification: Option<VerificationResult>,
    /// Optional output payload from the adapter.
    pub output: Option<serde_json::Value>,
    /// Optional error message for denied or failed actions.
    pub error: Option<String>,
    /// Timestamp when the outcome was recorded.
    pub completed_at: OffsetDateTime,
}

/// Classification of action execution outcomes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActionStatus {
    /// Action executed successfully.
    Executed,
    /// Action was denied by verification.
    Denied,
    /// Action failed during adapter execution.
    Failed,
}

/// Result returned by action adapters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdapterResult {
    /// Output payload returned by the adapter.
    pub output: serde_json::Value,
    /// Postconditions satisfied by the adapter execution.
    pub satisfied_postconditions: Vec<String>,
}

/// Action adapter interface for side-effectful execution.
pub trait ActionAdapter: Send + Sync {
    /// Executes the action request and returns the adapter result.
    fn execute(&self, action: &ActionRequest) -> Result<AdapterResult, AdapterError>;
}

/// Errors returned by action adapters.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// Adapter execution failed.
    #[error("adapter failed: {0}")]
    Failed(String),
}

/// Accessor trait for tenant policy and quota checks.
pub trait TenantAccess: Send + Sync {
    /// Verifies action permissions for the tenant.
    fn verify_policy(
        &self,
        tenant_id: &TenantId,
        agent_id: &AgentId,
        action: &Action,
        adapter: Option<&str>,
    ) -> VerificationResult;
    /// Verifies quota usage for the tenant and agent.
    fn verify_quota(
        &self,
        tenant_id: &TenantId,
        agent_id: &AgentId,
        usage: QuotaUsage,
    ) -> VerificationResult;
}

/// Evaluates invariant preconditions and postconditions.
pub trait InvariantEvaluator: Send + Sync {
    /// Verifies preconditions against the current context.
    fn verify_pre(&self, action: &Action, satisfied_preconditions: &[String])
        -> VerificationResult;
    /// Verifies postconditions against adapter results.
    fn verify_post(
        &self,
        action: &Action,
        satisfied_postconditions: &[String],
    ) -> VerificationResult;
}

/// Evaluates tripped circuit breakers before adapter execution.
pub trait CircuitBreakerEvaluator: Send + Sync {
    /// Verifies whether the action is allowed under the current breaker state.
    fn verify_action(
        &self,
        action: &ActionRequest,
        adapter: Option<&str>,
        runtime_identity: &RuntimeIdentityContext,
    ) -> VerificationResult;

    /// Verifies whether the runtime may accept new local work.
    fn verify_runtime_admission(
        &self,
        _runtime_identity: &RuntimeIdentityContext,
    ) -> VerificationResult {
        VerificationResult::allow()
    }
}

/// Circuit-breaker evaluator with no configured breakers.
#[derive(Clone, Debug, Default)]
pub struct NoopCircuitBreakerEvaluator;

impl CircuitBreakerEvaluator for NoopCircuitBreakerEvaluator {
    fn verify_action(
        &self,
        _action: &ActionRequest,
        _adapter: Option<&str>,
        _runtime_identity: &RuntimeIdentityContext,
    ) -> VerificationResult {
        VerificationResult::allow()
    }
}

/// Static local circuit-breaker evaluator used by the local config path.
#[derive(Clone, Debug, Default)]
pub struct StaticCircuitBreakerEvaluator {
    breakers: Vec<CircuitBreaker>,
}

impl StaticCircuitBreakerEvaluator {
    /// Creates an evaluator from explicit breaker control objects.
    pub fn new(breakers: Vec<CircuitBreaker>) -> Self {
        Self { breakers }
    }

    /// Returns configured breakers.
    pub fn breakers(&self) -> &[CircuitBreaker] {
        &self.breakers
    }
}

impl CircuitBreakerEvaluator for StaticCircuitBreakerEvaluator {
    fn verify_action(
        &self,
        action: &ActionRequest,
        adapter: Option<&str>,
        runtime_identity: &RuntimeIdentityContext,
    ) -> VerificationResult {
        evaluate_breakers(
            &self.breakers,
            runtime_identity,
            Some(action),
            adapter,
            false,
        )
    }

    fn verify_runtime_admission(
        &self,
        runtime_identity: &RuntimeIdentityContext,
    ) -> VerificationResult {
        evaluate_breakers(&self.breakers, runtime_identity, None, None, true)
    }
}

/// Invariant evaluator that checks declared conditions against satisfied lists.
#[derive(Clone, Debug, Default)]
pub struct SimpleInvariantEvaluator;

impl InvariantEvaluator for SimpleInvariantEvaluator {
    fn verify_pre(
        &self,
        action: &Action,
        satisfied_preconditions: &[String],
    ) -> VerificationResult {
        check_conditions(
            "precondition_missing",
            &action.preconditions,
            satisfied_preconditions,
        )
    }

    fn verify_post(
        &self,
        action: &Action,
        satisfied_postconditions: &[String],
    ) -> VerificationResult {
        check_conditions(
            "postcondition_missing",
            &action.postconditions,
            satisfied_postconditions,
        )
    }
}

#[derive(Clone)]
struct AdapterRegistration {
    adapter_id: String,
    adapter: Arc<dyn ActionAdapter>,
}

/// Gateway implementation that runs verifier pipelines before execution.
pub struct VerifiedActionGateway {
    adapters: HashMap<String, AdapterRegistration>,
    tenant_access: Arc<dyn TenantAccess>,
    invariant_evaluator: Arc<dyn InvariantEvaluator>,
    circuit_breaker_evaluator: Arc<dyn CircuitBreakerEvaluator>,
    runtime_identity: RuntimeIdentityContext,
}

impl VerifiedActionGateway {
    /// Creates a gateway with the provided tenant access.
    pub fn new(tenant_access: Arc<dyn TenantAccess>) -> Self {
        Self {
            adapters: HashMap::new(),
            tenant_access,
            invariant_evaluator: Arc::new(SimpleInvariantEvaluator),
            circuit_breaker_evaluator: Arc::new(NoopCircuitBreakerEvaluator),
            runtime_identity: RuntimeIdentityContext::default(),
        }
    }

    /// Registers an adapter for the given action name.
    pub fn register_adapter(
        &mut self,
        action_name: impl Into<String>,
        adapter_id: impl Into<String>,
        adapter: Arc<dyn ActionAdapter>,
    ) {
        self.adapters.insert(
            action_name.into(),
            AdapterRegistration {
                adapter_id: adapter_id.into(),
                adapter,
            },
        );
    }

    /// Overrides the invariant evaluator used by the gateway.
    pub fn set_invariant_evaluator(&mut self, evaluator: Arc<dyn InvariantEvaluator>) {
        self.invariant_evaluator = evaluator;
    }

    /// Overrides the circuit-breaker evaluator used by the gateway.
    pub fn set_circuit_breaker_evaluator(&mut self, evaluator: Arc<dyn CircuitBreakerEvaluator>) {
        self.circuit_breaker_evaluator = evaluator;
    }

    /// Sets runtime identity used for fleet/node/instance scoped breakers.
    pub fn set_runtime_identity(&mut self, identity: RuntimeIdentityContext) {
        self.runtime_identity = identity;
    }

    /// Evaluates runtime-scoped breakers before accepting new local work.
    pub fn verify_runtime_admission(&self) -> VerificationResult {
        self.circuit_breaker_evaluator
            .verify_runtime_admission(&self.runtime_identity)
    }
}

impl ActionGateway for VerifiedActionGateway {
    fn submit(&self, action: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        if let Err(error) = action.validate_identity() {
            return Ok(identity_denied_outcome(action.action_id, error));
        }

        let registration = self
            .adapters
            .get(&action.action.name)
            .ok_or_else(|| GatewayError::AdapterFailed("adapter not registered".to_string()))?;
        if let Some(adapter) = action.adapter.as_deref() {
            if adapter != registration.adapter_id {
                let verification = VerificationResult {
                    allowed: false,
                    reasons: vec!["adapter_mismatch".to_string()],
                    artifacts: serde_json::json!({
                        "context": request_context(&action, vec!["adapter".to_string()]),
                        "requested": adapter,
                        "registered": registration.adapter_id,
                    }),
                };
                return Ok(ActionOutcome {
                    action_id: action.action_id,
                    status: ActionStatus::Denied,
                    verification,
                    post_verification: None,
                    output: None,
                    error: Some("adapter_mismatch".to_string()),
                    completed_at: OffsetDateTime::now_utc(),
                });
            }
        }
        let adapter_id = action
            .adapter
            .as_deref()
            .unwrap_or(registration.adapter_id.as_str());

        let breaker_result = self.circuit_breaker_evaluator.verify_action(
            &action,
            Some(adapter_id),
            &self.runtime_identity,
        );
        if !breaker_result.allowed {
            let mut verification = combine_verifications([("circuit_breaker", breaker_result)]);
            attach_request_context(&mut verification, &action);
            return Ok(denied_outcome(action.action_id, verification));
        }

        let policy_result = self.tenant_access.verify_policy(
            &action.tenant_id,
            &action.agent_id,
            &action.action,
            Some(adapter_id),
        );
        let invariant_pre = self
            .invariant_evaluator
            .verify_pre(&action.action, &action.satisfied_preconditions);
        let mut verification =
            combine_verifications([("policy", policy_result), ("invariant", invariant_pre)]);

        if !verification.allowed {
            attach_request_context(&mut verification, &action);
            return Ok(denied_outcome(action.action_id, verification));
        }

        let quota_result = self.tenant_access.verify_quota(
            &action.tenant_id,
            &action.agent_id,
            action.quota_usage,
        );
        verification = combine_verifications([("quota", quota_result)]);
        if !verification.allowed {
            attach_request_context(&mut verification, &action);
            return Ok(denied_outcome(action.action_id, verification));
        }

        let adapter_result = match registration.adapter.execute(&action) {
            Ok(result) => result,
            Err(error) => {
                return Ok(ActionOutcome {
                    action_id: action.action_id,
                    status: ActionStatus::Failed,
                    verification,
                    post_verification: None,
                    output: None,
                    error: Some(error.to_string()),
                    completed_at: OffsetDateTime::now_utc(),
                })
            }
        };

        let post_verification = self
            .invariant_evaluator
            .verify_post(&action.action, &adapter_result.satisfied_postconditions);
        let status = if post_verification.allowed {
            ActionStatus::Executed
        } else {
            ActionStatus::Failed
        };
        let error = if post_verification.allowed {
            None
        } else {
            Some(post_verification.reasons.join(", "))
        };

        Ok(ActionOutcome {
            action_id: action.action_id,
            status,
            verification,
            post_verification: Some(post_verification),
            output: Some(adapter_result.output),
            error,
            completed_at: OffsetDateTime::now_utc(),
        })
    }
}

fn identity_denied_outcome(action_id: ActionId, error: IdentityValidationError) -> ActionOutcome {
    ActionOutcome {
        action_id,
        status: ActionStatus::Denied,
        verification: VerificationResult {
            allowed: false,
            reasons: vec!["identity_invalid".to_string()],
            artifacts: serde_json::json!({
                "error": error.to_string(),
            }),
        },
        post_verification: None,
        output: None,
        error: Some(error.to_string()),
        completed_at: OffsetDateTime::now_utc(),
    }
}

/// Synchronous action gateway interface.
pub trait ActionGateway: Send + Sync {
    /// Submits an `ActionRequest` and returns an `ActionOutcome`.
    fn submit(&self, action: ActionRequest) -> Result<ActionOutcome, GatewayError>;
}

/// Asynchronous action gateway interface.
pub trait AsyncActionGateway: Send + Sync {
    /// Future returned by `submit`.
    type SubmitFuture<'a>: Future<Output = Result<ActionOutcome, GatewayError>> + Send + 'a
    where
        Self: 'a;

    /// Submits an `ActionRequest` asynchronously.
    fn submit<'a>(&'a self, action: ActionRequest) -> Self::SubmitFuture<'a>;
}

/// Placeholder gateway implementation used during early milestones.
#[derive(Default)]
pub struct UnimplementedGateway;

impl ActionGateway for UnimplementedGateway {
    /// Always returns `GatewayError::Unimplemented`.
    fn submit(&self, _action: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        Err(GatewayError::Unimplemented)
    }
}

impl AsyncActionGateway for UnimplementedGateway {
    type SubmitFuture<'a>
        = Ready<Result<ActionOutcome, GatewayError>>
    where
        Self: 'a;

    /// Async wrapper that returns `GatewayError::Unimplemented`.
    fn submit<'a>(&'a self, action: ActionRequest) -> Self::SubmitFuture<'a> {
        ready(ActionGateway::submit(self, action))
    }
}

/// Errors produced by action gateway implementations.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// Gateway has not been implemented yet.
    #[error("gateway is not implemented yet")]
    Unimplemented,
    /// Verification denied the requested action.
    #[error("action verification failed: {0}")]
    VerificationFailed(String),
    /// Adapter failed to execute the action.
    #[error("adapter execution failed: {0}")]
    AdapterFailed(String),
}

fn check_conditions(reason: &str, expected: &[String], satisfied: &[String]) -> VerificationResult {
    if expected.is_empty() {
        return VerificationResult::allow();
    }
    let missing = expected
        .iter()
        .filter(|condition| !satisfied.iter().any(|value| value == *condition))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return VerificationResult::allow();
    }
    VerificationResult {
        allowed: false,
        reasons: vec![reason.to_string()],
        artifacts: serde_json::json!({
            "expected": expected,
            "satisfied": satisfied,
            "missing": missing,
        }),
    }
}

fn combine_verifications(
    results: impl IntoIterator<Item = (&'static str, VerificationResult)>,
) -> VerificationResult {
    let mut reasons = Vec::new();
    let mut artifacts = serde_json::Map::new();
    let mut denied = false;
    for (label, result) in results {
        if result.allowed {
            continue;
        }
        denied = true;
        reasons.extend(result.reasons);
        if !result.artifacts.is_null() {
            artifacts.insert(label.to_string(), result.artifacts);
        }
    }
    if !denied {
        VerificationResult::allow()
    } else {
        VerificationResult {
            allowed: false,
            reasons,
            artifacts: serde_json::Value::Object(artifacts),
        }
    }
}

fn evaluate_breakers(
    breakers: &[CircuitBreaker],
    runtime_identity: &RuntimeIdentityContext,
    action: Option<&ActionRequest>,
    adapter: Option<&str>,
    runtime_admission_only: bool,
) -> VerificationResult {
    for breaker in breakers.iter().filter(|breaker| breaker.is_tripped()) {
        match breaker_scope_matches(
            &breaker.scope,
            runtime_identity,
            action,
            adapter,
            runtime_admission_only,
        ) {
            BreakerScopeMatch::Matches => {
                return VerificationResult {
                    allowed: false,
                    reasons: vec!["circuit_breaker_tripped".to_string()],
                    artifacts: serde_json::json!({
                        "circuit_breaker": breaker.as_match().to_artifact(),
                    }),
                };
            }
            BreakerScopeMatch::Unknown(field) => {
                return VerificationResult {
                    allowed: false,
                    reasons: vec!["circuit_breaker_scope_unknown".to_string()],
                    artifacts: serde_json::json!({
                        "circuit_breaker": breaker.as_match().to_artifact(),
                        "missing_identity": field,
                    }),
                };
            }
            BreakerScopeMatch::DoesNotMatch => {}
        }
    }
    VerificationResult::allow()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BreakerScopeMatch {
    Matches,
    DoesNotMatch,
    Unknown(&'static str),
}

fn breaker_scope_matches(
    scope: &CircuitBreakerScope,
    runtime_identity: &RuntimeIdentityContext,
    action: Option<&ActionRequest>,
    adapter: Option<&str>,
    runtime_admission_only: bool,
) -> BreakerScopeMatch {
    match scope {
        CircuitBreakerScope::Global => BreakerScopeMatch::Matches,
        CircuitBreakerScope::Fleet(expected) => match runtime_identity.fleet_id.as_ref() {
            Some(actual) if actual == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("fleet_id"),
        },
        CircuitBreakerScope::Node(expected) => match runtime_identity.node_id.as_ref() {
            Some(actual) if actual == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("node_id"),
        },
        CircuitBreakerScope::Instance(expected) => match runtime_identity.instance_id.as_ref() {
            Some(actual) if actual == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("instance_id"),
        },
        CircuitBreakerScope::Tenant(expected) => match action {
            Some(action) if &action.tenant_id == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None if runtime_admission_only => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("tenant_id"),
        },
        CircuitBreakerScope::Agent(expected) => match action {
            Some(action) if &action.agent_id == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None if runtime_admission_only => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("agent_id"),
        },
        CircuitBreakerScope::Adapter(expected) => match adapter {
            Some(actual) if actual == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None if runtime_admission_only => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("adapter"),
        },
        CircuitBreakerScope::Action(expected) => match action {
            Some(action) if &action.action.name == expected => BreakerScopeMatch::Matches,
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None if runtime_admission_only => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("action"),
        },
        CircuitBreakerScope::ActionClass(expected) => match action {
            Some(action) if &action.action.side_effect_class == expected => {
                BreakerScopeMatch::Matches
            }
            Some(_) => BreakerScopeMatch::DoesNotMatch,
            None if runtime_admission_only => BreakerScopeMatch::DoesNotMatch,
            None => BreakerScopeMatch::Unknown("action_class"),
        },
    }
}

fn denied_outcome(action_id: ActionId, verification: VerificationResult) -> ActionOutcome {
    let error = if verification.reasons.is_empty() {
        "verification denied".to_string()
    } else {
        verification.reasons.join(", ")
    };
    ActionOutcome {
        action_id,
        status: ActionStatus::Denied,
        verification,
        post_verification: None,
        output: None,
        error: Some(error),
        completed_at: OffsetDateTime::now_utc(),
    }
}

fn attach_request_context(result: &mut VerificationResult, action: &ActionRequest) {
    let sources = result
        .artifacts
        .as_object()
        .map(|artifacts| artifacts.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let context = request_context(action, sources);
    match &mut result.artifacts {
        serde_json::Value::Object(artifacts) => {
            artifacts.insert("context".to_string(), context);
        }
        other => {
            let mut artifacts = serde_json::Map::new();
            if !other.is_null() {
                artifacts.insert("detail".to_string(), other.take());
            }
            artifacts.insert("context".to_string(), context);
            result.artifacts = serde_json::Value::Object(artifacts);
        }
    }
}

fn request_context(action: &ActionRequest, sources: Vec<String>) -> serde_json::Value {
    serde_json::json!({
        "source": "gateway_verifier_chain",
        "tenant_id": action.tenant_id.to_string(),
        "agent_id": action.agent_id.to_string(),
        "action": action.action.name,
        "sources": sources,
    })
}

#[cfg(test)]
#[path = "../tests/unit/gateway_tests.rs"]
mod tests;
