use super::*;
use crate::SnapshotPolicy;
use splendor_store::{
    InMemoryStateStore, InMemoryTraceStore, StateData, StateDataRef, StateMetadata, StateNode,
    StateNodeId, StateSnapshot, StateStore, StateStoreError,
};
use splendor_types::{
    ActionId, ApprovalDecision, ApprovalEvidence, ApprovalId, ApprovalTraceContext, ConstraintKind,
    ConstraintScope, DelegatedAuthority, PerceptProvenance, QuotaUsage, RevocationStatus, RunId,
    TenantId, TraceEvent, WorkOrder, WorkOrderId, WorkOrderPlacement, WorkOrderQuotaPolicy,
    WORK_ORDER_SCHEMA_VERSION,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct CapturingTraceSink {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl crate::TraceSink for CapturingTraceSink {
    fn record(&self, event: &TraceEvent) -> Result<(), crate::TraceError> {
        self.events.lock().expect("events lock").push(event.clone());
        Ok(())
    }
}

struct StaticPerceptor;

impl Perceptor for StaticPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, LoopError> {
        Ok(vec![Percept {
            schema: "sensor".to_string(),
            payload: serde_json::json!({"value": 7}),
            provenance: PerceptProvenance {
                source: "unit".to_string(),
                detail: None,
            },
            timestamp: OffsetDateTime::now_utc(),
        }])
    }
}

struct StaticPolicy;

impl Policy for StaticPolicy {
    fn name(&self) -> &str {
        "static-policy"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, LoopError> {
        let action = Action {
            name: "noop".to_string(),
            params: serde_json::json!({"ok": true}),
            side_effect_class: splendor_types::SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let candidate = ActionCandidate::new(action);
        let next_state = StateData {
            bytes: vec![2],
            content_type: Some("application/octet-stream".to_string()),
        };
        Ok(PolicyDecision::new(
            vec![candidate],
            next_state,
            Some("tick".to_string()),
        ))
    }
}

#[derive(Default)]
struct StubGateway;

impl ActionGateway for StubGateway {
    fn submit(&self, action: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        Ok(ActionOutcome {
            action_id: action.action_id,
            status: ActionStatus::Executed,
            verification: VerificationResult::allow(),
            post_verification: Some(VerificationResult::allow()),
            output: Some(serde_json::json!({"ok": true})),
            error: None,
            completed_at: OffsetDateTime::now_utc(),
        })
    }
}

struct StaticAdapterPolicy;

impl Policy for StaticAdapterPolicy {
    fn name(&self) -> &str {
        "static-adapter-policy"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, LoopError> {
        let action = Action {
            name: "noop".to_string(),
            params: serde_json::json!({"ok": true}),
            side_effect_class: splendor_types::SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let next_state = StateData {
            bytes: vec![2],
            content_type: Some("application/octet-stream".to_string()),
        };
        Ok(PolicyDecision::new(
            vec![ActionCandidate::new(action).with_adapter("stub".to_string())],
            next_state,
            Some("tick".to_string()),
        ))
    }
}

#[derive(Default)]
struct DenyGateway;

impl ActionGateway for DenyGateway {
    fn submit(&self, action: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        Ok(ActionOutcome {
            action_id: action.action_id,
            status: ActionStatus::Denied,
            verification: VerificationResult::deny("denied"),
            post_verification: None,
            output: None,
            error: Some("denied".to_string()),
            completed_at: OffsetDateTime::now_utc(),
        })
    }
}

#[derive(Default)]
struct StaticConstraintEngine;

impl ConstraintEngine for StaticConstraintEngine {
    fn evaluate(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
        _actions: &[ActionCandidate],
    ) -> ConstraintEvaluation {
        ConstraintEvaluation {
            constraints: vec![Constraint {
                id: "c1".to_string(),
                kind: ConstraintKind::Hard,
                scope: ConstraintScope::Action,
                predicate: "always".to_string(),
                obligation: None,
            }],
            result: VerificationResult::allow(),
        }
    }
}

#[derive(Default)]
struct DenyConstraintEngine;

impl ConstraintEngine for DenyConstraintEngine {
    fn evaluate(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
        _actions: &[ActionCandidate],
    ) -> ConstraintEvaluation {
        ConstraintEvaluation {
            constraints: vec![Constraint {
                id: "deny".to_string(),
                kind: ConstraintKind::Hard,
                scope: ConstraintScope::Action,
                predicate: "never".to_string(),
                obligation: None,
            }],
            result: VerificationResult::deny("constraints_denied"),
        }
    }
}

#[derive(Default)]
struct CountingGateway {
    calls: Arc<Mutex<u32>>,
}

impl ActionGateway for CountingGateway {
    fn submit(&self, action: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        *self.calls.lock().expect("calls lock") += 1;
        Ok(ActionOutcome {
            action_id: action.action_id,
            status: ActionStatus::Executed,
            verification: VerificationResult::allow(),
            post_verification: Some(VerificationResult::allow()),
            output: Some(serde_json::json!({"ok": true})),
            error: None,
            completed_at: OffsetDateTime::now_utc(),
        })
    }
}

#[derive(Default)]
struct ErrorGateway;

impl ActionGateway for ErrorGateway {
    fn submit(&self, _action: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        Err(GatewayError::AdapterFailed("boom".to_string()))
    }
}

struct MultiActionPolicy;

impl Policy for MultiActionPolicy {
    fn name(&self) -> &str {
        "multi-policy"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, LoopError> {
        let first = Action {
            name: "first".to_string(),
            params: serde_json::json!({}),
            side_effect_class: splendor_types::SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let second = Action {
            name: "second".to_string(),
            params: serde_json::json!({}),
            side_effect_class: splendor_types::SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let next_state = StateData {
            bytes: vec![4],
            content_type: None,
        };
        Ok(PolicyDecision::new(
            vec![ActionCandidate::new(first), ActionCandidate::new(second)],
            next_state,
            None,
        ))
    }
}

fn work_order_for(agent: &AgentContext, run_id: RunId) -> WorkOrder {
    let now = OffsetDateTime::now_utc();
    WorkOrder {
        schema_version: WORK_ORDER_SCHEMA_VERSION.to_string(),
        work_order_id: WorkOrderId::try_new("wo_loop").expect("work order id"),
        tenant_id: agent.tenant_id.clone(),
        agent_id: agent.agent_id.clone(),
        run_id: Some(run_id),
        objective: "record work order metadata".to_string(),
        allowed_actions: vec!["noop".to_string()],
        allowed_adapters: vec!["stub".to_string()],
        allowed_permissions: Vec::new(),
        data_refs: Vec::new(),
        quotas: WorkOrderQuotaPolicy::default(),
        placement: WorkOrderPlacement::default(),
        issued_at: now - time::Duration::minutes(1),
        expires_at: now + time::Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

struct RecordingOutcomeEvaluator;

impl OutcomeEvaluator for RecordingOutcomeEvaluator {
    fn evaluate(&self, action: &Action, _outcome: &ActionOutcome) -> OutcomeSignal {
        let now = OffsetDateTime::now_utc();
        if action.name == "first" {
            OutcomeSignal {
                feedback: Some(Feedback {
                    kind: "first".to_string(),
                    payload: serde_json::json!({"action": "first"}),
                    recorded_at: now,
                }),
                reward: None,
            }
        } else {
            OutcomeSignal {
                feedback: Some(Feedback {
                    kind: "second".to_string(),
                    payload: serde_json::json!({"action": "second"}),
                    recorded_at: now,
                }),
                reward: Some(Reward {
                    value: 2.0,
                    units: Some("pts".to_string()),
                    recorded_at: now,
                    context: Some(serde_json::json!({"source": "second"})),
                }),
            }
        }
    }
}

struct FailingStateStore;

impl StateStore for FailingStateStore {
    fn put_state(&self, _state: StateData) -> Result<StateDataRef, StateStoreError> {
        Err(StateStoreError::MissingState)
    }

    fn get_state(&self, _data_ref: &StateDataRef) -> Result<StateData, StateStoreError> {
        Err(StateStoreError::MissingState)
    }

    fn commit_node(
        &self,
        _parent_ids: Vec<StateNodeId>,
        _data_ref: StateDataRef,
        _metadata: StateMetadata,
    ) -> Result<StateNodeId, StateStoreError> {
        Err(StateStoreError::MissingState)
    }

    fn get_node(&self, _node_id: &StateNodeId) -> Result<StateNode, StateStoreError> {
        Err(StateStoreError::MissingNode)
    }

    fn snapshot(
        &self,
        _node_id: &StateNodeId,
    ) -> Result<splendor_types::SnapshotId, StateStoreError> {
        Err(StateStoreError::MissingSnapshot)
    }

    fn load_snapshot(
        &self,
        _snapshot_id: &splendor_types::SnapshotId,
    ) -> Result<StateSnapshot, StateStoreError> {
        Err(StateStoreError::MissingSnapshot)
    }
}

#[test]
fn loop_engine_emits_ordered_trace_events() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent_id = splendor_types::AgentId::new();
    let tenant_id = splendor_types::TenantId::new();
    let agent = AgentContext::new(
        agent_id.clone(),
        tenant_id.clone(),
        crate::AgentRuntimeConfig::default(),
    );
    let gateway = Arc::new(StubGateway);
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        runtime,
    );
    engine.add_perceptor(StaticPerceptor);
    engine.set_constraint_engine(StaticConstraintEngine);

    let outcome = engine.tick(1).expect("tick");
    assert_eq!(outcome.tick_id, 1);
    assert_eq!(outcome.action_outcomes.len(), 1);

    let recorded = events.lock().expect("events lock");
    let trace_ids = recorded
        .iter()
        .map(|event| event.trace_event_id.to_string())
        .collect::<HashSet<_>>();
    assert_eq!(trace_ids.len(), recorded.len());
    for (sequence, event) in recorded.iter().enumerate() {
        assert_eq!(event.sequence, sequence as u64);
        assert_eq!(
            event.trace_event_id,
            splendor_types::TraceEventId::from_run_sequence(&event.run_id, event.sequence)
        );
        assert_eq!(event.identity.run_id, event.run_id);
        assert_eq!(event.identity.tenant_id.as_ref(), Some(&tenant_id));
        assert_eq!(event.identity.agent_id.as_ref(), Some(&agent_id));
        assert_eq!(
            event.identity.tick_id,
            Some(splendor_types::TickId::from(1))
        );
    }
    let kinds = recorded
        .iter()
        .map(|event| event_kind_label(&event.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            "LoopTickStarted",
            "PerceptsReceived",
            "StateLoaded",
            "PolicyInvoked",
            "PolicyCompleted",
            "CandidatesProposed",
            "ConstraintsEvaluated",
            "ActionVerificationStarted",
            "ActionVerificationCompleted",
            "ActionExecuted",
            "OutcomeRecorded",
            "StateCommitted",
            "LoopTickCompleted",
        ]
    );
    let state_event = recorded
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::StateCommitted { .. }))
        .expect("state committed");
    assert_eq!(
        state_event.identity.state_node_id.as_ref(),
        Some(&outcome.state_commit.node_id)
    );
    assert_eq!(
        outcome.state_commit.trace_event_id.as_ref(),
        Some(&state_event.trace_event_id)
    );
    if let TraceEventKind::StateCommitted { state_hash, .. } = &state_event.kind {
        assert_eq!(state_hash, outcome.state_commit.node_id.hash());
    }

    for event in recorded.iter().filter(|event| {
        matches!(
            event.kind,
            TraceEventKind::ActionVerificationStarted { .. }
                | TraceEventKind::ActionVerificationCompleted { .. }
                | TraceEventKind::ActionExecuted { .. }
                | TraceEventKind::ActionDenied { .. }
                | TraceEventKind::ActionFailed { .. }
        )
    }) {
        assert_eq!(
            event.identity.action_id.as_ref(),
            Some(&outcome.action_outcomes[0].action_id)
        );
    }
}

#[test]
fn loop_engine_state_commit_failure_does_not_complete_tick() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let graph = StateGraph::new(Arc::new(FailingStateStore), SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        Arc::new(StubGateway),
        runtime,
    );

    let error = engine.tick(1).expect_err("state commit failure");
    assert!(matches!(error, LoopError::StateGraph(_)));
    assert_eq!(engine.state_graph.tick(), 0);
    assert!(engine.agent.state_head.is_none());

    let recorded = events.lock().expect("events lock");
    assert!(recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::LoopTickStarted { tick_id: 1 })));
    assert!(!recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::StateCommitted { .. })));
    assert!(!recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::LoopTickCompleted { .. })));
}

