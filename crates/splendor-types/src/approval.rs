//! # Approval Governance Primitives
//!
//! Minimal approval objects used by the 0.04-S2 approval verifier. They are
//! intentionally scoped and serializable so approval decisions can be carried in
//! gateway requests, emitted as trace events, and reconstructed during replay
//! without introducing a workflow engine or approval queue.

use crate::{
    Action, ActionId, AgentId, ApprovalId, RunId, SideEffectClass, TenantId, TraceEventId,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Version tag for approval policy objects introduced in 0.04-S2.
pub const APPROVAL_POLICY_SCHEMA_VERSION: &str = "splendor.approval_policy.v1";
/// Version tag for approval evidence objects introduced in 0.04-S2.
pub const APPROVAL_EVIDENCE_SCHEMA_VERSION: &str = "splendor.approval_evidence.v1";

/// Declares when an action must pause for approval before adapter execution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    /// Schema version for compatibility checks.
    pub schema_version: String,
    /// Stable policy identifier assigned by the local config/control boundary.
    pub policy_id: String,
    /// Tenant scope where this policy applies.
    pub tenant_id: TenantId,
    /// Optional agent scope. `None` applies to all agents in the tenant.
    pub agent_id: Option<AgentId>,
    /// Optional action name scope. `None` applies to all actions in the tenant.
    pub action_name: Option<String>,
    /// Optional adapter scope. `None` applies to any adapter.
    pub adapter: Option<String>,
    /// Optional required permission that triggers approval when present.
    pub required_permission: Option<String>,
    /// Optional side-effect class that triggers approval when it matches.
    pub side_effect_class: Option<SideEffectClass>,
    /// Human-readable risk label such as `high`, `regulated`, or `external`.
    pub risk_level: Option<String>,
    /// Reason included in trace artifacts when approval is required.
    pub reason: String,
    /// Optional policy expiry. Expired approval policies fail closed.
    pub expires_at: Option<OffsetDateTime>,
}

impl ApprovalPolicy {
    /// Builds a minimally scoped approval policy for one tenant.
    pub fn new(
        policy_id: impl Into<String>,
        tenant_id: TenantId,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: APPROVAL_POLICY_SCHEMA_VERSION.to_string(),
            policy_id: policy_id.into(),
            tenant_id,
            agent_id: None,
            action_name: None,
            adapter: None,
            required_permission: None,
            side_effect_class: None,
            risk_level: None,
            reason: reason.into(),
            expires_at: None,
        }
    }

    /// Returns true when the policy applies to the request scope.
    pub fn matches_action(&self, request: &ApprovalActionScope<'_>, _now: OffsetDateTime) -> bool {
        if self.tenant_id != *request.tenant_id {
            return false;
        }
        if let Some(agent_id) = &self.agent_id {
            if agent_id != request.agent_id {
                return false;
            }
        }
        if let Some(action_name) = &self.action_name {
            if action_name != &request.action.name {
                return false;
            }
        }
        if let Some(adapter) = &self.adapter {
            if Some(adapter.as_str()) != request.adapter {
                return false;
            }
        }
        if let Some(permission) = &self.required_permission {
            if !request
                .action
                .required_permissions
                .iter()
                .any(|required| required == permission)
            {
                return false;
            }
        }
        if let Some(side_effect_class) = &self.side_effect_class {
            if side_effect_class != &request.action.side_effect_class {
                return false;
            }
        }
        true
    }

    /// Returns true when the policy itself is expired and must fail closed.
    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        self.expires_at
            .map(|expires_at| expires_at < now)
            .unwrap_or(false)
    }
}

/// Borrowed action scope used to evaluate approval policy and evidence.
#[derive(Clone, Copy, Debug)]
pub struct ApprovalActionScope<'a> {
    /// Tenant submitting the action.
    pub tenant_id: &'a TenantId,
    /// Agent submitting the action.
    pub agent_id: &'a AgentId,
    /// Run that scopes the action.
    pub run_id: &'a RunId,
    /// Runtime action identity for this evaluation.
    pub action_id: &'a ActionId,
    /// Action payload.
    pub action: &'a Action,
    /// Adapter selected for execution.
    pub adapter: Option<&'a str>,
}

/// Decision carried by approval evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ApprovalDecision {
    /// Approval was granted for the scoped action.
    Granted,
    /// Approval was denied for the scoped action.
    Denied,
}

