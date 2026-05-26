//! # Local Delegation Model
//!
//! Local-only parent/child run delegation for Sprint 0.02-S4. The manager keeps
//! delegation explicit: parent runs name the target agent, child objective, and
//! delegated authority. Child agent contexts returned by this module are scoped
//! so the loop engine denies actions outside the delegated authority before any
//! adapter can execute.

use crate::{
    AgentContext, LocalMessageRouter, MessageRouter, MessageRouterError, MessageTraceRecorder,
};
use splendor_types::{
    AgentId, DelegatedAuthority, LocalDelegationTraceContext, Message, MessageEnvelope, MessageId,
    MessageTraceContext, MessageValidationError, RunId, TaskFailure, TaskRequest, TaskResponse,
    TaskResponseStatus, TenantId, TraceEvent, TraceEventKind, TraceId, TASK_REQUEST_SCHEMA,
    TASK_RESPONSE_SCHEMA,
};
use std::collections::HashMap;
use std::sync::Mutex;
use time::OffsetDateTime;

/// Lifecycle status for local parent/child runs known to the delegation manager.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRunStatus {
    /// Run is active and may create/complete delegated work.
    Running,
    /// Run completed successfully.
    Completed,
    /// Run failed with a structured child failure.
    Failed,
    /// Run was cancelled and cannot create child runs.
    Cancelled,
    /// Delegation was denied before a child run started.
    Denied,
}

impl LocalRunStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Denied
        )
    }
}

/// Registered agent boundary used for local delegation admission checks.
#[derive(Clone, Debug)]
pub struct LocalAgentRegistration {
    /// Agent runtime context.
    pub agent: AgentContext,
    /// Maximum local authority this agent may exercise when delegated to.
    pub authority: DelegatedAuthority,
}

/// Parent or child run metadata tracked by the local delegation manager.
#[derive(Clone, Debug)]
pub struct LocalRunRecord {
    /// Run identity.
    pub run_id: RunId,
    /// Agent that owns this run.
    pub agent_id: AgentId,
    /// Tenant scope for this run.
    pub tenant_id: TenantId,
    /// Parent run for child records.
    pub parent_run_id: Option<RunId>,
    /// Child runs created by this run.
    pub child_run_ids: Vec<RunId>,
    /// Explicit authority in effect for this run.
    pub authority: DelegatedAuthority,
    /// Scoped objective for child runs.
    pub objective: Option<String>,
    /// Parent trace event that recorded the delegation request.
    pub parent_trace_id: Option<TraceId>,
    /// Current local lifecycle status.
    pub status: LocalRunStatus,
    /// Request message that created this child run.
    pub request_message_id: Option<MessageId>,
    /// Response message sent when this child run completed or failed.
    pub response_message_id: Option<MessageId>,
}

/// Request to create a local child run.
#[derive(Clone, Debug)]
pub struct LocalDelegationRequest {
    /// Parent run requesting delegated work.
    pub parent_run_id: RunId,
    /// Child run to create.
    pub child_run_id: RunId,
    /// Parent/orchestrator agent.
    pub source_agent_id: AgentId,
    /// Child/specialist agent.
    pub target_agent_id: AgentId,
    /// Scoped objective for the child run.
    pub objective: String,
    /// Explicit authority granted to the child run.
    pub delegated_authority: DelegatedAuthority,
    /// Optional parent trace event that caused this delegation.
    pub parent_causal_trace_id: Option<TraceId>,
}

impl LocalDelegationRequest {
    /// Builds a local child-run request with a new child run ID.
    pub fn new(
        parent_run_id: RunId,
        source_agent_id: AgentId,
        target_agent_id: AgentId,
        objective: impl Into<String>,
        delegated_authority: DelegatedAuthority,
        parent_causal_trace_id: Option<TraceId>,
    ) -> Self {
        Self {
            parent_run_id,
            child_run_id: RunId::new(),
            source_agent_id,
            target_agent_id,
            objective: objective.into(),
            delegated_authority,
            parent_causal_trace_id,
        }
    }

    fn trace_context(&self) -> LocalDelegationTraceContext {
        LocalDelegationTraceContext {
            parent_run_id: self.parent_run_id.clone(),
            child_run_id: self.child_run_id.clone(),
            parent_trace_id: self.parent_causal_trace_id.clone(),
            request_message_id: None,
            response_message_id: None,
            source_agent_id: self.source_agent_id.clone(),
            target_agent_id: self.target_agent_id.clone(),
            objective: self.objective.clone(),
        }
    }
}

