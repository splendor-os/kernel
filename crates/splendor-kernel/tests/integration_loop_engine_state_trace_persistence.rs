use splendor_gateway::{ActionAdapter, AdapterError, AdapterResult, VerifiedActionGateway};
use splendor_kernel::{
    ActionCandidate, AgentContext, AgentRuntimeConfig, LoopEngine, Perceptor, Policy,
    PolicyDecision, QuotaPolicy, RunId, SideEffectClass, SnapshotPolicy, StateGraph, TenantContext,
    TenantPolicy, TenantRegistry, TraceEvent, TraceEventKind,
};
use splendor_store::{InMemoryStateStore, InMemoryTraceStore, StateData, StateStore, TraceStore};
use splendor_types::{Action, Percept, PerceptProvenance};
use std::sync::Arc;
use time::OffsetDateTime;

struct StaticPerceptor;

impl Perceptor for StaticPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, splendor_kernel::LoopError> {
        Ok(vec![Percept {
            schema: "sensor".to_string(),
            payload: serde_json::json!({"value": 7}),
            provenance: PerceptProvenance {
                source: "integration".to_string(),
                detail: None,
            },
            timestamp: OffsetDateTime::now_utc(),
        }])
    }
}

struct StaticPolicy;

impl Policy for StaticPolicy {
    fn name(&self) -> &str {
        "integration-policy"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        let action = Action {
            name: "noop".to_string(),
            params: serde_json::json!({"ok": true}),
            side_effect_class: SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let candidate = ActionCandidate::new(action).with_adapter("stub");
        let next_state = StateData {
            bytes: vec![9],
            content_type: Some("application/octet-stream".to_string()),
        };
        Ok(PolicyDecision::new(
            vec![candidate],
            next_state,
            Some("snapshot".to_string()),
        ))
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

#[test]
fn loop_engine_persists_state_and_trace_records() {
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let snapshot_policy = SnapshotPolicy {
        interval: Some(1),
        important_labels: Vec::new(),
    };
    let state_graph = StateGraph::new(state_store.clone(), snapshot_policy);
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };

    let tenant_id = splendor_kernel::TenantId::new();
    let agent_id = splendor_kernel::AgentId::new();
    let agent = AgentContext::new(agent_id, tenant_id.clone(), AgentRuntimeConfig::default());
    let registry = TenantRegistry::new();
    registry.insert(TenantContext::new(
        tenant_id.clone(),
        TenantPolicy {
            allowed_actions: vec!["noop".to_string()],
            allowed_adapters: vec!["stub".to_string()],
            allowed_permissions: Vec::new(),
        },
        QuotaPolicy::default(),
    ));
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    gateway.register_adapter("noop", "stub", Arc::new(StubAdapter));
    let gateway = Arc::new(gateway);

    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        state_graph,
        initial_state,
        Box::new(StaticPolicy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert_eq!(outcome.action_outcomes.len(), 1);
    assert!(matches!(
        outcome.action_outcomes[0].status,
        splendor_gateway::ActionStatus::Executed
    ));

    let snapshot_id = outcome
        .state_commit
        .snapshot_id
        .clone()
        .expect("snapshot id");
    let snapshot = state_store
        .load_snapshot(&snapshot_id)
        .expect("load snapshot");
    assert_eq!(snapshot.state.bytes, vec![9]);

    let records = trace_store
        .read(&run_id.to_string())
        .expect("trace records");
    assert!(!records.is_empty());

    let events = records
        .iter()
        .map(|record| serde_json::from_value::<TraceEvent>(record.payload.clone()).expect("event"))
        .collect::<Vec<_>>();
    for (record, event) in records.iter().zip(events.iter()) {
        assert_eq!(record.sequence, event.sequence);
        assert_eq!(record.run_id, run_id.to_string());
        assert_eq!(event.run_id, run_id);
    }

    let first = events.first().expect("first event");
    let last = events.last().expect("last event");
    assert!(matches!(
        first.kind,
        TraceEventKind::LoopTickStarted { tick_id: 1 }
    ));
    assert!(matches!(
        last.kind,
        TraceEventKind::LoopTickCompleted { tick_id: 1, .. }
    ));
    let state_event = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::StateCommitted { .. }))
        .expect("state committed");
    if let TraceEventKind::StateCommitted {
        state_hash,
        snapshot_id,
    } = &state_event.kind
    {
        assert_eq!(state_hash, outcome.state_commit.node_id.hash());
        assert_eq!(
            snapshot_id.as_ref(),
            outcome.state_commit.snapshot_id.as_ref()
        );
    }
}
