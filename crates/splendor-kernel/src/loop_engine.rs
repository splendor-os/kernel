//! # Loop Engine
//!
//! The loop engine executes a single agent tick: collect percepts, invoke the
//! policy, evaluate constraints, verify/execute actions, record outcomes, and
//! commit state. It emits the ordered trace events required for auditability.

use crate::{
    apply_escalation_to_outcome, escalations_require_intervention, AgentContext,
    EscalationEvaluator, EscalationOutcomeInput, KernelRuntime, KernelRuntimeConfig, StateCommit,
    StateGraph, StateGraphError,
};
use splendor_gateway::{
    ActionGateway, ActionId, ActionOutcome, ActionRequest, ActionStatus, GatewayError,
};
use splendor_store::{StateData, StateMetadata, TraceStore, TraceStoreError};
use splendor_types::{
    Action, ApprovalTraceContext, Constraint, ContentHash, EscalationContext, EscalationPolicy,
    Feedback, Percept, QuotaUsage, Reward, RunId, SnapshotId, TickId, TraceEvent, TraceEventId,
    TraceEventKind, TraceIdentityContext, VerificationResult, WorkOrder,
};
use std::sync::Arc;
use std::time::Instant;
use time::OffsetDateTime;

/// Collects percepts for a tick.
pub trait Perceptor: Send + Sync {
    /// Collects percepts for the provided agent context.
    fn collect(&self, agent: &AgentContext) -> Result<Vec<Percept>, LoopError>;
}

/// Policy callback that proposes actions and next state.
pub trait Policy: Send + Sync {
    /// Returns a human-readable policy identifier.
    fn name(&self) -> &str;
    /// Produces the next decision based on the current state and percepts.
    fn decide(&self, state: &StateData, percepts: &[Percept]) -> Result<PolicyDecision, LoopError>;
}

/// Constraint engine result bundle.
#[derive(Clone, Debug)]
pub struct ConstraintEvaluation {
    /// Constraints evaluated during the tick.
    pub constraints: Vec<Constraint>,
    /// Verification outcome for the constraint set.
    pub result: VerificationResult,
}

impl ConstraintEvaluation {
    /// Returns an allow-all evaluation.
    pub fn allow() -> Self {
        Self {
            constraints: Vec::new(),
            result: VerificationResult::allow(),
        }
    }
}

/// Constraint evaluation hook used by the loop engine.
pub trait ConstraintEngine: Send + Sync {
    /// Evaluates constraints for the tick.
    fn evaluate(
        &self,
        state: &StateData,
        percepts: &[Percept],
        actions: &[ActionCandidate],
    ) -> ConstraintEvaluation;
}

/// Constraint engine that always allows execution.
#[derive(Clone, Debug, Default)]
pub struct AllowAllConstraintEngine;

impl ConstraintEngine for AllowAllConstraintEngine {
    fn evaluate(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
        _actions: &[ActionCandidate],
    ) -> ConstraintEvaluation {
        ConstraintEvaluation::allow()
    }
}

/// Optional feedback and reward signals from outcomes.
#[derive(Clone, Debug, Default)]
pub struct OutcomeSignal {
    /// Feedback signal produced by the evaluator.
    pub feedback: Option<Feedback>,
    /// Reward signal produced by the evaluator.
    pub reward: Option<Reward>,
}

/// Evaluates action outcomes into feedback/reward signals.
pub trait OutcomeEvaluator: Send + Sync {
    /// Evaluates the outcome for a single action.
    fn evaluate(&self, action: &Action, outcome: &ActionOutcome) -> OutcomeSignal;
}

/// Outcome evaluator that emits no signals.
#[derive(Clone, Debug, Default)]
pub struct NoopOutcomeEvaluator;

impl OutcomeEvaluator for NoopOutcomeEvaluator {
    fn evaluate(&self, _action: &Action, _outcome: &ActionOutcome) -> OutcomeSignal {
        OutcomeSignal::default()
    }
}

