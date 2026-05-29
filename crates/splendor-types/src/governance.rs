//! Governance state schemas for approval, escalation, intervention,
//! circuit-breaker, and kill-switch lifecycle objects.
//!
//! Sprint 0.04-S1 keeps this module deliberately transport- and UI-neutral. It
//! defines first-class governance objects, explicit scopes, lifecycle states,
//! validation, and trace-ready transition records. Enforcement by verifiers,
//! approval queues, circuit-breaker blocking, and kill-switch propagation are
//! later isolated sprints.

use crate::{
    ActionId, AgentId, ApprovalId, CircuitBreakerId, EscalationId, FleetId, InstanceId,
    InterventionId, KillSwitchId, NodeId, RunId, SideEffectClass, TenantId, TraceEventId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// Canonical governance state schema version introduced in 0.04-S1.
pub const GOVERNANCE_STATE_SCHEMA_VERSION: &str = "splendor.governance_state.v1";

/// Forward-compatible, non-authoritative governance extension fields.
pub type GovernanceExtensions = BTreeMap<String, serde_json::Value>;

/// Schema version for circuit-breaker control objects introduced in 0.04-S4.
pub const CIRCUIT_BREAKER_SCHEMA_VERSION: &str = "splendor.circuit_breaker.v1";

impl CircuitBreakerId {
    /// Creates a deterministic circuit-breaker identifier from a non-empty label.
    ///
    /// Circuit-breaker config files may use operator-friendly labels such as
    /// `cb_adapter_http`. The runtime stores them as typed UUID-backed IDs so
    /// they remain distinct from tenant, agent, run, action, trace, and message
    /// identities.
    pub fn try_new(value: impl Into<String>) -> Result<Self, CircuitBreakerValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CircuitBreakerValidationError::EmptyBreakerId);
        }
        Ok(Self::from(Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            value.as_bytes(),
        )))
    }
}

/// Scope at which a circuit breaker applies during enforcement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "value", rename_all = "snake_case")]
pub enum CircuitBreakerScope {
    /// Applies to all runtime work handled by this evaluator.
    Global,
    /// Applies to a fleet boundary.
    Fleet(FleetId),
    /// Applies to a node boundary.
    Node(NodeId),
    /// Applies to a Splendor runtime instance boundary.
    Instance(InstanceId),
    /// Applies to all work for one tenant.
    Tenant(TenantId),
    /// Applies to all work for one agent.
    Agent(AgentId),
    /// Applies to a registered adapter identifier.
    Adapter(String),
    /// Applies to an action name.
    Action(String),
    /// Applies to a side-effect class such as filesystem, network, or external.
    ActionClass(SideEffectClass),
}

impl CircuitBreakerScope {
    /// Returns the stable scope label used in denial artifacts and replay output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Fleet(_) => "fleet",
            Self::Node(_) => "node",
            Self::Instance(_) => "instance",
            Self::Tenant(_) => "tenant",
            Self::Agent(_) => "agent",
            Self::Adapter(_) => "adapter",
            Self::Action(_) => "action",
            Self::ActionClass(_) => "action_class",
        }
    }

    /// Returns the concrete scoped value when the scope is not global.
    pub fn value(&self) -> Option<String> {
        match self {
            Self::Global => None,
            Self::Fleet(value) => Some(value.to_string()),
            Self::Node(value) => Some(value.to_string()),
            Self::Instance(value) => Some(value.to_string()),
            Self::Tenant(value) => Some(value.to_string()),
            Self::Agent(value) => Some(value.to_string()),
            Self::Adapter(value) | Self::Action(value) => Some(value.clone()),
            Self::ActionClass(value) => Some(side_effect_class_label(value)),
        }
    }
}

/// Current lifecycle state of an enforcing circuit breaker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitBreakerState {
    /// The breaker is currently blocking matching work.
    Tripped,
    /// The breaker has been explicitly cleared and no longer blocks matching work.
    Cleared,
}

/// Enforcing circuit-breaker control object consumed by the Action Gateway.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreaker {
    /// Schema version for compatibility checks.
    pub schema_version: String,
    /// Stable breaker identifier.
    pub breaker_id: CircuitBreakerId,
    /// Scope where the breaker applies.
    pub scope: CircuitBreakerScope,
    /// Current lifecycle state.
    pub state: CircuitBreakerState,
    /// Sanitized reason code or short explanation.
    pub reason: String,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
    /// Last state-change timestamp.
    pub updated_at: OffsetDateTime,
}

impl CircuitBreaker {
    /// Creates a tripped circuit breaker.
    pub fn tripped(
        breaker_id: CircuitBreakerId,
        scope: CircuitBreakerScope,
        reason: impl Into<String>,
        at: OffsetDateTime,
    ) -> Result<Self, CircuitBreakerValidationError> {
        let reason = normalize_reason(reason)?;
        Ok(Self {
            schema_version: CIRCUIT_BREAKER_SCHEMA_VERSION.to_string(),
            breaker_id,
            scope,
            state: CircuitBreakerState::Tripped,
            reason,
            created_at: at,
            updated_at: at,
        })
    }

    /// Returns true when this breaker should deny matching work.
    pub fn is_tripped(&self) -> bool {
        self.state == CircuitBreakerState::Tripped
    }

