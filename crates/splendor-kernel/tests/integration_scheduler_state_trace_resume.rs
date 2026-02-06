use splendor_gateway::{
    ActionAdapter, ActionGateway, ActionStatus, AdapterError, AdapterResult, VerifiedActionGateway,
};
use splendor_kernel::{
    ActionCandidate, AgentContext, AgentRuntimeConfig, LoopEngine, Perceptor, Policy,
    PolicyDecision, QuotaPolicy, RunId, Scheduler, SchedulerConfig, SideEffectClass,
    SnapshotPolicy, StateGraph, TenantContext, TenantPolicy, TenantRegistry, TraceEvent,
    TraceEventKind,
};
use splendor_store::{InMemoryStateStore, InMemoryTraceStore, StateData, StateStore, TraceStore};
use splendor_types::{Action, Percept, PerceptProvenance, TenantId};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

struct StaticPerceptor;

impl Perceptor for StaticPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, splendor_kernel::LoopError> {
        Ok(vec![Percept {
            schema: "sensor".to_string(),
            payload: serde_json::json!({"value": 1}),
            provenance: PerceptProvenance {
                source: "integration".to_string(),
                detail: None,
            },
            timestamp: OffsetDateTime::now_utc(),
        }])
    }
}

struct IncrementPolicy {
    action_name: String,
}

impl Policy for IncrementPolicy {
    fn name(&self) -> &str {
        "increment-policy"
    }

    fn decide(
        &self,
        state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        let counter = state.bytes.first().copied().unwrap_or(0);
        let next = counter.saturating_add(1);
        let action = Action {
            name: self.action_name.clone(),
            params: serde_json::json!({"counter": next}),
            side_effect_class: SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let candidate = ActionCandidate::new(action).with_adapter("stub");
        let next_state = StateData {
            bytes: vec![next],
            content_type: None,
        };
        Ok(PolicyDecision::new(vec![candidate], next_state, None))
    }
}

struct CustomPolicy {
    action_name: String,
    adapter: Option<String>,
    postconditions: Vec<String>,
}

impl Policy for CustomPolicy {
    fn name(&self) -> &str {
        "custom-policy"
    }

    fn decide(
        &self,
        state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        let counter = state.bytes.first().copied().unwrap_or(0);
        let next = counter.saturating_add(1);
        let action = Action {
            name: self.action_name.clone(),
            params: serde_json::json!({"counter": next}),
            side_effect_class: SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: self.postconditions.clone(),
        };
        let mut candidate = ActionCandidate::new(action);
        if let Some(adapter) = &self.adapter {
            candidate = candidate.with_adapter(adapter.clone());
        }
        let next_state = StateData {
            bytes: vec![next],
            content_type: None,
        };
        Ok(PolicyDecision::new(vec![candidate], next_state, None))
    }
}

#[derive(Default)]
struct StubAdapter;

impl ActionAdapter for StubAdapter {
    fn execute(
        &self,
        _action: &splendor_gateway::ActionRequest,
    ) -> Result<AdapterResult, AdapterError> {
        Ok(AdapterResult {
            output: serde_json::json!({"ok": true}),
            satisfied_postconditions: Vec::new(),
        })
    }
}

struct CountingAdapter {
    calls: Arc<Mutex<u32>>,
}

impl CountingAdapter {
    fn new(calls: Arc<Mutex<u32>>) -> Self {
        Self { calls }
    }
}

impl ActionAdapter for CountingAdapter {
    fn execute(
        &self,
        _action: &splendor_gateway::ActionRequest,
    ) -> Result<AdapterResult, AdapterError> {
        *self.calls.lock().expect("calls lock") += 1;
        Ok(AdapterResult {
            output: serde_json::json!({"ok": true}),
            satisfied_postconditions: Vec::new(),
        })
    }
}

fn build_registry(tenant_id: &TenantId, actions: &[&str]) -> TenantRegistry {
    let policy = TenantPolicy {
        allowed_actions: actions.iter().map(|name| (*name).to_string()).collect(),
        allowed_adapters: vec!["stub".to_string()],
        allowed_permissions: Vec::new(),
    };
    let registry = TenantRegistry::new();
    registry.insert(TenantContext::new(
        tenant_id.clone(),
        policy,
        QuotaPolicy::default(),
    ));
    registry
}

fn build_gateway(registry: &TenantRegistry, actions: &[&str]) -> Arc<dyn ActionGateway> {
    let mut gateway = VerifiedActionGateway::new(Arc::new(registry.clone()));
    for action in actions {
        gateway.register_adapter(*action, "stub", Arc::new(StubAdapter));
    }
    Arc::new(gateway)
}

fn build_engine(
    tenant_id: TenantId,
    run_id: RunId,
    action_name: &str,
    state_store: Arc<InMemoryStateStore>,
    trace_store: Arc<InMemoryTraceStore>,
    snapshot_policy: SnapshotPolicy,
    gateway: Arc<dyn ActionGateway>,
) -> LoopEngine {
    let graph = StateGraph::new(state_store, snapshot_policy);
    let initial_state = StateData {
        bytes: vec![0],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        initial_state,
        Box::new(IncrementPolicy {
            action_name: action_name.to_string(),
        }),
        gateway,
        trace_store,
        Some(run_id),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);
    engine
}

fn read_events(trace_store: &InMemoryTraceStore, run_id: &RunId) -> Vec<TraceEvent> {
    trace_store
        .read(&run_id.to_string())
        .expect("records")
        .into_iter()
        .map(|record| serde_json::from_value(record.payload).expect("event"))
        .collect()
}

fn last_snapshot_bytes(
    trace_store: &InMemoryTraceStore,
    state_store: &InMemoryStateStore,
    run_id: &RunId,
) -> Vec<u8> {
    let events = read_events(trace_store, run_id);
    let snapshot_id = events
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            TraceEventKind::StateCommitted {
                snapshot_id: Some(snapshot_id),
                ..
            } => Some(snapshot_id.clone()),
            _ => None,
        })
        .expect("snapshot id");
    state_store
        .load_snapshot(&snapshot_id)
        .expect("snapshot")
        .state
        .bytes
}

