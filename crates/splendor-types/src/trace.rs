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
    Action, AgentId, ApprovalTraceContext, AuditAttribution, Constraint, ContentHash, Feedback,
    GovernanceScope, GovernanceTransition, GovernanceTransitionRejection, IdentityValidationError,
    MessageId, MessageTraceContext, RemoteMessageTraceContext, Reward, RunId, SnapshotId,
    StateHandoffTraceContext, TaskFailure, TenantId, TickId, TraceEventId, TraceId,
    TraceIdentityContext, VerificationResult, WorkOrderId, EscalationContext,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Immutable record describing a single kernel trace event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Deterministic identifier for this event.
    #[serde(rename = "trace_event_id", alias = "trace_id")]
    pub trace_event_id: TraceEventId,
    /// Run identifier that scopes the event stream.
    pub run_id: RunId,
    /// Monotonic sequence number for ordering.
    pub sequence: u64,
    /// Timestamp captured at emission.
    pub timestamp: OffsetDateTime,
    /// Identity context needed to locate the runtime boundary that emitted this event.
    pub identity: TraceIdentityContext,
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
        let identity = apply_kind_identity(TraceIdentityContext::new(run_id.clone()), &kind);
        let trace_event_id = TraceEventId::from_run_sequence(&run_id, sequence);
        Self {
            trace_event_id,
            run_id,
            sequence,
            timestamp,
            identity,
            kind,
        }
    }

    /// Creates and validates a trace event from an explicit identity context.
    pub fn try_new_with_identity(
        identity: TraceIdentityContext,
        sequence: u64,
        timestamp: OffsetDateTime,
        kind: TraceEventKind,
    ) -> Result<Self, IdentityValidationError> {
        let identity = apply_kind_identity(identity, &kind);
        validate_kind_identity(&identity, &kind)?;
        identity.validate()?;
        let run_id = identity.run_id.clone();
        let trace_event_id = TraceEventId::from_run_sequence(&run_id, sequence);
        Ok(Self {
            trace_event_id,
            run_id,
            sequence,
            timestamp,
            identity,
            kind,
        })
    }
}

fn validate_kind_identity(
    identity: &TraceIdentityContext,
    kind: &TraceEventKind,
) -> Result<(), IdentityValidationError> {
    if let Some(scope) = governance_scope_for_kind(kind) {
        if let Some(scoped_run_id) = scope.run_id() {
            if scoped_run_id != &identity.run_id {
                return Err(IdentityValidationError::Mismatch {
                    field: "governance_scope.run_id",
                    expected: identity.run_id.to_string(),
                    actual: scoped_run_id.to_string(),
                });
            }
        }
    }
    Ok(())
}

fn governance_scope_for_kind(kind: &TraceEventKind) -> Option<&GovernanceScope> {
    match kind {
        TraceEventKind::GovernanceApprovalRequested { transition }
        | TraceEventKind::GovernanceApprovalGranted { transition }
        | TraceEventKind::GovernanceApprovalDenied { transition }
        | TraceEventKind::GovernanceApprovalExpired { transition }
        | TraceEventKind::GovernanceApprovalRevoked { transition }
        | TraceEventKind::EscalationOpened { transition }
        | TraceEventKind::EscalationResolved { transition }
        | TraceEventKind::EscalationExpired { transition }
        | TraceEventKind::EscalationRevoked { transition }
        | TraceEventKind::InterventionRequested { transition }
        | TraceEventKind::InterventionResolved { transition }
        | TraceEventKind::InterventionCancelled { transition }
        | TraceEventKind::InterventionExpired { transition }
        | TraceEventKind::InterventionRevoked { transition }
        | TraceEventKind::CircuitBreakerTripped { transition }
        | TraceEventKind::CircuitBreakerCleared { transition }
        | TraceEventKind::CircuitBreakerExpired { transition }
        | TraceEventKind::CircuitBreakerRevoked { transition }
        | TraceEventKind::KillSwitchActivated { transition }
        | TraceEventKind::KillSwitchCleared { transition }
        | TraceEventKind::KillSwitchExpired { transition }
        | TraceEventKind::KillSwitchRevoked { transition } => Some(&transition.scope),
        TraceEventKind::GovernanceTransitionRejected { rejection } => Some(&rejection.scope),
        _ => None,
    }
}