/// Result of creating a local child run.
#[derive(Clone, Debug)]
pub struct LocalChildRun {
    /// Child run metadata.
    pub run: LocalRunRecord,
    /// Scoped child agent context to pass to the child loop engine.
    pub child_agent: AgentContext,
    /// Task request message routed from parent to child.
    pub request_message: MessageEnvelope,
    /// Child-run start trace ID.
    pub child_started_trace_id: TraceId,
}

/// Result of completing or failing a child run.
#[derive(Clone, Debug)]
pub struct LocalTaskResponse {
    /// Structured task response payload.
    pub response: TaskResponse,
    /// Routed response message.
    pub response_message: MessageEnvelope,
    /// Parent trace ID that references the child completion/failure.
    pub parent_trace_id: TraceId,
    /// Child trace ID that recorded completion/failure.
    pub child_trace_id: TraceId,
}

/// Errors returned by local delegation operations.
#[derive(Debug, thiserror::Error)]
pub enum LocalDelegationError {
    /// Delegation state mutex was poisoned.
    #[error("local delegation storage is unavailable")]
    StorageUnavailable,
    /// Parent run was not registered.
    #[error("parent run {0} is not registered")]
    UnknownParentRun(RunId),
    /// Child run was not registered.
    #[error("child run {0} is not registered")]
    UnknownChildRun(RunId),
    /// Child run ID was already registered.
    #[error("child run {0} is already registered")]
    DuplicateChildRun(RunId),
    /// Child run was already completed, failed, denied, or cancelled.
    #[error("child run {child_run_id} is already finished with status {status:?}")]
    ChildRunAlreadyFinished {
        /// Child run that already reached a terminal status.
        child_run_id: RunId,
        /// Current terminal status.
        status: LocalRunStatus,
    },
    /// Agent was not registered.
    #[error("agent {0} is not registered for local delegation")]
    UnknownAgent(AgentId),
    /// Recorder is scoped to a different run than the delegation event.
    #[error(
        "trace recorder run {runtime_run_id} cannot record delegation run {delegation_run_id}"
    )]
    TraceRunMismatch {
        /// Run associated with the trace recorder.
        runtime_run_id: RunId,
        /// Run associated with the delegation operation.
        delegation_run_id: RunId,
    },
    /// Parent run cannot create child work while cancelled.
    #[error("parent run {0} is cancelled")]
    ParentCancelled(RunId),
    /// Request source does not match the parent run owner.
    #[error("delegation source agent does not own parent run")]
    SourceAgentMismatch,
    /// Target agent belongs to a different tenant.
    #[error("target agent tenant does not match parent run tenant")]
    TenantMismatch,
    /// Delegated authority exceeds parent or target scope.
    #[error("delegated authority exceeds {scope} scope")]
    DelegatedAuthorityDenied {
        /// Scope that denied the delegation.
        scope: &'static str,
    },
    /// Message schema validation failed.
    #[error("message validation failed: {0}")]
    Message(#[from] MessageValidationError),
    /// Message routing failed.
    #[error("message router failed: {0}")]
    Router(#[from] MessageRouterError),
}

/// Local-only delegation manager for parent/child run admission and trace links.
#[derive(Debug)]
pub struct LocalDelegationManager {
    router: LocalMessageRouter,
    lifecycle: Mutex<()>,
    state: Mutex<LocalDelegationState>,
}

#[derive(Debug, Default)]
struct LocalDelegationState {
    agents: HashMap<AgentId, LocalAgentRegistration>,
    runs: HashMap<RunId, LocalRunRecord>,
}

impl LocalDelegationManager {
    /// Creates a manager backed by an in-memory local router.
    pub fn new() -> Self {
        Self {
            router: LocalMessageRouter::new(),
            lifecycle: Mutex::new(()),
            state: Mutex::new(LocalDelegationState::default()),
        }
    }

    /// Returns the local router used for task request/response messages.
    pub fn router(&self) -> &LocalMessageRouter {
        &self.router
    }

    /// Registers an agent and its maximum local delegation authority.
    pub fn register_agent(
        &self,
        agent: AgentContext,
        authority: DelegatedAuthority,
    ) -> Result<(), LocalDelegationError> {
        self.router.register_agent_context(&agent)?;
        let mut state = self.lock_state()?;
        state.agents.insert(
            agent.agent_id.clone(),
            LocalAgentRegistration { agent, authority },
        );
        Ok(())
    }

    /// Registers an active root/parent run.
    pub fn register_root_run(
        &self,
        run_id: RunId,
        agent_id: AgentId,
    ) -> Result<LocalRunRecord, LocalDelegationError> {
        let mut state = self.lock_state()?;
        let agent = state
            .agents
            .get(&agent_id)
            .ok_or_else(|| LocalDelegationError::UnknownAgent(agent_id.clone()))?;
        let record = LocalRunRecord {
            run_id: run_id.clone(),
            agent_id: agent.agent.agent_id.clone(),
            tenant_id: agent.agent.tenant_id.clone(),
            parent_run_id: None,
            child_run_ids: Vec::new(),
            authority: agent.authority.clone(),
            objective: None,
            parent_trace_id: None,
            status: LocalRunStatus::Running,
            request_message_id: None,
            response_message_id: None,
        };
        state.runs.insert(run_id, record.clone());
        Ok(record)
    }

    /// Creates a child run from an explicit target, objective, and delegated scope.
    pub fn create_child_run(
        &self,
        parent_recorder: &dyn MessageTraceRecorder,
        child_recorder: &dyn MessageTraceRecorder,
        request: LocalDelegationRequest,
    ) -> Result<LocalChildRun, LocalDelegationError> {
        let _lifecycle = self.lock_lifecycle()?;
        ensure_recorder_run(parent_recorder, &request.parent_run_id)?;
        ensure_recorder_run(child_recorder, &request.child_run_id)?;
        let mut trace_context = request.trace_context();

        let (parent_run, target_agent, duplicate_child_run) = {
            let state = self.lock_state()?;
            let parent_run = state
                .runs
                .get(&request.parent_run_id)
                .cloned()
                .ok_or_else(|| {
                    LocalDelegationError::UnknownParentRun(request.parent_run_id.clone())
                })?;
            let target_agent = state
                .agents
                .get(&request.target_agent_id)
                .cloned()
                .ok_or_else(|| {
                    LocalDelegationError::UnknownAgent(request.target_agent_id.clone())
                })?;
            let duplicate_child_run = state.runs.contains_key(&request.child_run_id);
            (parent_run, target_agent, duplicate_child_run)
        };

        if parent_run.status == LocalRunStatus::Cancelled {
            let reason = "parent_run_cancelled".to_string();
            parent_recorder.record_message_event(TraceEventKind::DelegationRejected {
                delegation: trace_context,
                reason,
            })?;
            return Err(LocalDelegationError::ParentCancelled(request.parent_run_id));
        }
        if duplicate_child_run {
            parent_recorder.record_message_event(TraceEventKind::DelegationRejected {
                delegation: trace_context,
                reason: "duplicate_child_run_id".to_string(),
            })?;
            return Err(LocalDelegationError::DuplicateChildRun(
                request.child_run_id,
            ));
        }
        if parent_run.agent_id != request.source_agent_id {
            return Err(LocalDelegationError::SourceAgentMismatch);
        }
        if parent_run.tenant_id != target_agent.agent.tenant_id {
            return Err(LocalDelegationError::TenantMismatch);
        }
        if !request
            .delegated_authority
            .is_subset_of(&parent_run.authority)
        {
            parent_recorder.record_message_event(TraceEventKind::DelegationRejected {
                delegation: trace_context,
                reason: "delegated_authority_exceeds_parent_scope".to_string(),
            })?;
            return Err(LocalDelegationError::DelegatedAuthorityDenied { scope: "parent" });
        }
        if !request
            .delegated_authority
            .is_subset_of(&target_agent.authority)
        {
            parent_recorder.record_message_event(TraceEventKind::DelegationRejected {
                delegation: trace_context,
                reason: "delegated_authority_exceeds_target_scope".to_string(),
            })?;
            return Err(LocalDelegationError::DelegatedAuthorityDenied { scope: "target" });
        }

        let requested_trace =
            parent_recorder.record_message_event(TraceEventKind::DelegationRequested {
                delegation: trace_context.clone(),
            })?;
        trace_context = trace_context.with_parent_trace(requested_trace.clone());

        let task_request = TaskRequest::new(
            request.parent_run_id.clone(),
            request.child_run_id.clone(),
            request.target_agent_id.clone(),
            request.objective.clone(),
            request.delegated_authority.clone(),
        )?;
        let request_message = Message::new(
            MessageId::new(),
            request.source_agent_id.clone(),
            request.target_agent_id.clone(),
            request.parent_run_id.clone(),
            TASK_REQUEST_SCHEMA,
            serde_json::to_value(task_request).map_err(|error| {
                MessageValidationError::PayloadValidationFailed {
                    schema: TASK_REQUEST_SCHEMA.to_string(),
                    reason: error.to_string(),
                }
            })?,
            Some(requested_trace.clone()),
            true,
            OffsetDateTime::now_utc(),
        )?;
        let request_message_id = request_message.message_id.clone();
        let request_envelope = MessageEnvelope::new(request_message)?;
        let routed_request = self.router.send(parent_recorder, request_envelope)?;
        trace_context = trace_context.with_request_message(request_message_id.clone());

        let child_started_trace =
            child_recorder.record_message_event(TraceEventKind::ChildRunStarted {
                delegation: trace_context.clone(),
            })?;

        let mut child_agent = target_agent.agent.clone();
        child_agent.set_delegated_authority(request.delegated_authority.clone());
        let child_record = LocalRunRecord {
            run_id: request.child_run_id.clone(),
            agent_id: request.target_agent_id.clone(),
            tenant_id: parent_run.tenant_id.clone(),
            parent_run_id: Some(request.parent_run_id.clone()),
            child_run_ids: Vec::new(),
            authority: request.delegated_authority,
            objective: Some(request.objective),
            parent_trace_id: Some(requested_trace),
            status: LocalRunStatus::Running,
            request_message_id: Some(request_message_id),
            response_message_id: None,
        };

        let mut state = self.lock_state()?;
        state
            .runs
            .get_mut(&request.parent_run_id)
            .ok_or_else(|| LocalDelegationError::UnknownParentRun(request.parent_run_id.clone()))?
            .child_run_ids
            .push(request.child_run_id.clone());
        state
            .runs
            .insert(request.child_run_id, child_record.clone());

        Ok(LocalChildRun {
            run: child_record,
            child_agent,
            request_message: routed_request,
            child_started_trace_id: child_started_trace,
        })
    }

    /// Completes a child run and sends a structured task response to the parent.
    pub fn complete_child_run(
        &self,
        parent_recorder: &dyn MessageTraceRecorder,
        child_recorder: &dyn MessageTraceRecorder,
        child_run_id: &RunId,
        output: serde_json::Value,
    ) -> Result<LocalTaskResponse, LocalDelegationError> {
        self.finish_child_run(
            parent_recorder,
            child_recorder,
            child_run_id,
            TaskResponseStatus::Completed,
            Some(output),
            None,
        )
    }

    /// Fails a child run and sends a structured task response to the parent.
    pub fn fail_child_run(
        &self,
        parent_recorder: &dyn MessageTraceRecorder,
        child_recorder: &dyn MessageTraceRecorder,
        child_run_id: &RunId,
        failure: TaskFailure,
    ) -> Result<LocalTaskResponse, LocalDelegationError> {
        self.finish_child_run(
            parent_recorder,
            child_recorder,
            child_run_id,
            TaskResponseStatus::Failed,
            None,
            Some(failure),
        )
    }

    /// Cancels a parent run and records the cancellation trace event.
    pub fn cancel_parent_run(
        &self,
        parent_recorder: &dyn MessageTraceRecorder,
        parent_run_id: &RunId,
        reason: impl Into<String>,
    ) -> Result<TraceId, LocalDelegationError> {
        let _lifecycle = self.lock_lifecycle()?;
        ensure_recorder_run(parent_recorder, parent_run_id)?;
        let reason = reason.into();
        let mut state = self.lock_state()?;
        let parent = state
            .runs
            .get_mut(parent_run_id)
            .ok_or_else(|| LocalDelegationError::UnknownParentRun(parent_run_id.clone()))?;
        parent.status = LocalRunStatus::Cancelled;
        let agent_id = parent.agent_id.clone();
        drop(state);
        Ok(
            parent_recorder.record_message_event(TraceEventKind::ParentRunCancelled {
                parent_run_id: parent_run_id.clone(),
                agent_id,
                reason,
            })?,
        )
    }

    /// Returns a run record snapshot.
    pub fn run(&self, run_id: &RunId) -> Result<LocalRunRecord, LocalDelegationError> {
        self.lock_state()?
            .runs
            .get(run_id)
            .cloned()
            .ok_or_else(|| LocalDelegationError::UnknownChildRun(run_id.clone()))
    }

    fn finish_child_run(
        &self,
        parent_recorder: &dyn MessageTraceRecorder,
        child_recorder: &dyn MessageTraceRecorder,
        child_run_id: &RunId,
        status: TaskResponseStatus,
        output: Option<serde_json::Value>,
        failure: Option<TaskFailure>,
    ) -> Result<LocalTaskResponse, LocalDelegationError> {
        let _lifecycle = self.lock_lifecycle()?;
        ensure_recorder_run(child_recorder, child_run_id)?;
        let (child, parent) =
            {
                let state = self.lock_state()?;
                let child =
                    state.runs.get(child_run_id).cloned().ok_or_else(|| {
                        LocalDelegationError::UnknownChildRun(child_run_id.clone())
                    })?;
                if child.status.is_terminal() || child.response_message_id.is_some() {
                    return Err(LocalDelegationError::ChildRunAlreadyFinished {
                        child_run_id: child_run_id.clone(),
                        status: child.status,
                    });
                }
                let parent_run_id = child
                    .parent_run_id
                    .clone()
                    .ok_or_else(|| LocalDelegationError::UnknownParentRun(child_run_id.clone()))?;
                let parent =
                    state.runs.get(&parent_run_id).cloned().ok_or_else(|| {
                        LocalDelegationError::UnknownParentRun(parent_run_id.clone())
                    })?;
                (child, parent)
            };
        ensure_recorder_run(parent_recorder, &parent.run_id)?;
        let mut context = LocalDelegationTraceContext {
            parent_run_id: parent.run_id.clone(),
            child_run_id: child.run_id.clone(),
            parent_trace_id: None,
            request_message_id: child.request_message_id.clone(),
            response_message_id: None,
            source_agent_id: parent.agent_id.clone(),
            target_agent_id: child.agent_id.clone(),
            objective: child.objective.clone().unwrap_or_default(),
        };
        context.parent_trace_id = child.parent_trace_id.clone();

        let child_trace_id = match status {
            TaskResponseStatus::Completed => {
                child_recorder.record_message_event(TraceEventKind::ChildRunCompleted {
                    delegation: context.clone(),
                })?
            }
            TaskResponseStatus::Failed
            | TaskResponseStatus::Denied
            | TaskResponseStatus::Cancelled => {
                let failure_for_trace = failure
                    .clone()
                    .unwrap_or_else(|| TaskFailure::new("child_failed", "child run failed", false));
                child_recorder.record_message_event(TraceEventKind::ChildRunFailed {
                    delegation: context.clone(),
                    failure: failure_for_trace,
                })?
            }
        };

        let failure = failure.map(|failure| failure.with_trace_id(child_trace_id.clone()));
        let response = TaskResponse::new(
            parent.run_id.clone(),
            child.run_id.clone(),
            status,
            output,
            failure.clone(),
        )?;
        let response_message = Message::new(
            MessageId::new(),
            child.agent_id.clone(),
            parent.agent_id.clone(),
            parent.run_id.clone(),
            TASK_RESPONSE_SCHEMA,
            serde_json::to_value(response.clone()).map_err(|error| {
                MessageValidationError::PayloadValidationFailed {
                    schema: TASK_RESPONSE_SCHEMA.to_string(),
                    reason: error.to_string(),
                }
            })?,
            Some(child_trace_id.clone()),
            false,
            OffsetDateTime::now_utc(),
        )?;
        let response_message_id = response_message.message_id.clone();
        let routed_response = self
            .router
            .send(parent_recorder, MessageEnvelope::new(response_message)?)?;
        context = context.with_response_message(response_message_id.clone());

        let parent_trace_id = match status {
            TaskResponseStatus::Completed => {
                parent_recorder.record_message_event(TraceEventKind::ChildRunCompleted {
                    delegation: context,
                })?
            }
            TaskResponseStatus::Failed
            | TaskResponseStatus::Denied
            | TaskResponseStatus::Cancelled => {
                parent_recorder.record_message_event(TraceEventKind::ChildRunFailed {
                    delegation: context,
                    failure: failure.clone().unwrap_or_else(|| {
                        TaskFailure::new("child_failed", "child run failed", false)
                    }),
                })?
            }
        };

        let mut state = self.lock_state()?;
        if let Some(child_record) = state.runs.get_mut(child_run_id) {
            child_record.status = match status {
                TaskResponseStatus::Completed => LocalRunStatus::Completed,
                TaskResponseStatus::Failed => LocalRunStatus::Failed,
                TaskResponseStatus::Denied => LocalRunStatus::Denied,
                TaskResponseStatus::Cancelled => LocalRunStatus::Cancelled,
            };
            child_record.response_message_id = Some(response_message_id);
        }

        Ok(LocalTaskResponse {
            response,
            response_message: routed_response,
            parent_trace_id,
            child_trace_id,
        })
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, LocalDelegationState>, LocalDelegationError> {
        self.state
            .lock()
            .map_err(|_| LocalDelegationError::StorageUnavailable)
    }

    fn lock_lifecycle(&self) -> Result<std::sync::MutexGuard<'_, ()>, LocalDelegationError> {
        self.lifecycle
            .lock()
            .map_err(|_| LocalDelegationError::StorageUnavailable)
    }
}

impl Default for LocalDelegationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Replay summary for local delegation relationships and task messages.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalDelegationReplay {
    /// Parent/child edges reconstructed from delegation trace events.
    pub delegations: Vec<LocalDelegationTraceContext>,
    /// Task request/response message contexts seen during replay.
    pub messages: Vec<MessageTraceContext>,
    /// Structured child failures seen during replay.
    pub failures: Vec<TaskFailure>,
}

/// Reconstructs parent/child causal relationships and task message exchange from
/// trace events without executing policies, adapters, or child runs.
pub fn replay_local_delegations(events: &[TraceEvent]) -> LocalDelegationReplay {
    let mut replay = LocalDelegationReplay::default();
    let mut seen_delegations: Vec<(RunId, RunId)> = Vec::new();
    let mut seen_messages = Vec::new();
    for event in events {
        match &event.kind {
            TraceEventKind::DelegationRequested { delegation }
            | TraceEventKind::ChildRunStarted { delegation }
            | TraceEventKind::ChildRunCompleted { delegation } => {
                let key = (
                    delegation.parent_run_id.clone(),
                    delegation.child_run_id.clone(),
                );
                if !seen_delegations.contains(&key) {
                    seen_delegations.push(key);
                    replay.delegations.push(delegation.clone());
                }
            }
            TraceEventKind::DelegationRejected { delegation, .. } => {
                let key = (
                    delegation.parent_run_id.clone(),
                    delegation.child_run_id.clone(),
                );
                if !seen_delegations.contains(&key) {
                    seen_delegations.push(key);
                    replay.delegations.push(delegation.clone());
                }
            }
            TraceEventKind::ChildRunFailed {
                delegation,
                failure,
            } => {
                let key = (
                    delegation.parent_run_id.clone(),
                    delegation.child_run_id.clone(),
                );
                if !seen_delegations.contains(&key) {
                    seen_delegations.push(key);
                    replay.delegations.push(delegation.clone());
                }
                replay.failures.push(failure.clone());
            }
            TraceEventKind::MessageQueued { message }
            | TraceEventKind::MessageDelivered { message }
            | TraceEventKind::MessageConsumed { message }
                if (message.schema == TASK_REQUEST_SCHEMA
                    || message.schema == TASK_RESPONSE_SCHEMA)
                    && !seen_messages.contains(&message.message_id) =>
            {
                seen_messages.push(message.message_id.clone());
                replay.messages.push(message.clone());
            }
            _ => {}
        }
    }
    replay
}

fn ensure_recorder_run(
    recorder: &dyn MessageTraceRecorder,
    run_id: &RunId,
) -> Result<(), LocalDelegationError> {
    if recorder.run_id() != run_id {
        return Err(LocalDelegationError::TraceRunMismatch {
            runtime_run_id: recorder.run_id().clone(),
            delegation_run_id: run_id.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/local_delegation_tests.rs"]
mod tests;