/// Proposed action with quota usage and adapter metadata.
#[derive(Clone, Debug)]
pub struct ActionCandidate {
    /// Action to be executed.
    pub action: Action,
    /// Adapter identifier used for policy allowlists.
    pub adapter: Option<String>,
    /// Resource usage estimate for quota enforcement.
    pub usage: QuotaUsage,
    /// Preconditions satisfied for this action.
    pub satisfied_preconditions: Vec<String>,
    /// Optional approval evidence supplied for this action evaluation.
    pub approval_evidence: Option<splendor_types::ApprovalEvidence>,
}

impl ActionCandidate {
    /// Creates a candidate with default usage and no adapter.
    pub fn new(action: Action) -> Self {
        let satisfied_preconditions = action.preconditions.clone();
        Self {
            action,
            adapter: None,
            usage: QuotaUsage::single_action(),
            satisfied_preconditions,
            approval_evidence: None,
        }
    }

    /// Sets the adapter identifier used for allowlist checks.
    pub fn with_adapter(mut self, adapter: impl Into<String>) -> Self {
        self.adapter = Some(adapter.into());
        self
    }

    /// Sets the quota usage estimate.
    pub fn with_usage(mut self, usage: QuotaUsage) -> Self {
        self.usage = usage;
        self
    }

    /// Overrides the satisfied preconditions for this action.
    pub fn with_satisfied_preconditions(mut self, preconditions: Vec<String>) -> Self {
        self.satisfied_preconditions = preconditions;
        self
    }

    /// Attaches approval evidence for the gateway approval verifier.
    pub fn with_approval_evidence(mut self, evidence: splendor_types::ApprovalEvidence) -> Self {
        self.approval_evidence = Some(evidence);
        self
    }
}

/// Output from a policy decision.
#[derive(Clone, Debug)]
pub struct PolicyDecision {
    /// Actions proposed by the policy.
    pub actions: Vec<ActionCandidate>,
    /// Next state payload to commit.
    pub next_state: StateData,
    /// Metadata for the state commit.
    pub metadata: StateMetadata,
}

/// Resume metadata discovered from a trace store.
#[derive(Clone, Debug)]
pub struct ResumeInfo {
    /// Snapshot identifier to restore from.
    pub snapshot_id: SnapshotId,
    /// Latest completed tick identifier.
    pub tick_id: u64,
}

/// Optional trace/run metadata supplied when constructing a persisted loop
/// engine.
#[derive(Clone, Debug, Default)]
pub struct RunTraceContext {
    /// Run identifier to bind or resume.
    pub run_id: Option<RunId>,
    /// Validated work order that authorized this run, if present.
    pub work_order: Option<WorkOrder>,
}

impl RunTraceContext {
    /// Builds trace metadata for an optional run id without work-order authority.
    pub fn new(run_id: Option<RunId>) -> Self {
        Self {
            run_id,
            work_order: None,
        }
    }

    /// Attaches validated work-order metadata.
    pub fn with_work_order(mut self, work_order: WorkOrder) -> Self {
        self.work_order = Some(work_order);
        self
    }
}

impl PolicyDecision {
    /// Creates a decision with a label applied to the state metadata.
    pub fn new(
        actions: Vec<ActionCandidate>,
        next_state: StateData,
        label: Option<String>,
    ) -> Self {
        Self {
            actions,
            next_state,
            metadata: StateMetadata::new(OffsetDateTime::now_utc(), label),
        }
    }
}

/// Outcome produced by a loop tick.
#[derive(Clone, Debug)]
pub struct TickOutcome {
    /// Tick identifier for this outcome.
    pub tick_id: u64,
    /// Outcomes returned by the action gateway.
    pub action_outcomes: Vec<ActionOutcome>,
    /// State commit recorded for the tick.
    pub state_commit: StateCommit,
    /// Wall-clock duration for the tick in milliseconds.
    pub duration_ms: u64,
    /// Indicates whether the tick requires intervention.
    pub needs_intervention: bool,
    /// Indicates whether the tick paused waiting for approval.
    pub needs_approval: bool,
}