#[test]
fn scheduler_runs_cycles_and_persists_state_and_traces() {
    let tenant_id = TenantId::new();
    let actions = ["alpha", "beta"];
    let registry = build_registry(&tenant_id, &actions);
    let gateway = build_gateway(&registry, &actions);

    let snapshot_policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let state_store_one = Arc::new(InMemoryStateStore::default());
    let trace_store_one = Arc::new(InMemoryTraceStore::default());
    let run_id_one = RunId::new();
    let engine_one = build_engine(
        tenant_id.clone(),
        run_id_one.clone(),
        "alpha",
        state_store_one.clone(),
        trace_store_one.clone(),
        snapshot_policy.clone(),
        gateway.clone(),
    );

    let state_store_two = Arc::new(InMemoryStateStore::default());
    let trace_store_two = Arc::new(InMemoryTraceStore::default());
    let run_id_two = RunId::new();
    let engine_two = build_engine(
        tenant_id.clone(),
        run_id_two.clone(),
        "beta",
        state_store_two.clone(),
        trace_store_two.clone(),
        snapshot_policy.clone(),
        gateway,
    );

    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);
    scheduler.add_agent(engine_one);
    scheduler.add_agent(engine_two);

    let steps = scheduler.run_cycles(2).expect("cycles");
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[0].tick_id, 1);
    assert_eq!(steps[1].tick_id, 1);
    assert_eq!(steps[2].tick_id, 2);
    assert_eq!(steps[3].tick_id, 2);

    let events_one = read_events(trace_store_one.as_ref(), &run_id_one);
    assert!(events_one.iter().any(|event| matches!(
        event.kind,
        TraceEventKind::LoopTickCompleted { tick_id: 1, .. }
    )));
    assert!(events_one.iter().any(|event| matches!(
        event.kind,
        TraceEventKind::LoopTickCompleted { tick_id: 2, .. }
    )));

    let events_two = read_events(trace_store_two.as_ref(), &run_id_two);
    assert!(events_two.iter().any(|event| matches!(
        event.kind,
        TraceEventKind::LoopTickCompleted { tick_id: 1, .. }
    )));
    assert!(events_two.iter().any(|event| matches!(
        event.kind,
        TraceEventKind::LoopTickCompleted { tick_id: 2, .. }
    )));

    assert_eq!(
        last_snapshot_bytes(
            trace_store_one.as_ref(),
            state_store_one.as_ref(),
            &run_id_one
        ),
        vec![2]
    );
    assert_eq!(
        last_snapshot_bytes(
            trace_store_two.as_ref(),
            state_store_two.as_ref(),
            &run_id_two
        ),
        vec![2]
    );
}

