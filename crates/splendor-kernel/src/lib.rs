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

mod fleet_telemetry;
mod loop_engine;
mod message_router;
mod runtime;
mod scheduler;
mod state;
mod tenancy;
mod trace;

pub use fleet_telemetry::{FleetTelemetryCollector, TelemetryThresholds};
pub use loop_engine::{
    ActionCandidate, AllowAllConstraintEngine, ConstraintEngine, ConstraintEvaluation, LoopEngine,
    LoopError, NoopOutcomeEvaluator, OutcomeEvaluator, OutcomeSignal, Perceptor, Policy,
    PolicyDecision, ResumeInfo, TickOutcome,
};
pub use message_router::{
    AgentMailboxSnapshot, LocalMessageRouter, MessageRouter, MessageRouterConfig,
    MessageRouterError, MessageTraceRecorder,
};
pub use runtime::{KernelRuntime, KernelRuntimeConfig};
pub use scheduler::{Scheduler, SchedulerConfig, SchedulerError, SchedulerStep};
pub use splendor_types::{
    Action, AgentId, Constraint, ConstraintKind, ConstraintScope, ContentHash, CostEstimate,
    DenialSignal, FailureCategory, FailureSignal, Feedback, FleetId, FleetTelemetrySnapshot,
    HashAlgorithm, InstanceId, InstanceTelemetry, Message, MessageDeliveryStatus, MessageEnvelope,
    MessageId, MessageSchemaVersion, MessageTraceContext, MessageTraceLinks,
    MessageValidationError, NodeId, NodeOnlineState, NodeTelemetry, Percept, PerceptProvenance,
    QueueTelemetry, QuotaSignal, QuotaUsage, Reward, RunId, RunStatus, RunStatusCount,
    RunStatusCounts, RunTelemetry, RuntimeMode, SideEffectClass, SnapshotId, TelemetryAuthority,
    TenantId, TraceEvent, TraceEventKind, TraceId, TraceSyncFailure, TraceSyncTelemetry,
    VerificationResult, FLEET_TELEMETRY_SCHEMA_VERSION,
};
pub use state::{SnapshotPolicy, StateCommit, StateGraph, StateGraphError};
pub use tenancy::{
    AdapterQuota, AgentContext, AgentRuntimeConfig, QuotaLedger, QuotaPolicy, TenantContext,
    TenantPolicy, TenantRegistry,
};
pub use trace::{AsyncTraceSink, StdoutTraceSink, TraceError, TraceSink, TraceStoreSink};