#[test]
fn loop_engine_denies_actions_when_policy_disallows() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });
    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let gateway = Arc::new(DenyGateway);
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        runtime,
    );

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Denied
    ));

    let recorded = events.lock().expect("events lock");
    assert!(recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. })));
}

#[test]
fn loop_engine_denies_when_constraints_fail_and_skips_gateway() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let calls = Arc::new(Mutex::new(0));
    let gateway = Arc::new(CountingGateway {
        calls: Arc::clone(&calls),
    });
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        runtime,
    );
    engine.set_constraint_engine(DenyConstraintEngine);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Denied
    ));
    assert_eq!(*calls.lock().expect("calls lock"), 0);

    let recorded = events.lock().expect("events lock");
    let denied = recorded
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result.reasons.contains(&"constraints_denied".to_string()));
    }
}

#[test]
fn loop_engine_denies_child_action_outside_delegated_scope_and_skips_gateway() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let mut agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    agent.set_delegated_authority(DelegatedAuthority::empty());
    let calls = Arc::new(Mutex::new(0));
    let gateway = Arc::new(CountingGateway {
        calls: Arc::clone(&calls),
    });
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        runtime,
    );

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Denied
    ));
    assert_eq!(*calls.lock().expect("calls lock"), 0);

    let recorded = events.lock().expect("events lock");
    let denied = recorded
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("delegated scope denial");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result
            .reasons
            .contains(&"delegated_action_not_allowed".to_string()));
    }
}