/// Errors raised by the loop engine.
#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    /// Trace emission failed.
    #[error("trace error: {0}")]
    Trace(#[from] crate::TraceError),
    /// State graph commit failed.
    #[error("state graph error: {0}")]
    StateGraph(#[from] StateGraphError),
    /// Trace store access failed.
    #[error("trace store error: {0}")]
    TraceStore(#[from] TraceStoreError),
    /// Trace parsing failed.
    #[error("trace parse error: {0}")]
    TraceParse(#[from] serde_json::Error),
    /// Resume discovery failed.
    #[error("resume error: {0}")]
    Resume(String),
    /// Policy callback failed.
    #[error("policy error: {0}")]
    Policy(String),
    /// Perceptor callback failed.
    #[error("perceptor error: {0}")]
    Perceptor(String),
}

/// Kernel loop engine for a single agent.
pub struct LoopEngine {
    agent: AgentContext,
    runtime: KernelRuntime,
    state_graph: StateGraph,
    state: StateData,
    perceptors: Vec<Box<dyn Perceptor>>,
    policy: Box<dyn Policy>,
    constraint_engine: Box<dyn ConstraintEngine>,
    gateway: Arc<dyn ActionGateway>,
    outcome_evaluator: Box<dyn OutcomeEvaluator>,
    escalation_evaluator: Option<EscalationEvaluator>,
}

impl LoopEngine {
    /// Builds a loop engine with default trace configuration.
    pub fn new(
        agent: AgentContext,
        state_graph: StateGraph,
        state: StateData,
        policy: Box<dyn Policy>,
        gateway: Arc<dyn ActionGateway>,
    ) -> Self {
        Self::with_runtime(
            agent,
            state_graph,
            state,
            policy,
            gateway,
            KernelRuntime::new(KernelRuntimeConfig::default()),
        )
    }

    /// Builds a loop engine with an explicit runtime.
    pub fn with_runtime(
        agent: AgentContext,
        state_graph: StateGraph,
        state: StateData,
        policy: Box<dyn Policy>,
        gateway: Arc<dyn ActionGateway>,
        runtime: KernelRuntime,
    ) -> Self {
        let mut agent = agent;
        if let Some(head) = state_graph.head().cloned() {
            agent.set_state_head(head);
        }
        Self {
            agent,
            runtime,
            state_graph,
            state,
            perceptors: Vec::new(),
            policy,
            constraint_engine: Box::new(AllowAllConstraintEngine),
            gateway,
            outcome_evaluator: Box::new(NoopOutcomeEvaluator),
            escalation_evaluator: None,
        }
    }

    /// Builds a loop engine that records traces in a trace store.
    pub fn with_trace_store(
        agent: AgentContext,
        state_graph: StateGraph,
        state: StateData,
        policy: Box<dyn Policy>,
        gateway: Arc<dyn ActionGateway>,
        trace_store: Arc<dyn TraceStore>,
        run_id: Option<RunId>,
    ) -> Result<Self, LoopError> {
        Self::with_trace_store_and_work_order(
            agent,
            state_graph,
            state,
            policy,
            gateway,
            trace_store,
            RunTraceContext::new(run_id),
        )
    }

    /// Builds a loop engine that records traces in a trace store and attaches
    /// validated work-order metadata to the run trace stream.
    pub fn with_trace_store_and_work_order(
        mut agent: AgentContext,
        state_graph: StateGraph,
        state: StateData,
        policy: Box<dyn Policy>,
        gateway: Arc<dyn ActionGateway>,
        trace_store: Arc<dyn TraceStore>,
        context: RunTraceContext,
    ) -> Result<Self, LoopError> {
        let runtime = KernelRuntime::with_trace_store(trace_store, context.run_id)?;
        if runtime.next_sequence() == 0 {
            runtime.record_event(TraceEventKind::RunStarted)?;
        }
        if let Some(work_order) = context.work_order.as_ref() {
            agent.config.metadata.insert(
                "work_order_id".to_string(),
                work_order.work_order_id.to_string(),
            );
            runtime.record_event(TraceEventKind::WorkOrderAccepted {
                work_order_id: work_order.work_order_id.clone(),
                tenant_id: work_order.tenant_id.clone(),
                agent_id: work_order.agent_id.clone(),
                run_id: work_order.run_id.clone(),
            })?;
        }
        Ok(Self::with_runtime(
            agent,
            state_graph,
            state,
            policy,
            gateway,
            runtime,
        ))
    }

    /// Builds a loop engine by resuming from the most recent snapshot in the trace store.
    pub fn resume_from_trace_store(
        agent: AgentContext,
        state_graph: StateGraph,
        policy: Box<dyn Policy>,
        gateway: Arc<dyn ActionGateway>,
        trace_store: Arc<dyn TraceStore>,
        run_id: RunId,
    ) -> Result<Self, LoopError> {
        Self::resume_from_trace_store_with_work_order(
            agent,
            state_graph,
            policy,
            gateway,
            trace_store,
            run_id,
            None,
        )
    }

    /// Resumes from a trace store after validating a work order at the caller
    /// boundary. The work-order event is appended to the existing trace stream.
    pub fn resume_from_trace_store_with_work_order(
        agent: AgentContext,
        state_graph: StateGraph,
        policy: Box<dyn Policy>,
        gateway: Arc<dyn ActionGateway>,
        trace_store: Arc<dyn TraceStore>,
        run_id: RunId,
        work_order: Option<&WorkOrder>,
    ) -> Result<Self, LoopError> {
        let context = RunTraceContext::new(Some(run_id.clone()));
        let context = match work_order {
            Some(work_order) => context.with_work_order(work_order.clone()),
            None => context,
        };
        let resume = Self::resume_info(trace_store.as_ref(), &run_id)?;
        let mut engine = Self::with_trace_store_and_work_order(
            agent,
            state_graph,
            StateData {
                bytes: Vec::new(),
                content_type: None,
            },
            policy,
            gateway,
            trace_store,
            context,
        )?;
        engine.restore_snapshot(&resume.snapshot_id)?;
        engine.state_graph.set_tick(resume.tick_id);
        Ok(engine)
    }

    /// Returns the agent identifier for this loop.
    pub fn agent_id(&self) -> &splendor_types::AgentId {
        &self.agent.agent_id
    }

    /// Returns the tenant identifier for this loop.
    pub fn tenant_id(&self) -> &splendor_types::TenantId {
        &self.agent.tenant_id
    }

    fn trace_identity(&self, tick_id: u64) -> TraceIdentityContext {
        self.runtime
            .trace_identity()
            .with_tenant_agent(self.agent.tenant_id.clone(), self.agent.agent_id.clone())
            .with_tick_id(TickId::from(tick_id))
    }

    fn record_tick_event(
        &self,
        tick_id: u64,
        kind: TraceEventKind,
    ) -> Result<TraceEvent, LoopError> {
        Ok(self
            .runtime
            .record_event_with_identity(self.trace_identity(tick_id), kind)?)
    }

    fn record_action_event(
        &self,
        tick_id: u64,
        action_id: &ActionId,
        kind: TraceEventKind,
    ) -> Result<TraceEvent, LoopError> {
        Ok(self.runtime.record_event_with_identity(
            self.trace_identity(tick_id)
                .with_action_id(action_id.clone()),
            kind,
        )?)
    }

    /// Restores state from a snapshot and updates the agent head pointer.
    pub fn restore_snapshot(&mut self, snapshot_id: &SnapshotId) -> Result<(), LoopError> {
        let snapshot = self.state_graph.restore_snapshot(snapshot_id)?;
        self.state = snapshot.state;
        self.agent.set_state_head(snapshot.node_id);
        Ok(())
    }

    /// Adds a perceptor to the loop engine.
    pub fn add_perceptor(&mut self, perceptor: impl Perceptor + 'static) {
        self.perceptors.push(Box::new(perceptor));
    }

    /// Replaces the constraint engine.
    pub fn set_constraint_engine(&mut self, engine: impl ConstraintEngine + 'static) {
        self.constraint_engine = Box::new(engine);
    }

    /// Replaces the outcome evaluator.
    pub fn set_outcome_evaluator(&mut self, evaluator: impl OutcomeEvaluator + 'static) {
        self.outcome_evaluator = Box::new(evaluator);
    }

    /// Enables deterministic 0.04-S3 escalation evaluation for this loop. The
    /// evaluator consumes explicit verifier/runtime facts and only emits
    /// escalation trace events when a configured threshold is reached.
    pub fn set_escalation_policy(&mut self, policy: EscalationPolicy) {
        self.escalation_evaluator = Some(EscalationEvaluator::new(policy));
    }

    /// Records a non-tick runtime event through this loop's trace runtime.
    pub fn record_runtime_event(&self, kind: TraceEventKind) -> Result<TraceEvent, LoopError> {
        self.runtime.record_event(kind).map_err(LoopError::Trace)
    }

    /// Executes a single tick of the loop engine.
    pub fn tick(&mut self, tick_id: u64) -> Result<TickOutcome, LoopError> {
        let start = Instant::now();
        self.record_tick_event(tick_id, TraceEventKind::LoopTickStarted { tick_id })?;

        let percepts = self.collect_percepts()?;
        self.record_tick_event(
            tick_id,
            TraceEventKind::PerceptsReceived {
                percepts: percepts.clone(),
            },
        )?;

        self.record_tick_event(
            tick_id,
            TraceEventKind::StateLoaded {
                state_hash: Some(ContentHash::blake3(&self.state.bytes)),
            },
        )?;

        let policy_name = self.policy.name().to_string();
        self.record_tick_event(
            tick_id,
            TraceEventKind::PolicyInvoked {
                policy: policy_name.clone(),
            },
        )?;
        let decision = self.policy.decide(&self.state, &percepts)?;
        self.record_tick_event(
            tick_id,
            TraceEventKind::PolicyCompleted {
                policy: policy_name,
            },
        )?;

        let candidate_actions = decision
            .actions
            .iter()
            .map(|candidate| candidate.action.clone())
            .collect::<Vec<_>>();
        self.record_tick_event(
            tick_id,
            TraceEventKind::CandidatesProposed {
                actions: candidate_actions,
            },
        )?;

        let constraint_evaluation =
            self.constraint_engine
                .evaluate(&self.state, &percepts, &decision.actions);
        self.record_tick_event(
            tick_id,
            TraceEventKind::ConstraintsEvaluated {
                constraints: constraint_evaluation.constraints.clone(),
                result: constraint_evaluation.result.clone(),
            },
        )?;

        let mut outcomes = Vec::new();
        let mut escalations = Vec::new();
        for candidate in &decision.actions {
            let action = candidate.action.clone();
            let action_id = ActionId::new();
            self.record_action_event(
                tick_id,
                &action_id,
                TraceEventKind::ActionVerificationStarted {
                    action: action.clone(),
                },
            )?;

            let delegated_scope = self
                .agent
                .verify_delegated_action(&action, candidate.adapter.as_deref());
            let mut outcome = if !constraint_evaluation.result.allowed {
                ActionOutcome {
                    action_id: action_id.clone(),
                    status: ActionStatus::Denied,
                    verification: constraint_evaluation.result.clone(),
                    post_verification: None,
                    output: None,
                    error: Some(constraint_evaluation.result.reasons.join(", ")),
                    completed_at: OffsetDateTime::now_utc(),
                }
            } else if !delegated_scope.allowed {
                ActionOutcome {
                    action_id: action_id.clone(),
                    status: ActionStatus::Denied,
                    verification: delegated_scope.clone(),
                    post_verification: None,
                    output: None,
                    error: Some(delegated_scope.reasons.join(", ")),
                    completed_at: OffsetDateTime::now_utc(),
                }
            } else {
                let request = ActionRequest {
                    action_id: action_id.clone(),
                    tenant_id: self.agent.tenant_id.clone(),
                    agent_id: self.agent.agent_id.clone(),
                    run_id: self.runtime.run_id().clone(),
                    action: action.clone(),
                    adapter: candidate.adapter.clone(),
                    quota_usage: candidate.usage,
                    satisfied_preconditions: candidate.satisfied_preconditions.clone(),
                    requested_at: OffsetDateTime::now_utc(),
                    approval_evidence: candidate.approval_evidence.clone(),
                };

                match self.gateway.submit(request) {
                    Ok(outcome) => outcome,
                    Err(error) => outcome_from_gateway_error(action_id.clone(), error),
                }
            };

            let action_escalations = self.evaluate_escalations(
                &action_id,
                &action,
                candidate.adapter.as_deref(),
                &mut outcome,
            );

            self.record_action_event(
                tick_id,
                &action_id,
                TraceEventKind::ActionVerificationCompleted {
                    action: action.clone(),
                    result: outcome.verification.clone(),
                },
            )?;

            for escalation in &action_escalations {
                self.record_action_event(
                    tick_id,
                    &action_id,
                    TraceEventKind::EscalationTriggered {
                        escalation: escalation.clone(),
                    },
                )?;
            }

            match outcome.status {
                ActionStatus::Executed => {
                    self.record_approval_event_if_present(tick_id, &action_id, &outcome)?;
                    self.record_action_event(
                        tick_id,
                        &action_id,
                        TraceEventKind::ActionExecuted {
                            action: action.clone(),
                            outcome: outcome.output.clone().unwrap_or(serde_json::Value::Null),
                        },
                    )?;
                }
                ActionStatus::Denied => {
                    self.record_approval_event_if_present(tick_id, &action_id, &outcome)?;
                    self.record_action_event(
                        tick_id,
                        &action_id,
                        TraceEventKind::ActionDenied {
                            action: action.clone(),
                            result: outcome.verification.clone(),
                        },
                    )?;
                }
                ActionStatus::NeedsApproval => {
                    self.record_approval_event_if_present(tick_id, &action_id, &outcome)?;
                    self.record_action_event(
                        tick_id,
                        &action_id,
                        TraceEventKind::ActionNeedsApproval {
                            action: action.clone(),
                            result: outcome.verification.clone(),
                        },
                    )?;
                }
                ActionStatus::NeedsIntervention => {
                    self.record_approval_event_if_present(tick_id, &action_id, &outcome)?;
                    self.record_action_event(
                        tick_id,
                        &action_id,
                        TraceEventKind::ActionNeedsIntervention {
                            action: action.clone(),
                            result: outcome.verification.clone(),
                        },
                    )?;
                }
                ActionStatus::Failed => {
                    if outcome.output.is_some() {
                        self.record_action_event(
                            tick_id,
                            &action_id,
                            TraceEventKind::ActionExecuted {
                                action: action.clone(),
                                outcome: outcome.output.clone().unwrap_or(serde_json::Value::Null),
                            },
                        )?;
                    }
                    let denial = outcome
                        .post_verification
                        .clone()
                        .filter(|result| !result.allowed)
                        .unwrap_or_else(|| {
                            VerificationResult::deny(
                                outcome
                                    .error
                                    .clone()
                                    .unwrap_or_else(|| "action_failed".to_string()),
                            )
                        });
                    self.record_action_event(
                        tick_id,
                        &action_id,
                        TraceEventKind::ActionFailed {
                            action: action.clone(),
                            error: outcome
                                .error
                                .clone()
                                .unwrap_or_else(|| "action_failed".to_string()),
                            result: denial,
                        },
                    )?;
                }
            }

            escalations.extend(action_escalations);
            outcomes.push(outcome);
        }

        let (feedback, reward) = self.evaluate_outcomes(&decision, &outcomes);
        let duration_ms = start.elapsed().as_millis() as u64;
        let needs_intervention = escalations_require_intervention(&escalations)
            || outcomes.iter().any(|outcome| {
                outcome.status == ActionStatus::NeedsIntervention
                    || outcome
                        .post_verification
                        .as_ref()
                        .map(|result| !result.allowed)
                        .unwrap_or(false)
            });
        let needs_approval = outcomes
            .iter()
            .any(|outcome| outcome.status == ActionStatus::NeedsApproval);
        let outcome_payload = serde_json::json!({
            "tick_id": tick_id,
            "duration_ms": duration_ms,
            "needs_intervention": needs_intervention,
            "needs_approval": needs_approval,
            "escalations": escalations,
            "actions": outcomes
                .iter()
                .map(|outcome| serde_json::to_value(outcome).unwrap_or(serde_json::Value::Null))
                .collect::<Vec<_>>(),
        });

        self.record_tick_event(
            tick_id,
            TraceEventKind::OutcomeRecorded {
                outcome: outcome_payload,
                feedback,
                reward,
            },
        )?;

        let state_trace_event_id =
            TraceEventId::from_run_sequence(self.runtime.run_id(), self.runtime.next_sequence());
        let mut metadata = decision.metadata.clone();
        metadata.tenant_id = Some(self.agent.tenant_id.clone());
        metadata.agent_id = Some(self.agent.agent_id.clone());
        metadata.run_id = Some(self.runtime.run_id().clone());
        metadata.trace_event_id = Some(state_trace_event_id);
        let commit = self
            .state_graph
            .commit(decision.next_state.clone(), metadata)?;
        self.state = decision.next_state;
        self.agent.set_state_head(commit.node_id.clone());

        self.runtime.record_event_with_identity(
            self.trace_identity(tick_id)
                .with_state_node_id(commit.node_id.clone()),
            TraceEventKind::StateCommitted {
                state_hash: commit.node_id.hash().clone(),
                snapshot_id: commit.snapshot_id.clone(),
            },
        )?;

        self.record_tick_event(
            tick_id,
            TraceEventKind::LoopTickCompleted {
                tick_id,
                integrity: None,
            },
        )?;

        Ok(TickOutcome {
            tick_id,
            action_outcomes: outcomes,
            state_commit: commit,
            duration_ms,
            needs_intervention,
            needs_approval,
        })
    }

    fn record_approval_event_if_present(
        &self,
        tick_id: u64,
        action_id: &ActionId,
        outcome: &ActionOutcome,
    ) -> Result<(), LoopError> {
        let Some((status, approval)) = approval_artifact(&outcome.verification) else {
            return Ok(());
        };
        let kind = approval_trace_kind(status.as_str(), approval);
        self.record_action_event(tick_id, action_id, kind)?;
        Ok(())
    }

    fn collect_percepts(&self) -> Result<Vec<Percept>, LoopError> {
        let mut percepts = Vec::new();
        for perceptor in &self.perceptors {
            let mut batch = perceptor.collect(&self.agent)?;
            percepts.append(&mut batch);
        }
        Ok(percepts)
    }

    fn evaluate_outcomes(
        &self,
        decision: &PolicyDecision,
        outcomes: &[ActionOutcome],
    ) -> (Option<Feedback>, Option<Reward>) {
        let mut feedback = None;
        let mut reward = None;
        for (candidate, outcome) in decision.actions.iter().zip(outcomes.iter()) {
            let signal = self.outcome_evaluator.evaluate(&candidate.action, outcome);
            if feedback.is_none() {
                feedback = signal.feedback;
            }
            if reward.is_none() {
                reward = signal.reward;
            }
        }
        (feedback, reward)
    }

    fn evaluate_escalations(
        &self,
        action_id: &ActionId,
        action: &Action,
        adapter: Option<&str>,
        outcome: &mut ActionOutcome,
    ) -> Vec<EscalationContext> {
        let Some(evaluator) = &self.escalation_evaluator else {
            return Vec::new();
        };
        let input = EscalationOutcomeInput {
            tenant_id: &self.agent.tenant_id,
            agent_id: &self.agent.agent_id,
            run_id: self.runtime.run_id(),
            action_id,
            action,
            adapter,
            outcome,
        };
        let escalations = evaluator.evaluate_outcome(&input);
        for escalation in &escalations {
            apply_escalation_to_outcome(outcome, escalation);
        }
        escalations
    }

    fn resume_info(trace_store: &dyn TraceStore, run_id: &RunId) -> Result<ResumeInfo, LoopError> {
        let records = trace_store.read(&run_id.to_string())?;
        let mut snapshot_id = None;
        let mut tick_id = None;
        for record in records {
            let event: TraceEvent = serde_json::from_value(record.payload)?;
            if let TraceEventKind::StateCommitted {
                snapshot_id: Some(snapshot),
                ..
            } = &event.kind
            {
                snapshot_id = Some(snapshot.clone());
            }
            if let TraceEventKind::LoopTickCompleted {
                tick_id: completed, ..
            } = &event.kind
            {
                tick_id = Some(*completed);
            }
        }

        let snapshot_id = snapshot_id
            .ok_or_else(|| LoopError::Resume("no snapshot found in trace history".to_string()))?;
        Ok(ResumeInfo {
            snapshot_id,
            tick_id: tick_id.unwrap_or(0),
        })
    }
}

fn outcome_from_gateway_error(action_id: ActionId, error: GatewayError) -> ActionOutcome {
    let message = error.to_string();
    ActionOutcome {
        action_id,
        status: ActionStatus::Failed,
        verification: VerificationResult::deny(message.clone()),
        post_verification: None,
        output: None,
        error: Some(message),
        completed_at: OffsetDateTime::now_utc(),
    }
}

fn approval_artifact(result: &VerificationResult) -> Option<(String, ApprovalTraceContext)> {
    let artifact = result
        .artifacts
        .get("approval")
        .or_else(|| result.artifacts.get("approval_context"))?;
    let (status, approval_value) = if artifact.get("approval").is_some() {
        (
            artifact
                .get("approval_status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            artifact.get("approval")?,
        )
    } else {
        (
            result
                .artifacts
                .get("approval_status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            artifact,
        )
    };
    serde_json::from_value::<ApprovalTraceContext>(approval_value.clone())
        .ok()
        .map(|approval| (status, approval))
}

fn approval_trace_kind(status: &str, approval: ApprovalTraceContext) -> TraceEventKind {
    match status {
        "required" => TraceEventKind::ApprovalRequested { approval },
        "granted" => TraceEventKind::ApprovalGranted { approval },
        "expired" => TraceEventKind::ApprovalExpired {
            approval,
            reason: "approval_expired".to_string(),
        },
        "revoked" => TraceEventKind::ApprovalRevoked {
            approval,
            reason: "approval_revoked".to_string(),
        },
        "intervention_required" => TraceEventKind::ApprovalDenied {
            approval,
            reason: "approval_policy_expired".to_string(),
        },
        _ => TraceEventKind::ApprovalDenied {
            approval,
            reason: "approval_denied".to_string(),
        },
    }
}

#[cfg(test)]
#[path = "../tests/unit/loop_engine_tests.rs"]
mod tests;