    /// Explicitly clears the breaker and returns the trace context that must be emitted.
    pub fn clear_with_authority(
        mut self,
        reason: impl Into<String>,
        authorized_by: impl Into<String>,
        at: OffsetDateTime,
    ) -> Result<(Self, CircuitBreakerTraceContext), CircuitBreakerValidationError> {
        let reason = normalize_reason(reason)?;
        let context = CircuitBreakerTraceContext::try_new(
            self.breaker_id.clone(),
            self.scope.clone(),
            CircuitBreakerState::Cleared,
            reason.clone(),
            authorized_by,
            at,
        )?;
        self.state = CircuitBreakerState::Cleared;
        self.reason = reason;
        self.updated_at = at;
        Ok((self, context))
    }

    /// Builds a trace context for the tripped state.
    pub fn trip_trace_context(
        &self,
        authorized_by: impl Into<String>,
        at: OffsetDateTime,
    ) -> Result<CircuitBreakerTraceContext, CircuitBreakerValidationError> {
        CircuitBreakerTraceContext::try_new(
            self.breaker_id.clone(),
            self.scope.clone(),
            CircuitBreakerState::Tripped,
            self.reason.clone(),
            authorized_by,
            at,
        )
    }

    /// Converts a tripped breaker into the match record persisted in denial artifacts.
    pub fn as_match(&self) -> CircuitBreakerMatch {
        CircuitBreakerMatch {
            breaker_id: self.breaker_id.clone(),
            scope: self.scope.clone(),
            state: self.state.clone(),
            reason: self.reason.clone(),
        }
    }
}

/// Trace payload for breaker trip and clear events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreakerTraceContext {
    /// Breaker identity.
    pub breaker_id: CircuitBreakerId,
    /// Breaker scope.
    pub scope: CircuitBreakerScope,
    /// State recorded by this trace event.
    pub state: CircuitBreakerState,
    /// Sanitized reason code or short explanation.
    pub reason: String,
    /// Principal, operator, service, or local config authority that changed state.
    pub authorized_by: String,
    /// Timestamp of the explicit state-change event.
    pub recorded_at: OffsetDateTime,
}

impl CircuitBreakerTraceContext {
    /// Creates a validated trace context for an explicit breaker state change.
    pub fn try_new(
        breaker_id: CircuitBreakerId,
        scope: CircuitBreakerScope,
        state: CircuitBreakerState,
        reason: impl Into<String>,
        authorized_by: impl Into<String>,
        recorded_at: OffsetDateTime,
    ) -> Result<Self, CircuitBreakerValidationError> {
        let reason = normalize_reason(reason)?;
        let authorized_by = authorized_by.into();
        if authorized_by.trim().is_empty() {
            return Err(CircuitBreakerValidationError::MissingAuthority);
        }
        Ok(Self {
            breaker_id,
            scope,
            state,
            reason,
            authorized_by,
            recorded_at,
        })
    }
}

/// Breaker evidence attached to fail-closed verification artifacts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreakerMatch {
    /// Breaker identity that caused the denial.
    pub breaker_id: CircuitBreakerId,
    /// Scope that matched the runtime/action context.
    pub scope: CircuitBreakerScope,
    /// Breaker state at evaluation time.
    pub state: CircuitBreakerState,
    /// Sanitized reason code or short explanation.
    pub reason: String,
}

impl CircuitBreakerMatch {
    /// Returns a stable JSON artifact for replay and audit output.
    pub fn to_artifact(&self) -> serde_json::Value {
        serde_json::json!({
            "breaker_id": self.breaker_id.to_string(),
            "scope": self.scope.label(),
            "scope_value": self.scope.value(),
            "state": match self.state {
                CircuitBreakerState::Tripped => "tripped",
                CircuitBreakerState::Cleared => "cleared",
            },
            "reason": self.reason,
        })
    }
}

/// Validation failures for circuit-breaker control objects.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum CircuitBreakerValidationError {
    /// Breaker IDs must be non-empty.
    #[error("circuit breaker id is required")]
    EmptyBreakerId,
    /// State-change reasons must be non-empty.
    #[error("circuit breaker reason is required")]
    EmptyReason,
    /// Clear/reset events require explicit authority attribution.
    #[error("circuit breaker state changes require authorized_by")]
    MissingAuthority,
}

/// Governance scope supported by 0.04-S1 state objects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "scope_type")]
pub enum GovernanceScope {
    /// Applies globally to the local governance domain.
    Global,
    /// Applies to a governed fleet.
    Fleet { fleet_id: FleetId },
    /// Applies to a physical, virtual, or logical node.
    Node { node_id: NodeId },
    /// Applies to one Splendor runtime instance.
    Instance { instance_id: InstanceId },
    /// Applies to a tenant authority boundary.
    Tenant { tenant_id: TenantId },
    /// Applies to an agent within a tenant.
    Agent {
        tenant_id: TenantId,
        agent_id: AgentId,
    },
    /// Applies to a run owned by an agent within a tenant.
    Run {
        tenant_id: TenantId,
        agent_id: AgentId,
        run_id: RunId,
    },
    /// Applies to a specific action in a run.
    Action {
        tenant_id: TenantId,
        agent_id: AgentId,
        run_id: RunId,
        action_id: ActionId,
    },
    /// Applies to an adapter, optionally narrowed to a tenant.
    Adapter {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tenant_id: Option<TenantId>,
        adapter: String,
    },
}