#[test]
fn loop_engine_denies_delegated_action_without_explicit_adapter_and_skips_gateway() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let mut agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    agent.set_delegated_authority(DelegatedAuthority {
        allowed_actions: vec!["noop".to_string()],
        allowed_adapters: vec!["stub".to_string()],
        allowed_permissions: Vec::new(),
    });
    let calls = Arc::new(Mutex::new(0));
    let gateway = Arc::new(CountingGateway {
        calls: Arc::clone(&calls),
    });
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        runtime,
    );

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Denied
    ));
    assert_eq!(*calls.lock().expect("calls lock"), 0);

    let recorded = events.lock().expect("events lock");
    let denied = recorded
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("delegated missing adapter denial");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result
            .reasons
            .contains(&"delegated_adapter_unspecified".to_string()));
    }
}

#[test]
fn loop_engine_allows_delegated_action_with_explicit_adapter() {
    let runtime = KernelRuntime::new(KernelRuntimeConfig::default());
    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let mut agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    agent.set_delegated_authority(DelegatedAuthority {
        allowed_actions: vec!["noop".to_string()],
        allowed_adapters: vec!["stub".to_string()],
        allowed_permissions: Vec::new(),
    });
    let calls = Arc::new(Mutex::new(0));
    let gateway = Arc::new(CountingGateway {
        calls: Arc::clone(&calls),
    });
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticAdapterPolicy),
        gateway,
        runtime,
    );

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Executed
    ));
    assert_eq!(*calls.lock().expect("calls lock"), 1);
}

