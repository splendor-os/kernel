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
//!     approval_evidence: None,
//! };
//! assert!(ActionGateway::submit(&gateway, request).is_err());
//! ```

use serde::{Deserialize, Serialize};
use splendor_types::{
    Action, AgentId, ApprovalActionScope, ApprovalDecision, ApprovalEvidence, ApprovalId,
    ApprovalPolicy, ApprovalTraceContext, IdentityValidationError, QuotaUsage, RunId, TenantId,
    VerificationResult,
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
    /// Optional approval grant/denial evidence presented for this action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_evidence: Option<ApprovalEvidence>,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActionStatus {
    /// Action executed successfully.
    Executed,
    /// Action was denied by verification.
    Denied,
    /// Action is paused until scoped approval evidence is presented.
    NeedsApproval,
    /// Action did not execute because verifier uncertainty or escalation requires
    /// operator/control-plane intervention.
    NeedsIntervention,
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

/// Result returned by an approval verifier.
#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalVerification {
    /// No approval policy applies to this action.
    NotRequired,
    /// Scoped approval evidence is valid; execution may continue.
    Granted(VerificationResult),
    /// Approval is required but no valid evidence was supplied yet.
    Required(VerificationResult),
    /// Approval evidence denied, expired, was revoked, or had the wrong scope.
    Denied(VerificationResult),
    /// The approval verifier could not complete and failed closed.
    NeedsIntervention(VerificationResult),
}

/// Verifies scoped approval evidence before adapter execution.
pub trait ApprovalVerifier: Send + Sync {
    /// Validates whether an action requires approval and whether supplied evidence is valid.
    fn verify_approval(
        &self,
        action: &ActionRequest,
        adapter: Option<&str>,
        now: OffsetDateTime,
    ) -> ApprovalVerification;
}

/// Approval verifier that requires no approval policies.
#[derive(Clone, Debug, Default)]
pub struct NoApprovalVerifier;

impl ApprovalVerifier for NoApprovalVerifier {
    fn verify_approval(
        &self,
        _action: &ActionRequest,
        _adapter: Option<&str>,
        _now: OffsetDateTime,
    ) -> ApprovalVerification {
        ApprovalVerification::NotRequired
    }
}

/// Static approval verifier backed by local approval policies.
#[derive(Clone, Debug, Default)]
pub struct PolicyApprovalVerifier {
    policies: Vec<ApprovalPolicy>,
}

impl PolicyApprovalVerifier {
    /// Creates a static approval verifier from local policies.
    pub fn new(policies: Vec<ApprovalPolicy>) -> Self {
        Self { policies }
    }
}