fn apply_kind_identity(
    mut identity: TraceIdentityContext,
    kind: &TraceEventKind,
) -> TraceIdentityContext {
    match kind {
        TraceEventKind::LoopTickStarted { tick_id }
        | TraceEventKind::LoopTickCompleted { tick_id, .. } => {
            identity.tick_id.get_or_insert(TickId::from(*tick_id));
        }
        TraceEventKind::ApprovalRequested { approval }
        | TraceEventKind::ApprovalGranted { approval }
        | TraceEventKind::ApprovalDenied { approval, .. }
        | TraceEventKind::ApprovalExpired { approval, .. }
        | TraceEventKind::ApprovalRevoked { approval, .. } => {
            identity
                .approval_id
                .get_or_insert_with(|| approval.approval_id.clone());
            if let Some(action_id) = &approval.action_id {
                identity.action_id.get_or_insert_with(|| action_id.clone());
            }
        }
        TraceEventKind::MessageQueued { message }
        | TraceEventKind::MessageDelivered { message }
        | TraceEventKind::MessageRejected { message, .. }
        | TraceEventKind::MessageExpired { message, .. }
        | TraceEventKind::MessageConsumed { message } => {
            identity
                .message_id
                .get_or_insert_with(|| message.message_id.clone());
        }
        TraceEventKind::RemoteMessageSent { remote_message }
        | TraceEventKind::RemoteMessageAccepted { remote_message }
        | TraceEventKind::RemoteMessageRejected { remote_message, .. }
        | TraceEventKind::RemoteMessageDelivered { remote_message }
        | TraceEventKind::RemoteMessageTimedOut { remote_message, .. }
        | TraceEventKind::RemoteMessageDuplicate { remote_message, .. }
        | TraceEventKind::RemoteMessageTransportFailed { remote_message, .. } => {
            identity
                .message_id
                .get_or_insert_with(|| remote_message.message.message_id.clone());
        }
        TraceEventKind::GovernanceApprovalRequested { transition }
        | TraceEventKind::GovernanceApprovalGranted { transition }
        | TraceEventKind::GovernanceApprovalDenied { transition }
        | TraceEventKind::GovernanceApprovalExpired { transition }
        | TraceEventKind::GovernanceApprovalRevoked { transition }
        | TraceEventKind::EscalationOpened { transition }
        | TraceEventKind::EscalationResolved { transition }
        | TraceEventKind::EscalationExpired { transition }
        | TraceEventKind::EscalationRevoked { transition }
        | TraceEventKind::InterventionRequested { transition }
        | TraceEventKind::InterventionResolved { transition }
        | TraceEventKind::InterventionCancelled { transition }
        | TraceEventKind::InterventionExpired { transition }
        | TraceEventKind::InterventionRevoked { transition }
        | TraceEventKind::CircuitBreakerTripped { transition }
        | TraceEventKind::CircuitBreakerCleared { transition }
        | TraceEventKind::CircuitBreakerExpired { transition }
        | TraceEventKind::CircuitBreakerRevoked { transition }
        | TraceEventKind::KillSwitchActivated { transition }
        | TraceEventKind::KillSwitchCleared { transition }
        | TraceEventKind::KillSwitchExpired { transition }
        | TraceEventKind::KillSwitchRevoked { transition } => {
            apply_governance_scope_identity(&mut identity, &transition.scope);
        }
        TraceEventKind::GovernanceTransitionRejected { rejection } => {
            apply_governance_scope_identity(&mut identity, &rejection.scope);
        }
        TraceEventKind::EscalationTriggered { escalation } => {
            if let Some(action_id) = escalation.action_id.as_ref() {
                identity.action_id.get_or_insert_with(|| action_id.clone());
            }
        }
        _ => {}
    }
    identity
}