#[test]
fn loop_engine_records_gateway_errors_as_failed() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let gateway = Arc::new(ErrorGateway);
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        runtime,
    );

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Failed
    ));

    let recorded = events.lock().expect("events lock");
    assert!(!recorded
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionExecuted { .. })));
    let failed = recorded
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionFailed { .. }))
        .expect("failed");
    if let TraceEventKind::ActionFailed { result, .. } = &failed.kind {
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.contains("adapter execution failed")));
    }
}

#[test]
fn loop_engine_records_outcome_feedback_and_reward() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CapturingTraceSink {
        events: Arc::clone(&events),
    };
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(sink),
        ..KernelRuntimeConfig::default()
    });

    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let gateway = Arc::new(StubGateway);
    let mut engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(MultiActionPolicy),
        gateway,
        runtime,
    );
    engine.set_outcome_evaluator(RecordingOutcomeEvaluator);

    engine.tick(1).expect("tick");

    let recorded = events.lock().expect("events lock");
    let outcome = recorded
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::OutcomeRecorded { .. }))
        .expect("outcome event");
    if let TraceEventKind::OutcomeRecorded {
        feedback, reward, ..
    } = &outcome.kind
    {
        let feedback = feedback.as_ref().expect("feedback");
        assert_eq!(feedback.kind, "first");
        let reward = reward.as_ref().expect("reward");
        assert_eq!(reward.value, 2.0);
        assert_eq!(reward.units.as_deref(), Some("pts"));
    }
}