impl GovernanceScope {
    /// Validates that scope identities are present, non-nil, and non-blank.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        match self {
            Self::Global => Ok(()),
            Self::Fleet { fleet_id } => validate_uuid("fleet_id", fleet_id),
            Self::Node { node_id } => validate_uuid("node_id", node_id),
            Self::Instance { instance_id } => validate_uuid("instance_id", instance_id),
            Self::Tenant { tenant_id } => validate_uuid("tenant_id", tenant_id),
            Self::Agent {
                tenant_id,
                agent_id,
            } => {
                validate_uuid("tenant_id", tenant_id)?;
                validate_uuid("agent_id", agent_id)
            }
            Self::Run {
                tenant_id,
                agent_id,
                run_id,
            } => {
                validate_uuid("tenant_id", tenant_id)?;
                validate_uuid("agent_id", agent_id)?;
                validate_uuid("run_id", run_id)
            }
            Self::Action {
                tenant_id,
                agent_id,
                run_id,
                action_id,
            } => {
                validate_uuid("tenant_id", tenant_id)?;
                validate_uuid("agent_id", agent_id)?;
                validate_uuid("run_id", run_id)?;
                validate_uuid("action_id", action_id)
            }
            Self::Adapter { tenant_id, adapter } => {
                validate_optional_uuid("tenant_id", tenant_id.as_ref())?;
                if adapter.trim().is_empty() || adapter.trim() != adapter {
                    return Err(GovernanceValidationError::InvalidScope {
                        reason: "invalid_adapter_scope".to_string(),
                    });
                }
                Ok(())
            }
        }
    }

    /// Returns the tenant bound to this scope, if any.
    pub fn tenant_id(&self) -> Option<&TenantId> {
        match self {
            Self::Tenant { tenant_id }
            | Self::Agent { tenant_id, .. }
            | Self::Run { tenant_id, .. }
            | Self::Action { tenant_id, .. } => Some(tenant_id),
            Self::Adapter { tenant_id, .. } => tenant_id.as_ref(),
            _ => None,
        }
    }

    /// Returns the agent bound to this scope, if any.
    pub fn agent_id(&self) -> Option<&AgentId> {
        match self {
            Self::Agent { agent_id, .. }
            | Self::Run { agent_id, .. }
            | Self::Action { agent_id, .. } => Some(agent_id),
            _ => None,
        }
    }

    /// Returns the action bound to this scope, if any.
    pub fn action_id(&self) -> Option<&ActionId> {
        match self {
            Self::Action { action_id, .. } => Some(action_id),
            _ => None,
        }
    }

    /// Returns the run bound to this scope, if any.
    pub fn run_id(&self) -> Option<&RunId> {
        match self {
            Self::Run { run_id, .. } | Self::Action { run_id, .. } => Some(run_id),
            _ => None,
        }
    }
}

/// Source and issuer attribution for a governance object or transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GovernanceIssuer {
    /// Stable issuer principal or runtime component identifier.
    pub issuer_id: String,
    /// Source path such as `runtime`, `daemon`, `operator`, or `central_manager`.
    pub source: String,
}

impl GovernanceIssuer {
    /// Creates issuer/source attribution after rejecting blank values.
    pub fn new(
        issuer_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, GovernanceValidationError> {
        let issuer = Self {
            issuer_id: issuer_id.into(),
            source: source.into(),
        };
        issuer.validate()?;
        Ok(issuer)
    }

    /// Validates issuer/source attribution.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_non_blank("issuer_id", &self.issuer_id)?;
        validate_non_blank("source", &self.source)
    }
}

/// Trace linkage carried by every governance object and transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GovernanceTraceLink {
    /// Trace event that caused or recorded this governance object/transition.
    pub trace_event_id: TraceEventId,
    /// Run stream associated with the causal trace event, when run-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
}

impl GovernanceTraceLink {
    /// Creates a governance trace link.
    pub fn new(trace_event_id: TraceEventId, run_id: Option<RunId>) -> Self {
        Self {
            trace_event_id,
            run_id,
        }
    }

    /// Validates trace linkage before a governance object is accepted.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("trace_event_id", &self.trace_event_id)?;
        validate_optional_uuid("run_id", self.run_id.as_ref())
    }
}

/// Explicit revocation marker for governance objects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GovernanceRevocation {
    /// Revocation timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub revoked_at: OffsetDateTime,
    /// Human-readable, sanitized revocation reason.
    pub reason: String,
    /// Issuer/source that revoked the object.
    pub issuer: GovernanceIssuer,
    /// Trace linkage for the revocation decision.
    pub trace: GovernanceTraceLink,
}

impl GovernanceRevocation {
    /// Builds and validates a revocation marker.
    pub fn new(
        revoked_at: OffsetDateTime,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
    ) -> Result<Self, GovernanceValidationError> {
        let revocation = Self {
            revoked_at,
            reason: reason.into(),
            issuer,
            trace,
        };
        revocation.validate()?;
        Ok(revocation)
    }

    /// Validates revocation reason, issuer, and trace linkage.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_non_blank("revocation.reason", &self.reason)?;
        self.issuer.validate()?;
        self.trace.validate()
    }
}

/// Status values for approval lifecycle objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Requested,
    Granted,
    Denied,
    Expired,
    Revoked,
}

/// Status values for escalation lifecycle objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationStatus {
    Open,
    Resolved,
    Expired,
    Revoked,
}

/// Status values for intervention lifecycle objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterventionStatus {
    Requested,
    Resolved,
    Cancelled,
    Expired,
    Revoked,
}