fn apply_governance_scope_identity(identity: &mut TraceIdentityContext, scope: &GovernanceScope) {
    match scope {
        GovernanceScope::Global => {}
        GovernanceScope::Fleet { fleet_id } => {
            identity.fleet_id.get_or_insert_with(|| fleet_id.clone());
        }
        GovernanceScope::Node { node_id } => {
            identity.node_id.get_or_insert_with(|| node_id.clone());
        }
        GovernanceScope::Instance { instance_id } => {
            identity
                .instance_id
                .get_or_insert_with(|| instance_id.clone());
        }
        GovernanceScope::Tenant { tenant_id } => {
            identity.tenant_id.get_or_insert_with(|| tenant_id.clone());
        }
        GovernanceScope::Agent {
            tenant_id,
            agent_id,
        }
        | GovernanceScope::Run {
            tenant_id,
            agent_id,
            ..
        } => {
            identity.tenant_id.get_or_insert_with(|| tenant_id.clone());
            identity.agent_id.get_or_insert_with(|| agent_id.clone());
        }
        GovernanceScope::Action {
            tenant_id,
            agent_id,
            action_id,
            ..
        } => {
            identity.tenant_id.get_or_insert_with(|| tenant_id.clone());
            identity.agent_id.get_or_insert_with(|| agent_id.clone());
            identity.action_id.get_or_insert_with(|| action_id.clone());
        }
        GovernanceScope::Adapter { tenant_id, .. } => {
            if let Some(tenant_id) = tenant_id {
                identity.tenant_id.get_or_insert_with(|| tenant_id.clone());
            }
        }
    }
}

