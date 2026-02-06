use super::*;
use crate::loop_engine::{
    ActionCandidate, AllowAllConstraintEngine, LoopEngine, LoopError, Policy, PolicyDecision,
};
use crate::{
    AgentContext, KernelRuntime, KernelRuntimeConfig, SnapshotPolicy, TenantRegistry, TraceError,
    TraceSink,
};
use splendor_gateway::{
    ActionAdapter, ActionGateway, ActionRequest, ActionStatus, AdapterError, AdapterResult,
    VerifiedActionGateway,
};
use splendor_store::{InMemoryStateStore, StateData};
use splendor_types::{Action, TraceEvent};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

struct StaticPolicy {
    action_name: String,
    next_state: Vec<u8>,
}

impl Policy for StaticPolicy {
    fn name(&self) -> &str {
        "static"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[splendor_types::Percept],
    ) -> Result<PolicyDecision, LoopError> {
        let action = Action {
            name: self.action_name.clone(),
            params: serde_json::json!({}),
            side_effect_class: splendor_types::SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let candidate = ActionCandidate::new(action);
        let state = StateData {
            bytes: self.next_state.clone(),
            content_type: None,
        };
        Ok(PolicyDecision::new(vec![candidate], state, None))
    }
}

struct SlowPolicy {
    action_name: String,
    next_state: Vec<u8>,
    delay: Duration,
}

impl Policy for SlowPolicy {
    fn name(&self) -> &str {
        "slow"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[splendor_types::Percept],
    ) -> Result<PolicyDecision, LoopError> {
        sleep(self.delay);
        let action = Action {
            name: self.action_name.clone(),
            params: serde_json::json!({}),
            side_effect_class: splendor_types::SideEffectClass::ReadOnly,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        };
        let candidate = ActionCandidate::new(action);
        let state = StateData {
            bytes: self.next_state.clone(),
            content_type: None,
        };
        Ok(PolicyDecision::new(vec![candidate], state, None))
    }
}

struct FailPolicy;

impl Policy for FailPolicy {
    fn name(&self) -> &str {
        "fail"
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[splendor_types::Percept],
    ) -> Result<PolicyDecision, LoopError> {
        Err(LoopError::Policy("failed".to_string()))
    }
}

#[derive(Default)]
struct TestAdapter;

impl ActionAdapter for TestAdapter {
    fn execute(&self, action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
        Ok(AdapterResult {
            output: serde_json::json!({"ok": true}),
            satisfied_postconditions: action.action.postconditions.clone(),
        })
    }
}

#[derive(Default)]
struct NullTraceSink;

impl TraceSink for NullTraceSink {
    fn record(&self, _event: &TraceEvent) -> Result<(), TraceError> {
        Ok(())
    }
}

fn build_engine(
    tenant_id: TenantId,
    action_name: &str,
    next_state: &[u8],
    gateway: Arc<dyn ActionGateway>,
) -> LoopEngine {
    let policy = StaticPolicy {
        action_name: action_name.to_string(),
        next_state: next_state.to_vec(),
    };
    build_engine_with_policy(tenant_id, Box::new(policy), gateway)
}

fn build_engine_with_policy(
    tenant_id: TenantId,
    policy: Box<dyn Policy>,
    gateway: Arc<dyn ActionGateway>,
) -> LoopEngine {
    let store = Arc::new(InMemoryStateStore::default());
    let graph = crate::StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![0],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        tenant_id,
        crate::AgentRuntimeConfig::default(),
    );
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(NullTraceSink),
        ..KernelRuntimeConfig::default()
    });
    let mut engine =
        LoopEngine::with_runtime(agent, graph, initial_state, policy, gateway, runtime);
    engine.set_constraint_engine(AllowAllConstraintEngine);
    engine
}