#[test]
fn loop_engine_resumes_from_trace_store() {
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let snapshot_policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let graph = StateGraph::new(state_store.clone(), snapshot_policy.clone());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let gateway = Arc::new(StubGateway);
    let run_id = RunId::new();

    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);
    engine.tick(1).expect("tick");

    let graph = StateGraph::new(state_store, snapshot_policy);
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let gateway = Arc::new(StubGateway);
    let engine = LoopEngine::resume_from_trace_store(
        agent,
        graph,
        Box::new(StaticPolicy),
        gateway,
        trace_store,
        run_id.clone(),
    )
    .expect("resume");

    assert_eq!(engine.state.bytes, vec![2]);
    assert_eq!(engine.state_graph.tick(), 1);
    assert!(engine.agent.state_head.is_some());
    assert_eq!(engine.runtime.run_id(), &run_id);
}

#[test]
fn action_candidate_builder_methods() {
    let action = Action {
        name: "build".to_string(),
        params: serde_json::json!({"ok": true}),
        side_effect_class: splendor_types::SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: vec!["ready".to_string()],
        postconditions: Vec::new(),
    };
    let usage = QuotaUsage {
        actions: 2,
        ..QuotaUsage::default()
    };
    let run_id = RunId::new();
    let evidence = ApprovalEvidence::new(
        ApprovalId::new(),
        TenantId::new(),
        splendor_types::AgentId::new(),
        run_id,
        ApprovalDecision::Granted,
        OffsetDateTime::now_utc() + time::Duration::minutes(5),
    )
    .with_action_name("build")
    .with_adapter("adapter");
    let candidate = ActionCandidate::new(action)
        .with_adapter("adapter".to_string())
        .with_usage(usage)
        .with_satisfied_preconditions(vec!["ready".to_string()])
        .with_approval_evidence(evidence.clone());
    assert_eq!(candidate.adapter.as_deref(), Some("adapter"));
    assert_eq!(candidate.usage.actions, 2);
    assert_eq!(candidate.satisfied_preconditions, vec!["ready".to_string()]);
    assert_eq!(candidate.approval_evidence.as_ref(), Some(&evidence));
}