/// Status values for circuit-breaker lifecycle objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitBreakerStatus {
    Active,
    Cleared,
    Expired,
    Revoked,
}

/// Status values for kill-switch lifecycle objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillSwitchStatus {
    Active,
    Cleared,
    Expired,
    Revoked,
}

/// Generic state names used by transition records and trace payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceState {
    Requested,
    Granted,
    Denied,
    Open,
    Resolved,
    Active,
    Cleared,
    Cancelled,
    Expired,
    Revoked,
}

impl From<ApprovalStatus> for GovernanceState {
    fn from(value: ApprovalStatus) -> Self {
        match value {
            ApprovalStatus::Requested => Self::Requested,
            ApprovalStatus::Granted => Self::Granted,
            ApprovalStatus::Denied => Self::Denied,
            ApprovalStatus::Expired => Self::Expired,
            ApprovalStatus::Revoked => Self::Revoked,
        }
    }
}

impl From<EscalationStatus> for GovernanceState {
    fn from(value: EscalationStatus) -> Self {
        match value {
            EscalationStatus::Open => Self::Open,
            EscalationStatus::Resolved => Self::Resolved,
            EscalationStatus::Expired => Self::Expired,
            EscalationStatus::Revoked => Self::Revoked,
        }
    }
}

impl From<InterventionStatus> for GovernanceState {
    fn from(value: InterventionStatus) -> Self {
        match value {
            InterventionStatus::Requested => Self::Requested,
            InterventionStatus::Resolved => Self::Resolved,
            InterventionStatus::Cancelled => Self::Cancelled,
            InterventionStatus::Expired => Self::Expired,
            InterventionStatus::Revoked => Self::Revoked,
        }
    }
}

impl From<CircuitBreakerStatus> for GovernanceState {
    fn from(value: CircuitBreakerStatus) -> Self {
        match value {
            CircuitBreakerStatus::Active => Self::Active,
            CircuitBreakerStatus::Cleared => Self::Cleared,
            CircuitBreakerStatus::Expired => Self::Expired,
            CircuitBreakerStatus::Revoked => Self::Revoked,
        }
    }
}

impl From<KillSwitchStatus> for GovernanceState {
    fn from(value: KillSwitchStatus) -> Self {
        match value {
            KillSwitchStatus::Active => Self::Active,
            KillSwitchStatus::Cleared => Self::Cleared,
            KillSwitchStatus::Expired => Self::Expired,
            KillSwitchStatus::Revoked => Self::Revoked,
        }
    }
}

/// Stable reference to any governance lifecycle object.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "object_type")]
pub enum GovernanceObjectRef {
    Approval {
        approval_id: ApprovalId,
    },
    Escalation {
        escalation_id: EscalationId,
    },
    Intervention {
        intervention_id: InterventionId,
    },
    CircuitBreaker {
        circuit_breaker_id: CircuitBreakerId,
    },
    KillSwitch {
        kill_switch_id: KillSwitchId,
    },
}

impl GovernanceObjectRef {
    /// Returns the governance object kind used by transition validation.
    pub fn kind(&self) -> GovernanceObjectKind {
        match self {
            Self::Approval { .. } => GovernanceObjectKind::Approval,
            Self::Escalation { .. } => GovernanceObjectKind::Escalation,
            Self::Intervention { .. } => GovernanceObjectKind::Intervention,
            Self::CircuitBreaker { .. } => GovernanceObjectKind::CircuitBreaker,
            Self::KillSwitch { .. } => GovernanceObjectKind::KillSwitch,
        }
    }

    /// Returns the string form of the object identity for rejection summaries.
    pub fn object_id(&self) -> String {
        match self {
            Self::Approval { approval_id } => approval_id.to_string(),
            Self::Escalation { escalation_id } => escalation_id.to_string(),
            Self::Intervention { intervention_id } => intervention_id.to_string(),
            Self::CircuitBreaker { circuit_breaker_id } => circuit_breaker_id.to_string(),
            Self::KillSwitch { kill_switch_id } => kill_switch_id.to_string(),
        }
    }

    /// Validates the referenced identity.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        match self {
            Self::Approval { approval_id } => validate_uuid("approval_id", approval_id),
            Self::Escalation { escalation_id } => validate_uuid("escalation_id", escalation_id),
            Self::Intervention { intervention_id } => {
                validate_uuid("intervention_id", intervention_id)
            }
            Self::CircuitBreaker { circuit_breaker_id } => {
                validate_uuid("circuit_breaker_id", circuit_breaker_id)
            }
            Self::KillSwitch { kill_switch_id } => validate_uuid("kill_switch_id", kill_switch_id),
        }
    }
}

/// Kinds of governance objects with isolated transition tables.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceObjectKind {
    Approval,
    Escalation,
    Intervention,
    CircuitBreaker,
    KillSwitch,
}