#[test]
fn scheduler_runs_agents_in_order_and_enforces_tenant_quotas() {
    let tenant_id = TenantId::new();
    let policy = crate::TenantPolicy {
        allowed_actions: vec!["alpha".to_string(), "beta".to_string()],
        allowed_adapters: vec!["adapter".to_string()],
        ..crate::TenantPolicy::default()
    };
    let quotas = crate::QuotaPolicy {
        max_actions_per_tick: Some(1),
        ..crate::QuotaPolicy::default()
    };
    let tenant = crate::TenantContext::new(tenant_id.clone(), policy, quotas);

    let registry = TenantRegistry::new();
    registry.insert(tenant);

    let tenant_access = Arc::new(registry.clone());
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("alpha", "adapter", Arc::new(TestAdapter));
    gateway.register_adapter("beta", "adapter", Arc::new(TestAdapter));
    let gateway = Arc::new(gateway);

    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);

    let engine_one = build_engine(tenant_id.clone(), "alpha", &[1], gateway.clone());
    let engine_two = build_engine(tenant_id.clone(), "beta", &[2], gateway);

    scheduler.add_agent(engine_one);
    scheduler.add_agent(engine_two);

    let steps = scheduler.run_cycle().expect("cycle");
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].tick_id, 1);
    assert_eq!(steps[1].tick_id, 1);

    let first_status = &steps[0].outcome.action_outcomes[0].status;
    let second_status = &steps[1].outcome.action_outcomes[0].status;
    assert!(matches!(first_status, ActionStatus::Executed));
    assert!(matches!(second_status, ActionStatus::Denied));

    let usage = scheduler.tenant_usage(&tenant_id).expect("usage");
    assert_eq!(usage.actions, 1);
}

