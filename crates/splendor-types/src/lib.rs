//! # Splendor Kernel Types
//!
//! Canonical data structures that form Splendor's kernel contract: stable
//! identifiers, trace event taxonomy, verification outcomes, and
//! content-addressed hashes. These types are deterministic, serializable, and
//! safe to persist across runs so higher-level components can replay and audit
//! agent behavior.
//!
//! ## Design goals
//! - **Deterministic** identifiers for reproducibility.
//! - **Serializable** payloads for trace and state storage.
//! - **Auditable** structures for governance and debugging.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_types::{Action, CostEstimate, SideEffectClass};
//!
//! let action = Action {
//!     name: "http_get".to_string(),
//!     params: serde_json::json!({"url": "https://example.com"}),
//!     side_effect_class: SideEffectClass::Network,
//!     cost_estimate: Some(CostEstimate {
//!         units: "ms".to_string(),
//!         amount: 25.0,
//!     }),
//!     required_permissions: vec!["http:read".to_string()],
//!     preconditions: vec!["allowed_domain".to_string()],
//!     postconditions: vec!["status:200".to_string()],
//! };
//! assert_eq!(action.name, "http_get");
//! ```

mod approval;
mod capabilities;
mod daemon_security;
mod escalation;
mod fleet_telemetry;
mod governance;
mod hash;
mod ids;
mod message;
mod node_registry;
mod placement;
mod primitives;
mod state_handoff;
mod trace;
mod work_order;

pub use approval::{
    ApprovalActionScope, ApprovalDecision, ApprovalEvidence, ApprovalPolicy, ApprovalTraceContext,
    APPROVAL_EVIDENCE_SCHEMA_VERSION, APPROVAL_POLICY_SCHEMA_VERSION,
};
pub use capabilities::{
    is_valid_capability_name, CapabilityDocument, CapabilityValidationError,
    CAPABILITY_DOCUMENT_SCHEMA,
};
pub use daemon_security::{
    validate_client_connection_policy, validate_daemon_request, validate_insecure_dev_mode,
    AppPrincipal, AuditAttribution, CallerCredential, ClientConnectionPolicy, ClientPrincipal,
    CredentialAudience, CredentialBinding, DaemonEndpoint, DaemonSecurityDecision,
    DaemonSecurityError, DaemonSecurityRequest, EndpointScope, GatewayVerificationState,
    InsecureDevMode, LocalTransportBinding, RevocationStatus, WorkOrderAuthorization,
    WorkOrderSignature,
};
pub use escalation::{
    EscalationContext, EscalationDecision, EscalationObservation, EscalationPolicy,
    EscalationPolicyError, EscalationRule, EscalationScope, EscalationTrigger,
    ESCALATION_POLICY_SCHEMA_VERSION,
};
pub use fleet_telemetry::{
    DenialSignal, FailureCategory, FailureSignal, FleetTelemetrySnapshot, InstanceTelemetry,
    NodeOnlineState, NodeTelemetry, QueueTelemetry, QuotaSignal, RunStatus, RunStatusCount,
    RunStatusCounts, RunTelemetry, RuntimeMode as TelemetryRuntimeMode, TelemetryAuthority,
    TraceSyncFailure, TraceSyncTelemetry, FLEET_TELEMETRY_SCHEMA_VERSION,
};
pub use governance::{
    ApprovalDenial, ApprovalGrant, ApprovalRequest, ApprovalStatus, CircuitBreaker,
    CircuitBreakerMatch, CircuitBreakerScope, CircuitBreakerState, CircuitBreakerStatus,
    CircuitBreakerTraceContext, CircuitBreakerValidationError, Escalation, EscalationStatus,
    GovernanceCircuitBreaker, GovernanceExtensions, GovernanceIssuer, GovernanceObjectKind,
    GovernanceObjectRef, GovernanceRevocation, GovernanceScope, GovernanceState,
    GovernanceTraceLink, GovernanceTransition, GovernanceTransitionError,
    GovernanceTransitionRejection, GovernanceValidationError, Intervention, InterventionStatus,
    KillSwitch, KillSwitchStatus, CIRCUIT_BREAKER_SCHEMA_VERSION, GOVERNANCE_STATE_SCHEMA_VERSION,
};
pub use hash::{ContentHash, HashAlgorithm};
pub use ids::{
    ActionId, AgentId, ApprovalId, CircuitBreakerId, EscalationId, FleetId,
    IdentityValidationError, InstanceId, InterventionId, KillSwitchId, MessageId, NodeId, RunId,
    RuntimeIdentityContext, SnapshotId, StateNodeId, TenantId, TickId, TraceEventId, TraceId,
    TraceIdentityContext, WorkOrderId, WorkOrderIdError,
};
pub use message::{
    DelegatedAuthority, Message, MessageDeliveryStatus, MessageEnvelope, MessageSchemaVersion,
    MessageTraceContext, MessageTraceLinks, MessageValidationError, RemoteMessageEnvelope,
    RemoteMessageEnvelopeVersion, RemoteMessageRetryPolicy, RemoteMessageTraceContext,
    RemoteMessageValidationError, TaskFailure, TaskRequest, TaskResponse, TaskResponseStatus,
    TASK_REQUEST_SCHEMA, TASK_RESPONSE_SCHEMA,
};
pub use node_registry::{
    HealthStatus, InstanceHealth, InstanceHeartbeat, InstanceRegistration, ManagementAuditEvent,
    ManagementAuditEventKind, NodeHealth, NodeHeartbeat, NodeKind, NodeRegistration,
    NodeRegistryValidationError, RegistryScope, RuntimeMode,
};
pub use placement::{
    select_placement, DataLocality, PlacementCandidate, PlacementCandidateEvaluation,
    PlacementDecision, PlacementDecisionStatus, PlacementExecutionMode, PlacementExplain,
    PlacementRejectionReason, PlacementRequest, PlacementTarget, PlacementTraceAudit,
    PLACEMENT_DECISION_SCHEMA,
};
pub use primitives::{
    Action, Constraint, ConstraintKind, ConstraintScope, CostEstimate, Feedback, Percept,
    PerceptProvenance, QuotaUsage, Reward, SideEffectClass, VerificationResult,
};
pub use state_handoff::{
    StateHandoff, StateHandoffAuthority, StateHandoffSnapshot, StateHandoffTraceContext,
    StateReference, StateReferenceMode,
};
pub use trace::{LocalDelegationTraceContext, TraceEvent, TraceEventKind, TraceIntegrity};
pub use work_order::{
    validate_work_order, ValidatedWorkOrder, WorkOrder, WorkOrderEnvelope, WorkOrderKeyring,
    WorkOrderPlacement, WorkOrderQuotaPolicy, WorkOrderValidationContext, WorkOrderValidationError,
    WORK_ORDER_SCHEMA_VERSION, WORK_ORDER_SIGNATURE_ALGORITHM,
};

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod tests;
