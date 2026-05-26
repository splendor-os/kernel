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
mod runtime;
mod scheduler;
mod state;
mod tenancy;
mod trace;

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
    Feedback, HashAlgorithm, Message, MessageDeliveryStatus, MessageEnvelope, MessageId,
    MessageSchemaVersion, MessageTraceContext, MessageTraceLinks, MessageValidationError, Percept,
    PerceptProvenance, QuotaUsage, Reward, RunId, SideEffectClass, SnapshotId, TenantId,
    TraceEvent, TraceEventKind, TraceId, VerificationResult,
};
pub use state::{SnapshotPolicy, StateCommit, StateGraph, StateGraphError};
pub use tenancy::{
    AdapterQuota, AgentContext, AgentIsolationPolicy, AgentRuntimeConfig, QuotaLedger, QuotaPolicy,
    TenantContext, TenantPolicy, TenantRegistry, AGENT_ISOLATION_LEDGER_SOURCE,
    QUOTA_LEDGER_SOURCE,
};
pub use trace::{AsyncTraceSink, StdoutTraceSink, TraceError, TraceSink, TraceStoreSink};
