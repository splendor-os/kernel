use splendor_gateway::{ActionAdapter, AdapterError, AdapterResult, VerifiedActionGateway};
use splendor_kernel::{
    ActionCandidate, AgentContext, AgentRuntimeConfig, EscalationDecision, EscalationPolicy,
    EscalationRule, EscalationScope, EscalationTrigger, LoopEngine, Perceptor, Policy,
    PolicyDecision, QuotaPolicy, RunId, SideEffectClass, SnapshotPolicy, StateGraph, TenantContext,
    TenantPolicy, TenantRegistry, TraceEvent, TraceEventKind,
};
use splendor_store::{InMemoryStateStore, InMemoryTraceStore, StateData, TraceStore};
use splendor_types::{Action, Percept, PerceptProvenance, QuotaUsage};
use std::sync::Arc;
use time::OffsetDateTime;

struct EmptyPerceptor;

impl Perceptor for EmptyPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, splendor_kernel::LoopError> {
        Ok(vec![Percept {
            schema: "sensor".to_string(),
            payload: serde_json::json!({}),
            provenance: PerceptProvenance {
                source: "integration".to_string(),
                detail: None,
            },
            timestamp: OffsetDateTime::now_utc(),
        }])
    }
}

#[test]
fn quota_pressure_escalates_without_consuming_denied_usage() {
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let state_graph = StateGraph::new(state_store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };

    let tenant_id = splendor_kernel::TenantId::new();
    let agent_id = splendor_kernel::AgentId::new();
    let agent = AgentContext::new(
        agent_id.clone(),
        tenant_id.clone(),
        AgentRuntimeConfig::default(),
    );
    let registry = TenantRegistry::new();
    registry.insert(TenantContext::new(
        tenant_id.clone(),
        TenantPolicy {
            allowed_actions: vec!["quota".to_string()],
            allowed_adapters: vec!["stub".to_string()],
            allowed_permissions: Vec::new(),
        },
        QuotaPolicy {
            max_actions_per_tick: Some(0),
            ..QuotaPolicy::default()
        },
    ));
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry.clone()));
    gateway.register_adapter("quota", "stub", Arc::new(StubAdapter));
    let gateway = Arc::new(gateway);

    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        state_graph,
        initial_state,
        Box::new(SingleActionPolicy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(EmptyPerceptor);
    engine.set_escalation_policy(EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::QuotaPressure,
        EscalationScope::Action,
        1,
        EscalationDecision::Pause,
    )]));

    let outcome = engine.tick(1).expect("tick");
    assert!(outcome.needs_intervention);
    assert!(matches!(
        outcome.action_outcomes[0].status,
        splendor_gateway::ActionStatus::NeedsIntervention
    ));
    let usage = registry
        .with_tenant(&tenant_id, |tenant| tenant.tick_usage())
        .expect("tenant usage");
    assert_eq!(usage.actions, 0);

    let events = trace_store
        .read(&run_id.to_string())
        .expect("trace records")
        .iter()
        .map(|record| serde_json::from_value::<TraceEvent>(record.payload.clone()).expect("event"))
        .collect::<Vec<_>>();
    let escalation = events
        .iter()
        .find_map(|event| match &event.kind {
            TraceEventKind::EscalationTriggered { escalation } => Some((event, escalation)),
            _ => None,
        })
        .expect("escalation trace");
    assert_eq!(escalation.1.trigger, EscalationTrigger::QuotaPressure);
    assert_eq!(escalation.1.threshold, 1);
    assert_eq!(escalation.1.scope, EscalationScope::Action);
    assert_eq!(escalation.1.decision, EscalationDecision::Pause);
    assert_eq!(escalation.1.run_id, run_id);
    assert_eq!(escalation.0.identity.action_id, escalation.1.action_id);
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionNeedsIntervention { .. })));
}

struct SingleActionPolicy;

impl Policy for SingleActionPolicy {
    fn name(&self) -> &str {
        "quota-policy"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        let action = Action {
            name: "quota".to_string(),
            params: serde_json::json!({"ok": true}),
            side_effect_class: SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let candidate = ActionCandidate::new(action).with_adapter("stub");
        let next_state = StateData {
            bytes: vec![2],
            content_type: Some("application/octet-stream".to_string()),
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

#[test]
fn loop_engine_denies_action_when_quota_exceeded() {
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let snapshot_policy = SnapshotPolicy::default();
    let state_graph = StateGraph::new(state_store, snapshot_policy);
    let initial_state = StateData {
        bytes: vec![1],
        content_type: None,
    };

    let tenant_id = splendor_kernel::TenantId::new();
    let agent_id = splendor_kernel::AgentId::new();
    let agent = AgentContext::new(
        agent_id.clone(),
        tenant_id.clone(),
        AgentRuntimeConfig::default(),
    );
    let registry = TenantRegistry::new();
    registry.insert(TenantContext::new(
        tenant_id.clone(),
        TenantPolicy {
            allowed_actions: vec!["quota".to_string()],
            allowed_adapters: vec!["stub".to_string()],
            allowed_permissions: Vec::new(),
        },
        QuotaPolicy {
            max_actions_per_tick: Some(0),
            ..QuotaPolicy::default()
        },
    ));
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    gateway.register_adapter("quota", "stub", Arc::new(StubAdapter));
    let gateway = Arc::new(gateway);

    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        state_graph,
        initial_state,
        Box::new(SingleActionPolicy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(EmptyPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert_eq!(outcome.action_outcomes.len(), 1);
    let action_outcome = &outcome.action_outcomes[0];
    assert!(matches!(
        action_outcome.status,
        splendor_gateway::ActionStatus::Denied
    ));
    assert!(action_outcome
        .verification
        .reasons
        .contains(&"max_actions_per_tick".to_string()));

    let records = trace_store
        .read(&run_id.to_string())
        .expect("trace records");
    let events = records
        .iter()
        .map(|record| serde_json::from_value::<TraceEvent>(record.payload.clone()).expect("event"))
        .collect::<Vec<_>>();
    let denied = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("action denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result.reasons.contains(&"max_actions_per_tick".to_string()));
        let quota = result.artifacts.get("quota").expect("quota artifacts");
        let context = quota.get("context").expect("context");
        assert!(context.get("agent_id").is_some());
        assert!(quota
            .get("actions_per_tick")
            .and_then(|value| value.get("limit"))
            .is_some());
    }

    let usage = QuotaUsage::single_action();
    assert_eq!(usage.actions, 1);
}
