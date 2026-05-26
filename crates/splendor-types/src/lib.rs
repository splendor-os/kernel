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

mod capabilities;
mod daemon_security;
mod hash;
mod ids;
mod message;
mod node_registry;
mod primitives;
mod trace;

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
pub use hash::{ContentHash, HashAlgorithm};
pub use ids::{
    AgentId, FleetId, InstanceId, MessageId, NodeId, RunId, SnapshotId, TenantId, TraceId,
};
pub use message::{
    Message, MessageDeliveryStatus, MessageEnvelope, MessageSchemaVersion, MessageTraceContext,
    MessageTraceLinks, MessageValidationError,
};
pub use node_registry::{
    HealthStatus, InstanceHealth, InstanceHeartbeat, InstanceRegistration, ManagementAuditEvent,
    ManagementAuditEventKind, NodeHealth, NodeHeartbeat, NodeKind, NodeRegistration,
    NodeRegistryValidationError, RegistryScope, RuntimeMode,
};
pub use primitives::{
    Action, Constraint, ConstraintKind, ConstraintScope, CostEstimate, Feedback, Percept,
    PerceptProvenance, QuotaUsage, Reward, SideEffectClass, VerificationResult,
};
pub use trace::{TraceEvent, TraceEventKind, TraceIntegrity};

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod tests;
