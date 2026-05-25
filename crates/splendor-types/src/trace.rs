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
    Action, Constraint, ContentHash, Feedback, MessageTraceContext, Reward, RunId, SnapshotId,
    TraceId, VerificationResult,
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
    /// Marks the end of a loop tick.
    LoopTickCompleted {
        /// Tick counter within the run.
        tick_id: u64,
        /// Optional integrity chain metadata for audit validation.
        integrity: Option<TraceIntegrity>,
    },
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