/// Approval grant/denial token or percept-derived evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApprovalEvidence {
    /// Schema version for compatibility checks.
    pub schema_version: String,
    /// Approval identity distinct from run, action, trace, and message IDs.
    pub approval_id: ApprovalId,
    /// Tenant authorized by the evidence.
    pub tenant_id: TenantId,
    /// Agent authorized by the evidence.
    pub agent_id: AgentId,
    /// Run authorized by the evidence.
    pub run_id: RunId,
    /// Optional action identity. When present it must match this evaluation.
    pub action_id: Option<ActionId>,
    /// Optional action name. When present it must match this evaluation.
    pub action_name: Option<String>,
    /// Optional adapter. When present it must match this evaluation.
    pub adapter: Option<String>,
    /// Grant or denial decision.
    pub decision: ApprovalDecision,
    /// Reason supplied by the approver/control boundary.
    pub reason: Option<String>,
    /// Issuance time for audit/replay.
    pub issued_at: OffsetDateTime,
    /// Expiry for this approval evidence.
    pub expires_at: OffsetDateTime,
    /// Revocation flag supplied by the approval boundary.
    pub revoked: bool,
    /// Optional trace event that requested this approval.
    pub trace_event_id: Option<TraceEventId>,
}

impl ApprovalEvidence {
    /// Builds approval evidence scoped to a tenant, agent, and run.
    pub fn new(
        approval_id: ApprovalId,
        tenant_id: TenantId,
        agent_id: AgentId,
        run_id: RunId,
        decision: ApprovalDecision,
        expires_at: OffsetDateTime,
    ) -> Self {
        Self {
            schema_version: APPROVAL_EVIDENCE_SCHEMA_VERSION.to_string(),
            approval_id,
            tenant_id,
            agent_id,
            run_id,
            action_id: None,
            action_name: None,
            adapter: None,
            decision,
            reason: None,
            issued_at: OffsetDateTime::now_utc(),
            expires_at,
            revoked: false,
            trace_event_id: None,
        }
    }

    /// Returns a copy scoped to one action name.
    pub fn with_action_name(mut self, action_name: impl Into<String>) -> Self {
        self.action_name = Some(action_name.into());
        self
    }

    /// Returns a copy scoped to one adapter.
    pub fn with_adapter(mut self, adapter: impl Into<String>) -> Self {
        self.adapter = Some(adapter.into());
        self
    }

    /// Returns a copy linked to a requesting trace event.
    pub fn with_trace_event(mut self, trace_event_id: TraceEventId) -> Self {
        self.trace_event_id = Some(trace_event_id);
        self
    }
}

/// Trace-safe approval context embedded in approval lifecycle events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApprovalTraceContext {
    /// Approval identity.
    pub approval_id: ApprovalId,
    /// Tenant scope.
    pub tenant_id: TenantId,
    /// Agent scope.
    pub agent_id: AgentId,
    /// Run scope.
    pub run_id: RunId,
    /// Runtime action identity when known.
    pub action_id: Option<ActionId>,
    /// Action name scope.
    pub action_name: String,
    /// Adapter scope when known.
    pub adapter: Option<String>,
    /// Decision carried by approval evidence, when an approver decided.
    pub decision: Option<ApprovalDecision>,
    /// Human-readable reason or policy explanation.
    pub reason: Option<String>,
    /// Optional policy identifier that required approval.
    pub policy_id: Option<String>,
    /// Optional risk level that required approval.
    pub risk_level: Option<String>,
    /// Evidence issuance time when known.
    pub issued_at: Option<OffsetDateTime>,
    /// Expiry for policy or evidence when known.
    pub expires_at: Option<OffsetDateTime>,
    /// Whether the evidence was revoked.
    pub revoked: bool,
}

impl ApprovalTraceContext {
    /// Builds a request context for a policy-required approval.
    pub fn requested(
        policy: &ApprovalPolicy,
        request: &ApprovalActionScope<'_>,
        approval_id: ApprovalId,
    ) -> Self {
        Self {
            approval_id,
            tenant_id: request.tenant_id.clone(),
            agent_id: request.agent_id.clone(),
            run_id: request.run_id.clone(),
            action_id: Some(request.action_id.clone()),
            action_name: request.action.name.clone(),
            adapter: request.adapter.map(str::to_string),
            decision: None,
            reason: Some(policy.reason.clone()),
            policy_id: Some(policy.policy_id.clone()),
            risk_level: policy.risk_level.clone(),
            issued_at: None,
            expires_at: policy.expires_at,
            revoked: false,
        }
    }

    /// Builds a context from approval evidence and the current action scope.
    pub fn from_evidence(evidence: &ApprovalEvidence, request: &ApprovalActionScope<'_>) -> Self {
        Self {
            approval_id: evidence.approval_id.clone(),
            tenant_id: evidence.tenant_id.clone(),
            agent_id: evidence.agent_id.clone(),
            run_id: evidence.run_id.clone(),
            action_id: evidence
                .action_id
                .clone()
                .or_else(|| Some(request.action_id.clone())),
            action_name: evidence
                .action_name
                .clone()
                .unwrap_or_else(|| request.action.name.clone()),
            adapter: evidence
                .adapter
                .clone()
                .or_else(|| request.adapter.map(str::to_string)),
            decision: Some(evidence.decision.clone()),
            reason: evidence.reason.clone(),
            policy_id: None,
            risk_level: None,
            issued_at: Some(evidence.issued_at),
            expires_at: Some(evidence.expires_at),
            revoked: evidence.revoked,
        }
    }
}