#[test]
fn scheduler_reports_tick_budget_exceeded() {
    let tenant_id = TenantId::new();
    let policy = crate::TenantPolicy {
        allowed_actions: vec!["slow".to_string()],
        allowed_adapters: vec!["adapter".to_string()],
        ..crate::TenantPolicy::default()
    };
    let tenant =
        crate::TenantContext::new(tenant_id.clone(), policy, crate::QuotaPolicy::default());
    let registry = TenantRegistry::new();
    registry.insert(tenant);

    let tenant_access = Arc::new(registry.clone());
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("slow", "adapter", Arc::new(TestAdapter));
    let gateway = Arc::new(gateway);

    let config = SchedulerConfig {
        tick_budget: Some(Duration::from_millis(1)),
        ..SchedulerConfig::default()
    };
    let mut scheduler = Scheduler::with_registry(config, registry);

    let policy = SlowPolicy {
        action_name: "slow".to_string(),
        next_state: vec![3],
        delay: Duration::from_millis(5),
    };
    let engine = build_engine_with_policy(tenant_id, Box::new(policy), gateway);
    scheduler.add_agent(engine);

    let error = scheduler.run_once().expect_err("budget exceeded");
    match error {
        SchedulerError::TickBudgetExceeded {
            step,
            budget,
            elapsed,
        } => {
            assert_eq!(step.tick_id, 1);
            assert!(elapsed >= budget);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn scheduler_returns_no_agents_when_empty() {
    let mut scheduler = Scheduler::new(SchedulerConfig::default());
    let error = scheduler.run_once().expect_err("no agents");
    assert!(matches!(error, SchedulerError::NoAgents));
    let error = scheduler.run_cycle().expect_err("no agents");
    assert!(matches!(error, SchedulerError::NoAgents));
}

#[test]
fn scheduler_returns_missing_tenant() {
    let tenant_id = TenantId::new();
    let registry = TenantRegistry::new();

    let tenant_access = Arc::new(registry.clone());
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("noop", "adapter", Arc::new(TestAdapter));
    let gateway = Arc::new(gateway);

    let engine = build_engine(tenant_id.clone(), "noop", &[1], gateway);
    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);
    scheduler.add_agent(engine);

    let error = scheduler.run_once().expect_err("missing tenant");
    match error {
        SchedulerError::MissingTenant(id) => assert_eq!(id, tenant_id),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn scheduler_reports_loop_error() {
    let tenant_id = TenantId::new();
    let policy = crate::TenantPolicy {
        allowed_actions: vec!["noop".to_string()],
        allowed_adapters: vec!["adapter".to_string()],
        ..crate::TenantPolicy::default()
    };
    let tenant =
        crate::TenantContext::new(tenant_id.clone(), policy, crate::QuotaPolicy::default());
    let registry = TenantRegistry::new();
    registry.insert(tenant);

    let tenant_access = Arc::new(registry.clone());
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("noop", "adapter", Arc::new(TestAdapter));
    let gateway = Arc::new(gateway);

    let store = Arc::new(InMemoryStateStore::default());
    let graph = crate::StateGraph::new(store, SnapshotPolicy::default());
    let initial_state = StateData {
        bytes: vec![0],
        content_type: None,
    };
    let agent = AgentContext::new(
        splendor_types::AgentId::new(),
        tenant_id.clone(),
        crate::AgentRuntimeConfig::default(),
    );
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(NullTraceSink),
        ..KernelRuntimeConfig::default()
    });
    let engine = LoopEngine::with_runtime(
        agent,
        graph,
        initial_state,
        Box::new(FailPolicy),
        gateway,
        runtime,
    );

    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);
    scheduler.add_agent(engine);

    let error = scheduler.run_once().expect_err("loop error");
    assert!(matches!(error, SchedulerError::Loop(_)));
}

#[test]
fn scheduler_respects_tick_interval() {
    let tenant_id = TenantId::new();
    let policy = crate::TenantPolicy {
        allowed_actions: vec!["alpha".to_string()],
        allowed_adapters: vec!["adapter".to_string()],
        ..crate::TenantPolicy::default()
    };
    let tenant =
        crate::TenantContext::new(tenant_id.clone(), policy, crate::QuotaPolicy::default());
    let registry = TenantRegistry::new();
    registry.insert(tenant);

    let tenant_access = Arc::new(registry.clone());
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("alpha", "adapter", Arc::new(TestAdapter));
    let gateway = Arc::new(gateway);

    let mut scheduler = Scheduler::with_registry(
        SchedulerConfig {
            tick_budget: None,
            tick_interval: Some(Duration::from_millis(1)),
        },
        registry,
    );
    scheduler.add_agent(build_engine(tenant_id, "alpha", &[1], gateway));

    let steps = scheduler.run_cycle().expect("cycle");
    assert_eq!(steps.len(), 1);
}

#[test]
fn scheduler_run_cycles_returns_steps() {
    let tenant_id = TenantId::new();
    let policy = crate::TenantPolicy {
        allowed_actions: vec!["alpha".to_string()],
        allowed_adapters: vec!["adapter".to_string()],
        ..crate::TenantPolicy::default()
    };
    let tenant =
        crate::TenantContext::new(tenant_id.clone(), policy, crate::QuotaPolicy::default());
    let registry = TenantRegistry::new();
    registry.insert(tenant);

    let tenant_access = Arc::new(registry.clone());
    let mut gateway = VerifiedActionGateway::new(tenant_access);
    gateway.register_adapter("alpha", "adapter", Arc::new(TestAdapter));
    let gateway = Arc::new(gateway);

    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), registry);
    scheduler.add_agent(build_engine(tenant_id, "alpha", &[1], gateway));

    let steps = scheduler.run_cycles(2).expect("cycles");
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].tick_id, 1);
    assert_eq!(steps[1].tick_id, 2);
}

#[test]
fn scheduler_run_forever_returns_error_on_empty_queue() {
    let mut scheduler = Scheduler::new(SchedulerConfig::default());
    let error = scheduler.run_forever().expect_err("no agents");
    assert!(matches!(error, SchedulerError::NoAgents));
}

#[test]
fn scheduler_register_tenant_and_registry_access() {
    let tenant_id = TenantId::new();
    let tenant = crate::TenantContext::new(
        tenant_id.clone(),
        crate::TenantPolicy::default(),
        crate::QuotaPolicy::default(),
    );
    let mut scheduler = Scheduler::new(SchedulerConfig::default());
    scheduler.register_tenant(tenant);

    let registry = scheduler.tenant_registry();
    let found = registry.with_tenant(&tenant_id, |tenant| tenant.current_tick());
    assert_eq!(found, Some(0));
}