impl ApprovalVerifier for PolicyApprovalVerifier {
    fn verify_approval(
        &self,
        action: &ActionRequest,
        adapter: Option<&str>,
        now: OffsetDateTime,
    ) -> ApprovalVerification {
        let scope = approval_scope(action, adapter);
        let Some(policy) = self
            .policies
            .iter()
            .find(|policy| policy.matches_action(&scope, now))
        else {
            return ApprovalVerification::NotRequired;
        };

        if policy.is_expired(now) {
            return ApprovalVerification::NeedsIntervention(approval_result(
                false,
                "approval_policy_expired",
                "intervention_required",
                ApprovalTraceContext::requested(policy, &scope, ApprovalId::new()),
                Some(policy.policy_id.clone()),
            ));
        }

        let Some(evidence) = action.approval_evidence.as_ref() else {
            return ApprovalVerification::Required(approval_result(
                false,
                "approval_required",
                "required",
                ApprovalTraceContext::requested(policy, &scope, ApprovalId::new()),
                Some(policy.policy_id.clone()),
            ));
        };

        let context = ApprovalTraceContext::from_evidence(evidence, &scope);
        if (evidence.action_id.is_none() && evidence.action_name.is_none())
            || (adapter.is_some() && evidence.adapter.is_none())
        {
            return ApprovalVerification::Denied(approval_result(
                false,
                "approval_scope_incomplete",
                "denied",
                context,
                Some(policy.policy_id.clone()),
            ));
        }
        if evidence.tenant_id != action.tenant_id
            || evidence.agent_id != action.agent_id
            || evidence.run_id != action.run_id
            || evidence
                .action_id
                .as_ref()
                .map(|action_id| action_id != &action.action_id)
                .unwrap_or(false)
            || evidence
                .action_name
                .as_ref()
                .map(|name| name != &action.action.name)
                .unwrap_or(false)
            || evidence
                .adapter
                .as_ref()
                .map(|expected| Some(expected.as_str()) != adapter)
                .unwrap_or(false)
        {
            return ApprovalVerification::Denied(approval_result(
                false,
                "approval_scope_mismatch",
                "denied",
                context,
                Some(policy.policy_id.clone()),
            ));
        }

        if evidence.revoked {
            return ApprovalVerification::Denied(approval_result(
                false,
                "approval_revoked",
                "revoked",
                context,
                Some(policy.policy_id.clone()),
            ));
        }

        if evidence.expires_at < now {
            return ApprovalVerification::Denied(approval_result(
                false,
                "approval_expired",
                "expired",
                context,
                Some(policy.policy_id.clone()),
            ));
        }

        match evidence.decision {
            ApprovalDecision::Granted => ApprovalVerification::Granted(approval_result(
                true,
                "approval_granted",
                "granted",
                context,
                Some(policy.policy_id.clone()),
            )),
            ApprovalDecision::Denied => ApprovalVerification::Denied(approval_result(
                false,
                "approval_denied",
                "denied",
                context,
                Some(policy.policy_id.clone()),
            )),
        }
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
    approval_verifier: Arc<dyn ApprovalVerifier>,
}

impl VerifiedActionGateway {
    /// Creates a gateway with the provided tenant access.
    pub fn new(tenant_access: Arc<dyn TenantAccess>) -> Self {
        Self {
            adapters: HashMap::new(),
            tenant_access,
            invariant_evaluator: Arc::new(SimpleInvariantEvaluator),
            approval_verifier: Arc::new(NoApprovalVerifier),
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

    /// Overrides the approval verifier used by the gateway.
    pub fn set_approval_verifier(&mut self, verifier: Arc<dyn ApprovalVerifier>) {
        self.approval_verifier = verifier;
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

        let approval_verification = self.approval_verifier.verify_approval(
            &action,
            Some(adapter_id),
            OffsetDateTime::now_utc(),
        );
        let approval_grant = match approval_verification {
            ApprovalVerification::NotRequired => None,
            ApprovalVerification::Granted(result) => Some(result),
            ApprovalVerification::Required(mut result) => {
                attach_request_context(&mut result, &action);
                return Ok(needs_approval_outcome(action.action_id, result));
            }
            ApprovalVerification::Denied(mut result) => {
                attach_request_context(&mut result, &action);
                return Ok(denied_outcome(action.action_id, result));
            }
            ApprovalVerification::NeedsIntervention(mut result) => {
                attach_request_context(&mut result, &action);
                return Ok(needs_intervention_outcome(action.action_id, result));
            }
        };

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
        if let Some(approval_grant) = approval_grant {
            attach_allowed_artifact(&mut verification, "approval", approval_grant.artifacts);
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

fn needs_approval_outcome(action_id: ActionId, verification: VerificationResult) -> ActionOutcome {
    ActionOutcome {
        action_id,
        status: ActionStatus::NeedsApproval,
        verification,
        post_verification: None,
        output: None,
        error: Some("approval_required".to_string()),
        completed_at: OffsetDateTime::now_utc(),
    }
}

fn needs_intervention_outcome(
    action_id: ActionId,
    verification: VerificationResult,
) -> ActionOutcome {
    ActionOutcome {
        action_id,
        status: ActionStatus::NeedsIntervention,
        verification,
        post_verification: None,
        output: None,
        error: Some("needs_intervention".to_string()),
        completed_at: OffsetDateTime::now_utc(),
    }
}

fn approval_scope<'a>(
    action: &'a ActionRequest,
    adapter: Option<&'a str>,
) -> ApprovalActionScope<'a> {
    ApprovalActionScope {
        tenant_id: &action.tenant_id,
        agent_id: &action.agent_id,
        run_id: &action.run_id,
        action_id: &action.action_id,
        action: &action.action,
        adapter,
    }
}

fn approval_result(
    allowed: bool,
    reason: &str,
    status: &str,
    approval: ApprovalTraceContext,
    policy_id: Option<String>,
) -> VerificationResult {
    VerificationResult {
        allowed,
        reasons: if allowed {
            Vec::new()
        } else {
            vec![reason.to_string()]
        },
        artifacts: serde_json::json!({
            "verifier": "approval_verifier",
            "approval_status": status,
            "approval": approval,
            "policy_id": policy_id,
        }),
    }
}

fn attach_allowed_artifact(
    result: &mut VerificationResult,
    label: &str,
    artifact: serde_json::Value,
) {
    if artifact.is_null() {
        return;
    }
    let mut artifacts = match std::mem::take(&mut result.artifacts) {
        serde_json::Value::Object(map) => map,
        serde_json::Value::Null => serde_json::Map::new(),
        other => {
            let mut map = serde_json::Map::new();
            map.insert("detail".to_string(), other);
            map
        }
    };
    artifacts.insert(label.to_string(), artifact);
    result.artifacts = serde_json::Value::Object(artifacts);
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
        "run_id": action.run_id.to_string(),
        "action_id": action.action_id.to_string(),
        "action": action.action.name,
        "adapter": action.adapter,
        "sources": sources,
    })
}

#[cfg(test)]
#[path = "../tests/unit/gateway_tests.rs"]
mod tests;
