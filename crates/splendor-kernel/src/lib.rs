//! # Splendor Kernel Runtime
//!
//! The kernel crate exposes the runtime surface used by higher-level systems to
//! emit trace events, manage explicit state graphs, and drive deterministic
//! kernel loops. It re-exports the stable primitives from `splendor-types` so
//! consumers can build against a unified contract.
//!
//! ## Capabilities
//! - Emit ordered trace events via pluggable sinks.
//! - Commit explicit state graph nodes and snapshots.
//! - Provide a stable API surface for kernel-adjacent components.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_kernel::{KernelRuntime, KernelRuntimeConfig, TraceEventKind};
//!
//! let runtime = KernelRuntime::new(KernelRuntimeConfig::default());
//! let event = runtime
//!     .record_event(TraceEventKind::LoopTickCompleted {
//!         tick_id: 1,
//!         integrity: None,
//!     })
//!     .expect("record");
//! assert_eq!(event.sequence, 0);
//! ```

mod loop_engine;
mod message_router;
mod node_registry;
mod remote_message_transport;
mod runtime;
mod scheduler;
mod state;
mod tenancy;
mod trace;

pub use loop_engine::{
    ActionCandidate, AllowAllConstraintEngine, ConstraintEngine, ConstraintEvaluation, LoopEngine,
    LoopError, NoopOutcomeEvaluator, OutcomeEvaluator, OutcomeSignal, Perceptor, Policy,
    PolicyDecision, ResumeInfo, RunTraceContext, TickOutcome,
};
pub use message_router::{
    AgentMailboxSnapshot, LocalMessageRouter, MessageRouter, MessageRouterConfig,
    MessageRouterError, MessageTraceRecorder,
};
pub use node_registry::{
    HeartbeatFreshness, InMemoryManagementAuditSink, InMemoryNodeRegistry, InstanceRecord,
    ManagementAuditError, ManagementAuditSink, NodeRecord, NodeRegistry, NodeRegistryConfig,
    NodeRegistryError, RegistryHealthStatus,
};
pub use remote_message_transport::{
    send_remote_message, InMemoryRemoteMessageTransport, InMemoryRemoteTransportFault,
    RemoteMessageReceiver, RemoteMessageTransport, RemoteMessageTransportError,
};
pub use runtime::{KernelRuntime, KernelRuntimeConfig};
pub use scheduler::{Scheduler, SchedulerConfig, SchedulerError, SchedulerStep};
pub use splendor_types::{
    Action, ActionId, AgentId, CapabilityDocument, CapabilityValidationError, Constraint,
    ConstraintKind, ConstraintScope, ContentHash, CostEstimate, Feedback, FleetId, HashAlgorithm,
    HealthStatus, IdentityValidationError, InstanceHealth, InstanceHeartbeat, InstanceId,
    InstanceRegistration, ManagementAuditEvent, ManagementAuditEventKind, Message,
    MessageDeliveryStatus, MessageEnvelope, MessageId, MessageSchemaVersion, MessageTraceContext,
    MessageTraceLinks, MessageValidationError, NodeHealth, NodeHeartbeat, NodeId, NodeKind,
    NodeRegistration, NodeRegistryValidationError, Percept, PerceptProvenance, QuotaUsage,
    RegistryScope, RemoteMessageEnvelope, RemoteMessageEnvelopeVersion, RemoteMessageRetryPolicy,
    RemoteMessageTraceContext, RemoteMessageValidationError, Reward, RunId, RuntimeIdentityContext,
    RuntimeMode, SideEffectClass, SnapshotId, StateNodeId, TenantId, TickId, TraceEvent,
    TraceEventId, TraceEventKind, TraceId, TraceIdentityContext, VerificationResult,
};
pub use state::{SnapshotPolicy, StateCommit, StateGraph, StateGraphError};
pub use tenancy::{
    AdapterQuota, AgentContext, AgentRuntimeConfig, QuotaLedger, QuotaPolicy, TenantContext,
    TenantPolicy, TenantRegistry,
};
pub use trace::{AsyncTraceSink, StdoutTraceSink, TraceError, TraceSink, TraceStoreSink};
