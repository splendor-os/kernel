//! # Trace Events
//!
//! Trace events form the append-only audit log of each kernel tick. Events are
//! ordered by sequence number within a run and are designed to be serialized for
//! replay, debugging, and governance review.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_types::{RunId, TraceEvent, TraceEventKind};
//! use time::OffsetDateTime;
//!
//! let run_id = RunId::new();
//! let event = TraceEvent::new(
//!     run_id,
//!     0,
//!     OffsetDateTime::now_utc(),
//!     TraceEventKind::LoopTickStarted { tick_id: 1 },
//! );
//! assert_eq!(event.sequence, 0);
//! ```

use crate::{
    Action, AgentId, AuditAttribution, Constraint, ContentHash, Feedback, MessageId,
    MessageTraceContext, Reward, RunId, SnapshotId, TaskFailure, TraceId, VerificationResult,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Immutable record describing a single kernel trace event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Deterministic identifier for this event.
    pub trace_id: TraceId,
    /// Run identifier that scopes the event stream.
    pub run_id: RunId,
    /// Monotonic sequence number for ordering.
    pub sequence: u64,
    /// Timestamp captured at emission.
    pub timestamp: OffsetDateTime,
    /// Event payload describing the loop step.
    pub kind: TraceEventKind,
}

impl TraceEvent {
    /// Creates a new trace event with a deterministic trace ID.
    pub fn new(
        run_id: RunId,
        sequence: u64,
        timestamp: OffsetDateTime,
        kind: TraceEventKind,
    ) -> Self {
        let trace_id = TraceId::from_run_sequence(&run_id, sequence);
        Self {
            trace_id,
            run_id,
            sequence,
            timestamp,
            kind,
        }
    }
}

/// Ordered event taxonomy for a kernel tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TraceEventKind {
    /// Marks the start of a run trace stream.
    RunStarted,
    /// Records a local daemon run pause transition.
    RunPaused {
        /// Human-readable reason for the pause.
        reason: Option<String>,
    },
    /// Records a local daemon run resume transition.
    RunResumed {
        /// Human-readable reason for the resume.
        reason: Option<String>,
    },
    /// Records a local daemon run stop transition.
    RunStopped {
        /// Human-readable reason for the stop.
        reason: Option<String>,
    },
    /// Records percepts accepted by the local daemon before a tick consumes them.
    PerceptsAppended {
        /// Number of percepts accepted.
        count: usize,
        /// Accepted percept schemas.
        schemas: Vec<String>,
    },
    /// Records caller attribution for a mutating daemon operation.
    DaemonAudit {
        /// Endpoint or endpoint scope that accepted the mutating request.
        endpoint: String,
        /// Caller attribution validated at the daemon boundary.
        audit: AuditAttribution,
    },
    /// Marks the start of a loop tick.
    LoopTickStarted {
        /// Tick counter within the run.
        tick_id: u64,
    },
    /// Records percepts delivered to the policy.
    PerceptsReceived {
        /// Percepts collected for this tick.
        percepts: Vec<crate::Percept>,
    },
    /// Records the state snapshot/hash available to the policy for this tick.
    StateLoaded {
        /// Hash of the loaded state bytes when available.
        state_hash: Option<ContentHash>,
    },
    /// Signals that the policy callback has been invoked.
    PolicyInvoked {
        /// Identifier for the policy implementation.
        policy: String,
    },
    /// Signals that the policy callback completed successfully.
    PolicyCompleted {
        /// Identifier for the policy implementation.
        policy: String,
    },
    /// Captures candidate actions proposed by the policy.
    CandidatesProposed {
        /// Candidate actions for verification.
        actions: Vec<Action>,
    },
    /// Records constraint evaluation results.
    ConstraintsEvaluated {
        /// Constraints that were evaluated.
        constraints: Vec<Constraint>,
        /// Aggregate verification outcome.
        result: VerificationResult,
    },
    /// Indicates that action verification has begun.
    ActionVerificationStarted {
        /// Action being verified.
        action: Action,
    },
    /// Captures the completed verification result.
    ActionVerificationCompleted {
        /// Action that was verified.
        action: Action,
        /// Result of verification checks.
        result: VerificationResult,
    },
    /// Records a successfully executed action and its output.
    ActionExecuted {
        /// Executed action.
        action: Action,
        /// Adapter output payload.
        outcome: serde_json::Value,
    },
    /// Records a denied action and the reason.
    ActionDenied {
        /// Action that was denied.
        action: Action,
        /// Verification result describing the denial.
        result: VerificationResult,
    },
    /// Records an action that failed during or after adapter execution.
    ActionFailed {
        /// Action that failed.
        action: Action,
        /// Error message describing the failure.
        error: String,
        /// Verification or post-verification result associated with the failure.
        result: VerificationResult,
    },
    /// Captures final outcome, feedback, and reward signals.
    OutcomeRecorded {
        /// Outcome payload from adapters or policy.
        outcome: serde_json::Value,
        /// Optional feedback signal.
        feedback: Option<Feedback>,
        /// Optional reward signal.
        reward: Option<Reward>,
    },
    /// Records committed state and optional snapshot identifiers.
    StateCommitted {
        /// Hash of the new committed state.
        state_hash: ContentHash,
        /// Snapshot identifier when one was created.
        snapshot_id: Option<SnapshotId>,
    },
    /// Records a message accepted into a local delivery path.
    MessageQueued {
        /// Identity and causality context for the message.
        message: MessageTraceContext,
    },
    /// Records a message reaching the target agent's delivery boundary.
    MessageDelivered {
        /// Identity and causality context for the message.
        message: MessageTraceContext,
    },
    /// Records a message rejected before delivery.
    MessageRejected {
        /// Identity and causality context for the message.
        message: MessageTraceContext,
        /// Fail-closed rejection reason.
        reason: String,
    },
    /// Records a message expiring before delivery or consumption.
    MessageExpired {
        /// Identity and causality context for the message.
        message: MessageTraceContext,
        /// Optional expiration reason.
        reason: Option<String>,
    },
    /// Records a message consumed by the target agent runtime context.
    MessageConsumed {
        /// Identity and causality context for the message.
        message: MessageTraceContext,
    },
    /// Records a parent run requesting a scoped local child run.
    DelegationRequested {
        /// Local parent/child delegation context.
        delegation: LocalDelegationTraceContext,
    },
    /// Records a local delegation denied before a child run can execute.
    DelegationRejected {
        /// Local parent/child delegation context.
        delegation: LocalDelegationTraceContext,
        /// Fail-closed rejection reason.
        reason: String,
    },
    /// Records parent run cancellation for local delegation admission control.
    ParentRunCancelled {
        /// Cancelled parent run.
        parent_run_id: RunId,
        /// Agent that owned the cancelled parent run.
        agent_id: AgentId,
        /// Structured cancellation reason.
        reason: String,
    },
    /// Records that a child run started from an explicit local delegation.
    ChildRunStarted {
        /// Local parent/child delegation context.
        delegation: LocalDelegationTraceContext,
    },
    /// Records a child run completing successfully and linking back to parent.
    ChildRunCompleted {
        /// Local parent/child delegation context.
        delegation: LocalDelegationTraceContext,
    },
    /// Records a child run failure as a structured outcome.
    ChildRunFailed {
        /// Local parent/child delegation context.
        delegation: LocalDelegationTraceContext,
        /// Structured child failure outcome.
        failure: TaskFailure,
    },
    /// Records an explicit local parent/child run relationship for replay.
    ChildRunLinked {
        /// Parent run that delegated local work.
        parent_run_id: RunId,
        /// Child run receiving scoped local work.
        child_run_id: RunId,
        /// Agent that owns the parent run side of the relationship.
        parent_agent_id: AgentId,
        /// Agent that owns the child run side of the relationship.
        child_agent_id: AgentId,
        /// Optional trace event that caused the child run link.
        causal_parent: Option<TraceId>,
        /// Optional message that carried the local delegation request.
        source_message_id: Option<MessageId>,
    },
    /// Marks the end of a loop tick.
    LoopTickCompleted {
        /// Tick counter within the run.
        tick_id: u64,
        /// Optional integrity chain metadata for audit validation.
        integrity: Option<TraceIntegrity>,
    },
}