#[test]
fn approval_artifact_and_trace_kind_cover_lifecycle_variants() {
    let run_id = RunId::new();
    let approval = ApprovalTraceContext {
        approval_id: ApprovalId::new(),
        tenant_id: TenantId::new(),
        agent_id: splendor_types::AgentId::new(),
        run_id: run_id.clone(),
        action_id: Some(ActionId::new()),
        action_name: "artifact.publish".to_string(),
        adapter: Some("artifact-store".to_string()),
        decision: Some(ApprovalDecision::Granted),
        reason: Some("operator decision".to_string()),
        policy_id: Some("publish_policy".to_string()),
        risk_level: Some("external".to_string()),
        issued_at: Some(OffsetDateTime::now_utc()),
        expires_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(10)),
        revoked: false,
    };

    let direct = VerificationResult {
        allowed: false,
        reasons: vec!["approval_required".to_string()],
        artifacts: serde_json::json!({
            "approval_status": "required",
            "approval_context": approval,
        }),
    };
    let (status, parsed) = approval_artifact(&direct).expect("direct approval artifact");
    assert_eq!(status, "required");
    assert_eq!(parsed.action_name, "artifact.publish");

    let nested = VerificationResult {
        allowed: true,
        reasons: Vec::new(),
        artifacts: serde_json::json!({
            "approval": {
                "approval_status": "granted",
                "approval": parsed,
            }
        }),
    };
    let (status, approval) = approval_artifact(&nested).expect("nested approval artifact");
    assert_eq!(status, "granted");

    for (status, expected) in [
        ("required", "requested"),
        ("granted", "granted"),
        ("expired", "expired"),
        ("revoked", "revoked"),
        ("intervention_required", "policy_expired"),
        ("denied", "denied"),
    ] {
        let kind = approval_trace_kind(status, approval.clone());
        match (expected, kind) {
            ("requested", TraceEventKind::ApprovalRequested { .. }) => {}
            ("granted", TraceEventKind::ApprovalGranted { .. }) => {}
            ("expired", TraceEventKind::ApprovalExpired { reason, .. }) => {
                assert_eq!(reason, "approval_expired")
            }
            ("revoked", TraceEventKind::ApprovalRevoked { reason, .. }) => {
                assert_eq!(reason, "approval_revoked")
            }
            ("policy_expired", TraceEventKind::ApprovalDenied { reason, .. }) => {
                assert_eq!(reason, "approval_policy_expired")
            }
            ("denied", TraceEventKind::ApprovalDenied { reason, .. }) => {
                assert_eq!(reason, "approval_denied")
            }
            (expected, other) => panic!("expected {expected}, got {other:?}"),
        }
    }

    assert!(approval_artifact(&VerificationResult::allow()).is_none());
}

#[test]
fn loop_engine_new_sets_head_from_graph() {
    let store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(store, SnapshotPolicy::default());
    let commit = graph
        .commit(
            StateData {
                bytes: vec![1],
                content_type: None,
            },
            splendor_store::StateMetadata {
                created_at: OffsetDateTime::now_utc(),
                label: None,
                tenant_id: None,
                agent_id: None,
                run_id: None,
                trace_event_id: None,
            },
        )
        .expect("commit");
    let head = commit.node_id.clone();

    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let engine = LoopEngine::new(
        agent,
        graph,
        StateData {
            bytes: vec![0],
            content_type: None,
        },
        Box::new(StaticPolicy),
        Arc::new(StubGateway),
    );

    assert_eq!(engine.agent.state_head.as_ref(), Some(&head));
}