#[test]
fn scheduler_resumes_from_trace_store_and_continues_state() {
    let tenant_id = TenantId::new();
    let actions = ["resume"];
    let registry = build_registry(&tenant_id, &actions);
    let gateway = build_gateway(&registry, &actions);

    let snapshot_policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let run_id = RunId::new();
    let engine = build_engine(
        tenant_id.clone(),
        run_id.clone(),
        "resume",
        state_store.clone(),
        trace_store.clone(),
        snapshot_policy.clone(),
        gateway,
    );

    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);
    scheduler.add_agent(engine);
    scheduler.run_cycle().expect("first cycle");

    let registry = build_registry(&tenant_id, &actions);
    let gateway = build_gateway(&registry, &actions);
    let graph = StateGraph::new(state_store.clone(), snapshot_policy);
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let mut engine = LoopEngine::resume_from_trace_store(
        agent,
        graph,
        Box::new(IncrementPolicy {
            action_name: "resume".to_string(),
        }),
        gateway,
        trace_store.clone(),
        run_id.clone(),
    )
    .expect("resume");
    engine.add_perceptor(StaticPerceptor);

    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);
    scheduler.add_agent(engine);
    scheduler.run_cycle().expect("second cycle");

    let events = read_events(trace_store.as_ref(), &run_id);
    let completed = events
        .iter()
        .filter(|event| matches!(event.kind, TraceEventKind::LoopTickCompleted { .. }))
        .count();
    assert_eq!(completed, 2);
    assert_eq!(
        last_snapshot_bytes(trace_store.as_ref(), state_store.as_ref(), &run_id),
        vec![2]
    );
}

#[test]
fn loop_engine_marks_needs_intervention_on_postcondition_failure() {
    let tenant_id = TenantId::new();
    let actions = ["post"];
    let registry = build_registry(&tenant_id, &actions);
    registry.begin_tick(1, OffsetDateTime::now_utc());
    let gateway = build_gateway(&registry, &actions);

    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let snapshot_policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let graph = StateGraph::new(state_store.clone(), snapshot_policy);
    let initial_state = StateData {
        bytes: vec![0],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        initial_state,
        Box::new(CustomPolicy {
            action_name: "post".to_string(),
            adapter: Some("stub".to_string()),
            postconditions: vec!["done".to_string()],
        }),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Failed
    ));
    assert!(outcome.needs_intervention);

    let events = read_events(trace_store.as_ref(), &run_id);
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionExecuted { .. })));
    let denial = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("action denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denial.kind {
        assert!(result
            .reasons
            .contains(&"postcondition_missing".to_string()));
    }
}

#[test]
fn loop_engine_denies_adapter_mismatch_without_execution() {
    let tenant_id = TenantId::new();
    let actions = ["mismatch"];
    let registry = build_registry(&tenant_id, &actions);
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let calls = Arc::new(Mutex::new(0));
    let mut gateway = VerifiedActionGateway::new(Arc::new(registry.clone()));
    gateway.register_adapter(
        "mismatch",
        "stub",
        Arc::new(CountingAdapter::new(Arc::clone(&calls))),
    );
    let gateway = Arc::new(gateway);

    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let snapshot_policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let graph = StateGraph::new(state_store, snapshot_policy);
    let initial_state = StateData {
        bytes: vec![0],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        initial_state,
        Box::new(CustomPolicy {
            action_name: "mismatch".to_string(),
            adapter: Some("wrong".to_string()),
            postconditions: Vec::new(),
        }),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Denied
    ));
    assert!(!outcome.needs_intervention);
    assert_eq!(*calls.lock().expect("calls lock"), 0);

    let events = read_events(trace_store.as_ref(), &run_id);
    assert!(!events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionExecuted { .. })));
    let denial = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("action denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denial.kind {
        assert!(result.reasons.contains(&"adapter_mismatch".to_string()));
    }
}