/// Trace-ready governance state transition record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GovernanceTransition {
    /// Governance state schema version.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Governance object identity being transitioned.
    pub object: GovernanceObjectRef,
    /// Authority scope affected by the transition.
    pub scope: GovernanceScope,
    /// Previous lifecycle state; `None` means object creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<GovernanceState>,
    /// Target lifecycle state.
    pub to: GovernanceState,
    /// Transition timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    /// Human-readable, sanitized transition reason.
    pub reason: String,
    /// Issuer/source responsible for the transition.
    pub issuer: GovernanceIssuer,
    /// Trace linkage for the transition.
    pub trace: GovernanceTraceLink,
    /// Non-authoritative extension fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl GovernanceTransition {
    /// Builds and validates a governance transition using the 0.04-S1 transition table.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        object: GovernanceObjectRef,
        scope: GovernanceScope,
        from: Option<GovernanceState>,
        to: GovernanceState,
        occurred_at: OffsetDateTime,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
        extensions: GovernanceExtensions,
    ) -> Result<Self, GovernanceTransitionError> {
        let transition = Self {
            schema_version: default_schema_version(),
            object,
            scope,
            from,
            to,
            occurred_at,
            reason: reason.into(),
            issuer,
            trace,
            extensions,
        };
        transition
            .validate_common()
            .map_err(GovernanceTransitionError::Validation)?;
        if !is_allowed_transition(transition.object.kind(), transition.from, transition.to) {
            return Err(GovernanceTransitionError::Rejected(Box::new(
                GovernanceTransitionRejection::from_transition(
                    &transition,
                    "invalid_governance_transition",
                ),
            )));
        }
        Ok(transition)
    }

    /// Validates shape and non-authoritative extension rules.
    pub fn validate(&self) -> Result<(), GovernanceTransitionError> {
        self.validate_common()
            .map_err(GovernanceTransitionError::Validation)?;
        if !is_allowed_transition(self.object.kind(), self.from, self.to) {
            return Err(GovernanceTransitionError::Rejected(Box::new(
                GovernanceTransitionRejection::from_transition(
                    self,
                    "invalid_governance_transition",
                ),
            )));
        }
        Ok(())
    }

    fn validate_common(&self) -> Result<(), GovernanceValidationError> {
        validate_schema(&self.schema_version)?;
        self.object.validate()?;
        self.scope.validate()?;
        validate_non_blank("reason", &self.reason)?;
        self.issuer.validate()?;
        self.trace.validate()?;
        validate_scope_trace_run_match(&self.scope, &self.trace)?;
        validate_extensions(&self.extensions)
    }
}

/// Rejection payload for invalid governance transitions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GovernanceTransitionRejection {
    /// Governance state schema version.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Governance object identity that rejected the transition.
    pub object: GovernanceObjectRef,
    /// Authority scope affected by the attempted transition.
    pub scope: GovernanceScope,
    /// Current lifecycle state, if the object already exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<GovernanceState>,
    /// Attempted target state.
    pub attempted: GovernanceState,
    /// Stable rejection reason code.
    pub reason: String,
    /// Rejection timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub rejected_at: OffsetDateTime,
    /// Issuer/source responsible for the attempted transition.
    pub issuer: GovernanceIssuer,
    /// Trace linkage for the attempted transition.
    pub trace: GovernanceTraceLink,
}

impl GovernanceTransitionRejection {
    fn from_transition(transition: &GovernanceTransition, reason: impl Into<String>) -> Self {
        Self {
            schema_version: transition.schema_version.clone(),
            object: transition.object.clone(),
            scope: transition.scope.clone(),
            from: transition.from,
            attempted: transition.to,
            reason: reason.into(),
            rejected_at: transition.occurred_at,
            issuer: transition.issuer.clone(),
            trace: transition.trace.clone(),
        }
    }
}