/// Ordered event taxonomy for a kernel tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TraceEventKind {
    /// Marks the start of a run trace stream.
    RunStarted,
    /// Records accepted work-order authority for a run without exposing secrets.
    WorkOrderAccepted {
        /// Work-order identity that authorized the run boundary.
        work_order_id: WorkOrderId,
        /// Tenant authorized by the work order.
        tenant_id: TenantId,
        /// Agent authorized by the work order.
        agent_id: crate::AgentId,
        /// Run bound by the work order when present.
        run_id: Option<RunId>,
    },
    /// Records fail-closed work-order ingestion rejection for management audit.
    WorkOrderRejected {
        /// Work-order identity when parseable; never contains signature material.
        work_order_id: Option<WorkOrderId>,
        /// Tenant identity when parseable.
        tenant_id: Option<TenantId>,
        /// Agent identity when parseable.
        agent_id: Option<crate::AgentId>,
        /// Run binding when known.
        run_id: Option<RunId>,
        /// Sanitized rejection reason code.
        reason: String,
    },
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
    /// Records an action paused by the approval verifier before adapter execution.
    ActionNeedsApproval {
        /// Action that requires approval.
        action: Action,
        /// Verification result explaining why approval is required.
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
    /// Records creation of a scoped approval request.
    ApprovalRequested {
        /// Approval request scope and reason.
        approval: ApprovalTraceContext,
    },
    /// Records a scoped approval grant being presented to the verifier.
    ApprovalGranted {
        /// Approval grant scope and expiry.
        approval: ApprovalTraceContext,
    },
    /// Records an approval denial or wrong-scope approval rejection.
    ApprovalDenied {
        /// Approval evidence scope.
        approval: ApprovalTraceContext,
        /// Denial or validation reason.
        reason: String,
    },
    /// Records an expired approval evidence rejection.
    ApprovalExpired {
        /// Approval evidence scope.
        approval: ApprovalTraceContext,
        /// Expiry reason.
        reason: String,
    },
    /// Records a revoked approval evidence rejection.
    ApprovalRevoked {
        /// Approval evidence scope.
        approval: ApprovalTraceContext,
        /// Revocation reason.
        reason: String,
    },
    /// Records an action that requires intervention before continuing.
    ActionNeedsIntervention {
        /// Action that requires intervention.
        action: Action,
        /// Verification or escalation result explaining the intervention need.
        result: VerificationResult,
    },
    /// Records a deterministic escalation decision and its evidence.
    EscalationTriggered {
        /// Escalation trigger, threshold, scope, references, and decision.
        escalation: EscalationContext,
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
    /// Records source-side export of a state handoff snapshot.
    StateHandoffExported {
        /// State handoff identity, ownership, and snapshot context.
        handoff: StateHandoffTraceContext,
    },
    /// Records receiver-side import of a validated handoff snapshot.
    StateHandoffImported {
        /// State handoff identity, ownership, and snapshot context.
        handoff: StateHandoffTraceContext,
    },
    /// Records receiver-side failure before a handoff snapshot changed state head.
    StateHandoffImportFailed {
        /// State handoff identity, ownership, and snapshot context.
        handoff: StateHandoffTraceContext,
        /// Fail-closed rejection reason.
        reason: String,
    },
    /// Records attachment of a read-only state reference.
    ReadOnlyStateReferenced {
        /// Read-only reference identity and source context.
        handoff: StateHandoffTraceContext,
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
    /// Records a remote message leaving the source instance transport boundary.
    RemoteMessageSent {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
    },
    /// Records a remote message accepted by the destination transport boundary
    /// after envelope/work-order validation.
    RemoteMessageAccepted {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
    },
    /// Records a remote message rejected before local delivery.
    RemoteMessageRejected {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
        /// Fail-closed rejection reason.
        reason: String,
    },
    /// Records a remote message delivered into the target local inbox boundary.
    RemoteMessageDelivered {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
    },
    /// Records a remote message transport timeout.
    RemoteMessageTimedOut {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
        /// Timeout reason or duration description.
        reason: String,
    },
    /// Records a deterministic duplicate detection outcome.
    RemoteMessageDuplicate {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
        /// Duplicate handling reason.
        reason: String,
    },
    /// Records a non-timeout remote transport failure.
    RemoteMessageTransportFailed {
        /// Remote identity, authority, and message causality context.
        remote_message: RemoteMessageTraceContext,
        /// Transport failure reason.
        reason: String,
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
    /// Records creation of an approval request governance state.
    GovernanceApprovalRequested {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records an approval grant governance state.
    GovernanceApprovalGranted {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records an approval denial governance state.
    GovernanceApprovalDenied {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit approval expiry.
    GovernanceApprovalExpired {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit approval revocation.
    GovernanceApprovalRevoked {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records creation of an open escalation governance state.
    EscalationOpened {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records escalation resolution.
    EscalationResolved {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit escalation expiry.
    EscalationExpired {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit escalation revocation.
    EscalationRevoked {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records an intervention request governance state.
    InterventionRequested {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records intervention resolution.
    InterventionResolved {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records intervention cancellation.
    InterventionCancelled {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit intervention expiry.
    InterventionExpired {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit intervention revocation.
    InterventionRevoked {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records a circuit breaker being tripped/activated in state.
    CircuitBreakerTripped {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records a circuit breaker being cleared in state.
    CircuitBreakerCleared {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit circuit-breaker expiry.
    CircuitBreakerExpired {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit circuit-breaker revocation.
    CircuitBreakerRevoked {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records a kill switch being activated in state.
    KillSwitchActivated {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records a kill switch being cleared in state.
    KillSwitchCleared {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit kill-switch expiry.
    KillSwitchExpired {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records explicit kill-switch revocation.
    KillSwitchRevoked {
        /// Trace-ready governance transition context.
        transition: GovernanceTransition,
    },
    /// Records fail-closed rejection of an invalid governance state transition.
    GovernanceTransitionRejected {
        /// Rejection context with object identity, scope, attempted state, and reason.
        rejection: GovernanceTransitionRejection,
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