#[test]
fn loop_engine_records_validated_work_order_metadata() {
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let store = Arc::new(InMemoryStateStore::default());
    let graph = StateGraph::new(store, SnapshotPolicy::default());
    let run_id = RunId::new();
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        splendor_types::TenantId::new(),
        crate::AgentRuntimeConfig::default(),
    );
    let work_order = work_order_for(&agent, run_id.clone());
    let context = RunTraceContext::new(Some(run_id.clone())).with_work_order(work_order.clone());

    let engine = LoopEngine::with_trace_store_and_work_order(
        agent,
        graph,
        StateData {
            bytes: Vec::new(),
            content_type: None,
        },
        Box::new(StaticPolicy),
        Arc::new(StubGateway),
        trace_store.clone(),
        context,
    )
    .expect("engine");

    assert_eq!(
        engine.agent.config.metadata.get("work_order_id"),
        Some(&"wo_loop".to_string())
    );
    let records = trace_store.read(&run_id.to_string()).expect("records");
    assert_eq!(records.len(), 2);
    let accepted: TraceEvent = serde_json::from_value(records[1].payload.clone()).unwrap();
    match accepted.kind {
        TraceEventKind::WorkOrderAccepted {
            work_order_id,
            tenant_id,
            agent_id,
            run_id: accepted_run,
        } => {
            assert_eq!(work_order_id.as_str(), "wo_loop");
            assert_eq!(tenant_id, work_order.tenant_id);
            assert_eq!(agent_id, work_order.agent_id);
            assert_eq!(accepted_run, Some(run_id));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

fn event_kind_label(kind: &TraceEventKind) -> &'static str {
    match kind {
        TraceEventKind::RunStarted => "RunStarted",
        TraceEventKind::WorkOrderAccepted { .. } => "WorkOrderAccepted",
        TraceEventKind::WorkOrderRejected { .. } => "WorkOrderRejected",
        TraceEventKind::RunPaused { .. } => "RunPaused",
        TraceEventKind::RunResumed { .. } => "RunResumed",
        TraceEventKind::RunStopped { .. } => "RunStopped",
        TraceEventKind::PerceptsAppended { .. } => "PerceptsAppended",
        TraceEventKind::DaemonAudit { .. } => "DaemonAudit",
        TraceEventKind::CircuitBreakerTripped { .. } => "CircuitBreakerTripped",
        TraceEventKind::CircuitBreakerCleared { .. } => "CircuitBreakerCleared",
        TraceEventKind::LoopTickStarted { .. } => "LoopTickStarted",
        TraceEventKind::PerceptsReceived { .. } => "PerceptsReceived",
        TraceEventKind::StateLoaded { .. } => "StateLoaded",
        TraceEventKind::PolicyInvoked { .. } => "PolicyInvoked",
        TraceEventKind::PolicyCompleted { .. } => "PolicyCompleted",
        TraceEventKind::CandidatesProposed { .. } => "CandidatesProposed",
        TraceEventKind::ConstraintsEvaluated { .. } => "ConstraintsEvaluated",
        TraceEventKind::ActionVerificationStarted { .. } => "ActionVerificationStarted",
        TraceEventKind::ActionVerificationCompleted { .. } => "ActionVerificationCompleted",
        TraceEventKind::ActionNeedsApproval { .. } => "ActionNeedsApproval",
        TraceEventKind::ActionExecuted { .. } => "ActionExecuted",
        TraceEventKind::ActionDenied { .. } => "ActionDenied",
        TraceEventKind::ActionFailed { .. } => "ActionFailed",
        TraceEventKind::ApprovalRequested { .. } => "ApprovalRequested",
        TraceEventKind::ApprovalGranted { .. } => "ApprovalGranted",
        TraceEventKind::ApprovalDenied { .. } => "ApprovalDenied",
        TraceEventKind::ApprovalExpired { .. } => "ApprovalExpired",
        TraceEventKind::ApprovalRevoked { .. } => "ApprovalRevoked",
        TraceEventKind::ActionNeedsIntervention { .. } => "ActionNeedsIntervention",
        TraceEventKind::EscalationTriggered { .. } => "EscalationTriggered",
        TraceEventKind::OutcomeRecorded { .. } => "OutcomeRecorded",
        TraceEventKind::StateCommitted { .. } => "StateCommitted",
        TraceEventKind::StateHandoffExported { .. } => "StateHandoffExported",
        TraceEventKind::StateHandoffImported { .. } => "StateHandoffImported",
        TraceEventKind::StateHandoffImportFailed { .. } => "StateHandoffImportFailed",
        TraceEventKind::ReadOnlyStateReferenced { .. } => "ReadOnlyStateReferenced",
        TraceEventKind::MessageQueued { .. } => "MessageQueued",
        TraceEventKind::MessageDelivered { .. } => "MessageDelivered",
        TraceEventKind::MessageRejected { .. } => "MessageRejected",
        TraceEventKind::MessageExpired { .. } => "MessageExpired",
        TraceEventKind::MessageConsumed { .. } => "MessageConsumed",
        TraceEventKind::RemoteMessageSent { .. } => "RemoteMessageSent",
        TraceEventKind::RemoteMessageAccepted { .. } => "RemoteMessageAccepted",
        TraceEventKind::RemoteMessageRejected { .. } => "RemoteMessageRejected",
        TraceEventKind::RemoteMessageDelivered { .. } => "RemoteMessageDelivered",
        TraceEventKind::RemoteMessageTimedOut { .. } => "RemoteMessageTimedOut",
        TraceEventKind::RemoteMessageDuplicate { .. } => "RemoteMessageDuplicate",
        TraceEventKind::RemoteMessageTransportFailed { .. } => "RemoteMessageTransportFailed",
        TraceEventKind::DelegationRequested { .. } => "DelegationRequested",
        TraceEventKind::DelegationRejected { .. } => "DelegationRejected",
        TraceEventKind::ParentRunCancelled { .. } => "ParentRunCancelled",
        TraceEventKind::ChildRunStarted { .. } => "ChildRunStarted",
        TraceEventKind::ChildRunCompleted { .. } => "ChildRunCompleted",
        TraceEventKind::ChildRunFailed { .. } => "ChildRunFailed",
        TraceEventKind::ChildRunLinked { .. } => "ChildRunLinked",
        TraceEventKind::GovernanceApprovalRequested { .. } => "GovernanceApprovalRequested",
        TraceEventKind::GovernanceApprovalGranted { .. } => "GovernanceApprovalGranted",
        TraceEventKind::GovernanceApprovalDenied { .. } => "GovernanceApprovalDenied",
        TraceEventKind::GovernanceApprovalExpired { .. } => "GovernanceApprovalExpired",
        TraceEventKind::GovernanceApprovalRevoked { .. } => "GovernanceApprovalRevoked",
        TraceEventKind::EscalationOpened { .. } => "EscalationOpened",
        TraceEventKind::EscalationResolved { .. } => "EscalationResolved",
        TraceEventKind::EscalationExpired { .. } => "EscalationExpired",
        TraceEventKind::EscalationRevoked { .. } => "EscalationRevoked",
        TraceEventKind::InterventionRequested { .. } => "InterventionRequested",
        TraceEventKind::InterventionResolved { .. } => "InterventionResolved",
        TraceEventKind::InterventionCancelled { .. } => "InterventionCancelled",
        TraceEventKind::InterventionExpired { .. } => "InterventionExpired",
        TraceEventKind::InterventionRevoked { .. } => "InterventionRevoked",
        TraceEventKind::GovernanceCircuitBreakerTripped { .. } => "GovernanceCircuitBreakerTripped",
        TraceEventKind::GovernanceCircuitBreakerCleared { .. } => "GovernanceCircuitBreakerCleared",
        TraceEventKind::GovernanceCircuitBreakerExpired { .. } => "GovernanceCircuitBreakerExpired",
        TraceEventKind::GovernanceCircuitBreakerRevoked { .. } => "GovernanceCircuitBreakerRevoked",
        TraceEventKind::KillSwitchActivated { .. } => "KillSwitchActivated",
        TraceEventKind::KillSwitchCleared { .. } => "KillSwitchCleared",
        TraceEventKind::KillSwitchExpired { .. } => "KillSwitchExpired",
        TraceEventKind::KillSwitchRevoked { .. } => "KillSwitchRevoked",
        TraceEventKind::GovernanceTransitionRejected { .. } => "GovernanceTransitionRejected",
        TraceEventKind::LoopTickCompleted { .. } => "LoopTickCompleted",
    }
}