/// Approval request schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApprovalRequest {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub approval_id: ApprovalId,
    pub scope: GovernanceScope,
    pub status: ApprovalStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl ApprovalRequest {
    /// Builds and validates a new approval request.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        approval_id: ApprovalId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
        extensions: GovernanceExtensions,
    ) -> Result<Self, GovernanceValidationError> {
        let request = Self {
            schema_version: default_schema_version(),
            approval_id,
            scope,
            status: ApprovalStatus::Requested,
            created_at,
            expires_at,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions,
        };
        request.validate()?;
        Ok(request)
    }

    /// Creates a grant decision for an active request.
    #[allow(clippy::too_many_arguments)]
    pub fn grant(
        &self,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
        extensions: GovernanceExtensions,
    ) -> Result<ApprovalGrant, GovernanceTransitionError> {
        let reason = reason.into();
        GovernanceTransition::try_new(
            GovernanceObjectRef::Approval {
                approval_id: self.approval_id.clone(),
            },
            self.scope.clone(),
            Some(self.status.into()),
            GovernanceState::Granted,
            created_at,
            reason.clone(),
            issuer.clone(),
            trace.clone(),
            extensions.clone(),
        )?;
        ApprovalGrant::new(
            self.approval_id.clone(),
            self.scope.clone(),
            created_at,
            expires_at,
            reason,
            issuer,
            trace,
            extensions,
        )
        .map_err(GovernanceTransitionError::Validation)
    }

    /// Creates a denial decision for an active request.
    #[allow(clippy::too_many_arguments)]
    pub fn deny(
        &self,
        created_at: OffsetDateTime,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
        extensions: GovernanceExtensions,
    ) -> Result<ApprovalDenial, GovernanceTransitionError> {
        let reason = reason.into();
        GovernanceTransition::try_new(
            GovernanceObjectRef::Approval {
                approval_id: self.approval_id.clone(),
            },
            self.scope.clone(),
            Some(self.status.into()),
            GovernanceState::Denied,
            created_at,
            reason.clone(),
            issuer.clone(),
            trace.clone(),
            extensions.clone(),
        )?;
        ApprovalDenial::new(
            self.approval_id.clone(),
            self.scope.clone(),
            created_at,
            reason,
            issuer,
            trace,
            extensions,
        )
        .map_err(GovernanceTransitionError::Validation)
    }

    /// Validates the approval request schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("approval_id", &self.approval_id)?;
        validate_status_member(
            matches!(
                self.status,
                ApprovalStatus::Requested | ApprovalStatus::Expired | ApprovalStatus::Revoked
            ),
            "approval_request",
            "requested, expired, or revoked",
        )?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Approval grant schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApprovalGrant {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub approval_id: ApprovalId,
    pub scope: GovernanceScope,
    pub status: ApprovalStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl ApprovalGrant {
    /// Builds and validates an approval grant.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        approval_id: ApprovalId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
        extensions: GovernanceExtensions,
    ) -> Result<Self, GovernanceValidationError> {
        let grant = Self {
            schema_version: default_schema_version(),
            approval_id,
            scope,
            status: ApprovalStatus::Granted,
            created_at,
            expires_at,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions,
        };
        grant.validate()?;
        Ok(grant)
    }

    /// Validates the approval grant schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("approval_id", &self.approval_id)?;
        validate_status_member(
            matches!(
                self.status,
                ApprovalStatus::Granted | ApprovalStatus::Expired | ApprovalStatus::Revoked
            ),
            "approval_grant",
            "granted, expired, or revoked",
        )?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Approval denial schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApprovalDenial {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub approval_id: ApprovalId,
    pub scope: GovernanceScope,
    pub status: ApprovalStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl ApprovalDenial {
    /// Builds and validates an approval denial.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        approval_id: ApprovalId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
        extensions: GovernanceExtensions,
    ) -> Result<Self, GovernanceValidationError> {
        let denial = Self {
            schema_version: default_schema_version(),
            approval_id,
            scope,
            status: ApprovalStatus::Denied,
            created_at,
            expires_at: None,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions,
        };
        denial.validate()?;
        Ok(denial)
    }

    /// Validates the approval denial schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("approval_id", &self.approval_id)?;
        validate_status_member(
            matches!(
                self.status,
                ApprovalStatus::Denied | ApprovalStatus::Revoked
            ),
            "approval_denial",
            "denied or revoked",
        )?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Escalation schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Escalation {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub escalation_id: EscalationId,
    pub scope: GovernanceScope,
    pub status: EscalationStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl Escalation {
    /// Builds and validates an open escalation.
    pub fn open(
        escalation_id: EscalationId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
    ) -> Result<Self, GovernanceValidationError> {
        let escalation = Self {
            schema_version: default_schema_version(),
            escalation_id,
            scope,
            status: EscalationStatus::Open,
            created_at,
            expires_at,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions: GovernanceExtensions::new(),
        };
        escalation.validate()?;
        Ok(escalation)
    }

    /// Validates the escalation schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("escalation_id", &self.escalation_id)?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Intervention schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Intervention {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub intervention_id: InterventionId,
    pub scope: GovernanceScope,
    pub status: InterventionStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl Intervention {
    /// Builds and validates an intervention request.
    pub fn requested(
        intervention_id: InterventionId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
    ) -> Result<Self, GovernanceValidationError> {
        let intervention = Self {
            schema_version: default_schema_version(),
            intervention_id,
            scope,
            status: InterventionStatus::Requested,
            created_at,
            expires_at,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions: GovernanceExtensions::new(),
        };
        intervention.validate()?;
        Ok(intervention)
    }

    /// Validates the intervention schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("intervention_id", &self.intervention_id)?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Governance-state circuit-breaker schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GovernanceCircuitBreaker {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub circuit_breaker_id: CircuitBreakerId,
    pub scope: GovernanceScope,
    pub status: CircuitBreakerStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl GovernanceCircuitBreaker {
    /// Builds and validates an active circuit breaker.
    pub fn active(
        circuit_breaker_id: CircuitBreakerId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
    ) -> Result<Self, GovernanceValidationError> {
        let breaker = Self {
            schema_version: default_schema_version(),
            circuit_breaker_id,
            scope,
            status: CircuitBreakerStatus::Active,
            created_at,
            expires_at,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions: GovernanceExtensions::new(),
        };
        breaker.validate()?;
        Ok(breaker)
    }

    /// Validates the circuit-breaker schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("circuit_breaker_id", &self.circuit_breaker_id)?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Kill-switch schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KillSwitch {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub kill_switch_id: KillSwitchId,
    pub scope: GovernanceScope,
    pub status: KillSwitchStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub reason: String,
    pub issuer: GovernanceIssuer,
    pub trace: GovernanceTraceLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<GovernanceRevocation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: GovernanceExtensions,
}

impl KillSwitch {
    /// Builds and validates an active kill switch.
    pub fn active(
        kill_switch_id: KillSwitchId,
        scope: GovernanceScope,
        created_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
        reason: impl Into<String>,
        issuer: GovernanceIssuer,
        trace: GovernanceTraceLink,
    ) -> Result<Self, GovernanceValidationError> {
        let kill_switch = Self {
            schema_version: default_schema_version(),
            kill_switch_id,
            scope,
            status: KillSwitchStatus::Active,
            created_at,
            expires_at,
            reason: reason.into(),
            issuer,
            trace,
            revocation: None,
            extensions: GovernanceExtensions::new(),
        };
        kill_switch.validate()?;
        Ok(kill_switch)
    }

    /// Validates the kill-switch schema.
    pub fn validate(&self) -> Result<(), GovernanceValidationError> {
        validate_uuid("kill_switch_id", &self.kill_switch_id)?;
        validate_lifecycle_markers(
            self.status.into(),
            self.expires_at,
            self.revocation.as_ref(),
        )?;
        validate_common(
            &self.schema_version,
            &self.scope,
            self.created_at,
            self.expires_at,
            &self.reason,
            &self.issuer,
            &self.trace,
            self.revocation.as_ref(),
            &self.extensions,
        )
    }
}

/// Validation failures for governance schemas.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum GovernanceValidationError {
    /// A governance schema version is not supported by this compatibility line.
    #[error("unsupported governance schema version: {schema}")]
    UnsupportedSchema { schema: String },
    /// A required field was missing, nil, or blank.
    #[error("{field} is required")]
    Missing { field: &'static str },
    /// A scope was malformed.
    #[error("invalid governance scope: {reason}")]
    InvalidScope { reason: String },
    /// Expiry must be after creation when present.
    #[error("expires_at must be after created_at")]
    InvalidExpiry,
    /// The status does not match the concrete object schema.
    #[error("invalid status for {object}: expected {expected}")]
    InvalidStatus {
        object: &'static str,
        expected: &'static str,
    },
    /// Extension keys or values attempted to carry authoritative fields.
    #[error("invalid governance extensions: {reason}")]
    InvalidExtensions { reason: String },
    /// Governance scope and trace linkage referenced different runs.
    #[error("governance run scope mismatch: expected {expected}, found {actual}")]
    RunScopeMismatch { expected: String, actual: String },
}

/// Transition failures for governance state changes.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum GovernanceTransitionError {
    /// Transition payload was malformed before state validation.
    #[error(transparent)]
    Validation(#[from] GovernanceValidationError),
    /// Transition is well formed but not allowed by the lifecycle table.
    #[error("governance transition rejected: {0:?}")]
    Rejected(Box<GovernanceTransitionRejection>),
}

fn default_schema_version() -> String {
    GOVERNANCE_STATE_SCHEMA_VERSION.to_string()
}

#[allow(clippy::too_many_arguments)]
fn validate_common(
    schema_version: &str,
    scope: &GovernanceScope,
    created_at: OffsetDateTime,
    expires_at: Option<OffsetDateTime>,
    reason: &str,
    issuer: &GovernanceIssuer,
    trace: &GovernanceTraceLink,
    revocation: Option<&GovernanceRevocation>,
    extensions: &GovernanceExtensions,
) -> Result<(), GovernanceValidationError> {
    validate_schema(schema_version)?;
    scope.validate()?;
    if let Some(expires_at) = expires_at {
        if expires_at <= created_at {
            return Err(GovernanceValidationError::InvalidExpiry);
        }
    }
    validate_non_blank("reason", reason)?;
    issuer.validate()?;
    trace.validate()?;
    validate_scope_trace_run_match(scope, trace)?;
    if let Some(revocation) = revocation {
        revocation.validate()?;
    }
    validate_extensions(extensions)
}

fn validate_scope_trace_run_match(
    scope: &GovernanceScope,
    trace: &GovernanceTraceLink,
) -> Result<(), GovernanceValidationError> {
    if let (Some(scope_run_id), Some(trace_run_id)) = (scope.run_id(), trace.run_id.as_ref()) {
        if scope_run_id != trace_run_id {
            return Err(GovernanceValidationError::RunScopeMismatch {
                expected: scope_run_id.to_string(),
                actual: trace_run_id.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_schema(schema_version: &str) -> Result<(), GovernanceValidationError> {
    if schema_version == GOVERNANCE_STATE_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(GovernanceValidationError::UnsupportedSchema {
            schema: schema_version.to_string(),
        })
    }
}

fn validate_status_member(
    valid: bool,
    object: &'static str,
    expected: &'static str,
) -> Result<(), GovernanceValidationError> {
    if valid {
        Ok(())
    } else {
        Err(GovernanceValidationError::InvalidStatus { object, expected })
    }
}

fn validate_lifecycle_markers(
    status: GovernanceState,
    expires_at: Option<OffsetDateTime>,
    revocation: Option<&GovernanceRevocation>,
) -> Result<(), GovernanceValidationError> {
    if status == GovernanceState::Expired && expires_at.is_none() {
        return Err(GovernanceValidationError::Missing {
            field: "expires_at",
        });
    }
    if status == GovernanceState::Revoked && revocation.is_none() {
        return Err(GovernanceValidationError::Missing {
            field: "revocation",
        });
    }
    if status != GovernanceState::Revoked && revocation.is_some() {
        return Err(GovernanceValidationError::InvalidStatus {
            object: "governance_object",
            expected: "revoked status when revocation is present",
        });
    }
    Ok(())
}

fn is_allowed_transition(
    kind: GovernanceObjectKind,
    from: Option<GovernanceState>,
    to: GovernanceState,
) -> bool {
    matches!(
        (kind, from, to),
        (
            GovernanceObjectKind::Approval,
            None,
            GovernanceState::Requested
        ) | (
            GovernanceObjectKind::Approval,
            Some(GovernanceState::Requested),
            GovernanceState::Granted
                | GovernanceState::Denied
                | GovernanceState::Expired
                | GovernanceState::Revoked
        ) | (
            GovernanceObjectKind::Approval,
            Some(GovernanceState::Granted),
            GovernanceState::Expired | GovernanceState::Revoked
        ) | (
            GovernanceObjectKind::Escalation,
            None,
            GovernanceState::Open
        ) | (
            GovernanceObjectKind::Escalation,
            Some(GovernanceState::Open),
            GovernanceState::Resolved | GovernanceState::Expired | GovernanceState::Revoked
        ) | (
            GovernanceObjectKind::Intervention,
            None,
            GovernanceState::Requested
        ) | (
            GovernanceObjectKind::Intervention,
            Some(GovernanceState::Requested),
            GovernanceState::Resolved
                | GovernanceState::Cancelled
                | GovernanceState::Expired
                | GovernanceState::Revoked
        ) | (
            GovernanceObjectKind::CircuitBreaker,
            None,
            GovernanceState::Active
        ) | (
            GovernanceObjectKind::CircuitBreaker,
            Some(GovernanceState::Active),
            GovernanceState::Cleared | GovernanceState::Expired | GovernanceState::Revoked
        ) | (
            GovernanceObjectKind::KillSwitch,
            None,
            GovernanceState::Active
        ) | (
            GovernanceObjectKind::KillSwitch,
            Some(GovernanceState::Active),
            GovernanceState::Cleared | GovernanceState::Expired | GovernanceState::Revoked
        )
    )
}

fn validate_non_blank(field: &'static str, value: &str) -> Result<(), GovernanceValidationError> {
    if value.trim().is_empty() || value.trim() != value {
        Err(GovernanceValidationError::Missing { field })
    } else {
        Ok(())
    }
}

fn validate_extensions(extensions: &GovernanceExtensions) -> Result<(), GovernanceValidationError> {
    for (key, value) in extensions {
        let normalized = normalize_extension_key(key);
        if key.trim().is_empty() || key.trim() != key {
            return Err(GovernanceValidationError::InvalidExtensions {
                reason: "blank_extension_key".to_string(),
            });
        }
        if is_reserved_extension_key(&normalized) {
            return Err(GovernanceValidationError::InvalidExtensions {
                reason: format!("reserved_extension_key:{key}"),
            });
        }
        reject_reserved_extension_value(value)?;
    }
    Ok(())
}

fn reject_reserved_extension_value(
    value: &serde_json::Value,
) -> Result<(), GovernanceValidationError> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let normalized = normalize_extension_key(key);
                if is_reserved_extension_key(&normalized) {
                    return Err(GovernanceValidationError::InvalidExtensions {
                        reason: format!("reserved_extension_key:{key}"),
                    });
                }
                reject_reserved_extension_value(child)?;
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_reserved_extension_value(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn normalize_extension_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace('-', "_")
}

fn is_reserved_extension_key(key: &str) -> bool {
    matches!(
        key,
        "scope"
            | "scope_type"
            | "tenant_id"
            | "agent_id"
            | "run_id"
            | "action_id"
            | "adapter"
            | "fleet_id"
            | "node_id"
            | "instance_id"
            | "issuer"
            | "issuer_id"
            | "source"
            | "reason"
            | "created_at"
            | "expires_at"
            | "revocation"
            | "revoked_at"
            | "status"
            | "state"
            | "allowed_actions"
            | "allowed_adapters"
            | "allowed_permissions"
            | "permissions"
            | "authority"
            | "credential"
            | "credentials"
            | "work_order"
            | "signature"
            | "approval_token"
    )
}

trait GovernanceUuid {
    fn is_nil(&self) -> bool;
}

macro_rules! impl_governance_uuid {
    ($($name:ident),+ $(,)?) => {
        $(
            impl GovernanceUuid for $name {
                fn is_nil(&self) -> bool {
                    self.is_nil()
                }
            }
        )+
    };
}

impl_governance_uuid!(
    ActionId,
    AgentId,
    ApprovalId,
    CircuitBreakerId,
    EscalationId,
    FleetId,
    InstanceId,
    InterventionId,
    KillSwitchId,
    NodeId,
    RunId,
    TenantId,
    TraceEventId,
);

fn validate_uuid<T: GovernanceUuid>(
    field: &'static str,
    value: &T,
) -> Result<(), GovernanceValidationError> {
    if value.is_nil() {
        Err(GovernanceValidationError::Missing { field })
    } else {
        Ok(())
    }
}

fn validate_optional_uuid<T: GovernanceUuid>(
    field: &'static str,
    value: Option<&T>,
) -> Result<(), GovernanceValidationError> {
    match value {
        Some(value) => validate_uuid(field, value),
        None => Ok(()),
    }
}

fn normalize_reason(reason: impl Into<String>) -> Result<String, CircuitBreakerValidationError> {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return Err(CircuitBreakerValidationError::EmptyReason);
    }
    Ok(reason)
}

fn side_effect_class_label(value: &SideEffectClass) -> String {
    match value {
        SideEffectClass::ReadOnly => "read_only".to_string(),
        SideEffectClass::Filesystem => "filesystem".to_string(),
        SideEffectClass::Network => "network".to_string(),
        SideEffectClass::External => "external".to_string(),
        SideEffectClass::Custom(value) => format!("custom:{value}"),
    }
}

#[cfg(test)]
#[path = "../tests/unit/governance_tests.rs"]
mod tests;