/// Trace context for local parent/child delegation events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalDelegationTraceContext {
    /// Parent run that requested scoped local work.
    pub parent_run_id: RunId,
    /// Child run created for the delegated work.
    pub child_run_id: RunId,
    /// Parent trace event that caused or recorded the delegation request.
    pub parent_trace_id: Option<TraceId>,
    /// Task request message associated with the delegation, if created.
    pub request_message_id: Option<MessageId>,
    /// Task response message associated with completion/failure, if created.
    pub response_message_id: Option<MessageId>,
    /// Parent/orchestrator agent.
    pub source_agent_id: AgentId,
    /// Child/specialist agent.
    pub target_agent_id: AgentId,
    /// Scoped child objective.
    pub objective: String,
}

impl LocalDelegationTraceContext {
    /// Returns a copy with a request message link.
    pub fn with_request_message(mut self, message_id: MessageId) -> Self {
        self.request_message_id = Some(message_id);
        self
    }

    /// Returns a copy with a response message link.
    pub fn with_response_message(mut self, message_id: MessageId) -> Self {
        self.response_message_id = Some(message_id);
        self
    }

    /// Returns a copy with the parent trace event that caused this delegation.
    pub fn with_parent_trace(mut self, trace_id: TraceId) -> Self {
        self.parent_trace_id = Some(trace_id);
        self
    }
}

/// Integrity metadata recorded at the end of a tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceIntegrity {
    /// Hash of the previous event in the run, if any.
    pub prev_event_hash: Option<ContentHash>,
    /// Hash of this LoopTickCompleted event (computed before embedding integrity).
    pub event_hash: ContentHash,
}

#[cfg(test)]
#[path = "../tests/unit/trace_tests.rs"]
mod tests;
