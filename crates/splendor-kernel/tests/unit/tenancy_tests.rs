use super::*;
use crate::{SnapshotPolicy, StateGraph};
use splendor_store::{InMemoryStateStore, StateData, StateMetadata};
use splendor_types::{
    RevocationStatus, RunId, SideEffectClass, WorkOrder, WorkOrderId, WorkOrderPlacement,
    WorkOrderQuotaPolicy, WORK_ORDER_SCHEMA_VERSION,
};
use std::sync::Arc;

fn basic_tenant(quotas: QuotaPolicy) -> TenantContext {
    TenantContext::new(TenantId::new(), TenantPolicy::default(), quotas)
}

fn work_order_scope(tenant_id: TenantId, agent_id: AgentId) -> WorkOrder {
    let now = OffsetDateTime::now_utc();
    WorkOrder {
        schema_version: WORK_ORDER_SCHEMA_VERSION.to_string(),
        work_order_id: WorkOrderId::try_new("wo_tenancy").expect("work order id"),
        tenant_id,
        agent_id,
        run_id: Some(RunId::new()),
        objective: "narrow runtime scope".to_string(),
        allowed_actions: vec!["write".to_string()],
        allowed_adapters: vec!["filesystem".to_string()],
        allowed_permissions: vec!["fs:write".to_string()],
        data_refs: Vec::new(),
        quotas: WorkOrderQuotaPolicy {
            max_actions_per_tick: Some(1),
            max_filesystem_write_bytes: Some(10),
            ..WorkOrderQuotaPolicy::default()
        },
        placement: WorkOrderPlacement::default(),
        issued_at: now - Duration::minutes(1),
        expires_at: now + Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

#[test]
fn tenant_policy_default_denies_actions() {
    let policy = TenantPolicy::default();
    let empty: Vec<String> = Vec::new();
    let denied = policy.verify_action("write", None, &empty);
    assert!(!denied.allowed);
    assert!(denied.reasons.contains(&"action_not_allowed".to_string()));
}

#[test]
fn tenant_policy_verifies_action_permissions_and_adapters() {
    let policy = TenantPolicy {
        allowed_actions: vec!["write".to_string()],
        allowed_adapters: vec!["filesystem".to_string()],
        allowed_permissions: vec!["fs:write".to_string()],
    };
    let permissions = vec!["fs:write".to_string()];

    assert!(
        policy
            .verify_action("write", Some("filesystem"), &permissions)
            .allowed
    );

    let denied_action = policy.verify_action("read", Some("filesystem"), &permissions);
    assert!(!denied_action.allowed);
    assert!(denied_action
        .reasons
        .contains(&"action_not_allowed".to_string()));

    let denied_adapter = policy.verify_action("write", Some("http"), &permissions);
    assert!(!denied_adapter.allowed);
    assert!(denied_adapter
        .reasons
        .contains(&"adapter_not_allowed".to_string()));

    let denied_permissions =
        policy.verify_action("write", Some("filesystem"), &["fs:read".to_string()]);
    assert!(!denied_permissions.allowed);
    assert!(denied_permissions
        .reasons
        .contains(&"permission_denied".to_string()));
}

#[test]
fn work_order_scope_only_narrows_policy_and_quota() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let work_order = work_order_scope(tenant_id, agent_id);
    let policy = TenantPolicy {
        allowed_actions: vec!["write".to_string(), "delete".to_string()],
        allowed_adapters: vec!["filesystem".to_string(), "http".to_string()],
        allowed_permissions: vec!["fs:write".to_string(), "admin".to_string()],
    };

    let scoped = policy.constrain_to_work_order(&work_order);
    assert_eq!(scoped.allowed_actions, vec!["write".to_string()]);
    assert_eq!(scoped.allowed_adapters, vec!["filesystem".to_string()]);
    assert_eq!(scoped.allowed_permissions, vec!["fs:write".to_string()]);

    let quotas = QuotaPolicy {
        max_actions_per_tick: Some(5),
        filesystem: AdapterQuota {
            max_write_bytes: Some(100),
            ..AdapterQuota::default()
        },
        network: AdapterQuota {
            max_write_bytes: Some(200),
            ..AdapterQuota::default()
        },
        ..QuotaPolicy::default()
    };
    let scoped_quotas = quotas.constrain_to_work_order(&work_order);
    assert_eq!(scoped_quotas.max_actions_per_tick, Some(1));
    assert_eq!(scoped_quotas.filesystem.max_write_bytes, Some(10));
    assert_eq!(scoped_quotas.network.max_write_bytes, Some(200));
}

#[test]
fn agent_policy_denies_permission_laundering_between_agents() {
    let tenant_id = TenantId::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    let mut tenant = TenantContext::new(
        tenant_id,
        TenantPolicy {
            allowed_actions: vec!["write".to_string()],
            allowed_adapters: vec!["filesystem".to_string()],
            allowed_permissions: vec!["fs:read".to_string(), "fs:write".to_string()],
        },
        QuotaPolicy::default(),
    );
    tenant.register_agent_policy(
        agent_a.clone(),
        AgentIsolationPolicy {
            allowed_permissions: vec!["fs:read".to_string()],
            ..AgentIsolationPolicy::default()
        },
    );
    tenant.register_agent_policy(
        agent_b.clone(),
        AgentIsolationPolicy {
            allowed_permissions: vec!["fs:write".to_string()],
            ..AgentIsolationPolicy::default()
        },
    );
    let permissions = vec!["fs:write".to_string()];

    let denied = tenant.verify_action(&agent_a, "write", Some("filesystem"), &permissions);
    assert!(!denied.allowed);
    assert!(denied
        .reasons
        .contains(&"agent_permission_denied".to_string()));
    assert_eq!(
        denied.artifacts[AGENT_ISOLATION_LEDGER_SOURCE]["context"]["source"].as_str(),
        Some(AGENT_ISOLATION_LEDGER_SOURCE)
    );
    assert_eq!(
        denied.artifacts[AGENT_ISOLATION_LEDGER_SOURCE]["context"]["agent_id"].as_str(),
        Some(agent_a.to_string().as_str())
    );

    assert!(
        tenant
            .verify_action(&agent_b, "write", Some("filesystem"), &permissions)
            .allowed
    );
}

#[test]
fn missing_agent_policy_denies_permissioned_actions() {
    let agent = AgentId::new();
    let tenant = TenantContext::new(
        TenantId::new(),
        TenantPolicy {
            allowed_actions: vec!["write".to_string()],
            allowed_adapters: vec!["filesystem".to_string()],
            allowed_permissions: vec!["fs:write".to_string()],
        },
        QuotaPolicy::default(),
    );
    let denied = tenant.verify_action(
        &agent,
        "write",
        Some("filesystem"),
        &["fs:write".to_string()],
    );

    assert!(!denied.allowed);
    assert!(denied
        .reasons
        .contains(&"agent_isolation_profile_missing".to_string()));
}

#[test]
fn quota_actions_are_isolated_per_agent() {
    let quotas = QuotaPolicy {
        max_actions_per_tick: Some(1),
        ..QuotaPolicy::default()
    };
    let mut tenant = basic_tenant(quotas);
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    let now = OffsetDateTime::now_utc();

    tenant.begin_tick(1, now);
    let usage = QuotaUsage::single_action();
    assert!(tenant.record_usage(&agent_a, usage, now).allowed);

    let denied = tenant.record_usage(&agent_a, usage, now);
    assert!(!denied.allowed);
    assert!(denied.reasons.contains(&"max_actions_per_tick".to_string()));
    assert_eq!(
        denied.artifacts["context"]["source"].as_str(),
        Some(QUOTA_LEDGER_SOURCE)
    );

    assert!(tenant.record_usage(&agent_b, usage, now).allowed);

    let later = now + Duration::seconds(1);
    tenant.begin_tick(2, later);
    assert!(tenant.record_usage(&agent_a, usage, later).allowed);
}

#[test]
fn quota_denial_includes_context_artifacts() {
    let quotas = QuotaPolicy {
        max_actions_per_tick: Some(0),
        ..QuotaPolicy::default()
    };
    let mut tenant = basic_tenant(quotas);
    let agent = AgentId::new();
    let now = OffsetDateTime::now_utc();

    tenant.begin_tick(1, now);
    let denied = tenant.record_usage(&agent, QuotaUsage::single_action(), now);
    assert!(!denied.allowed);

    let tenant_id = tenant.tenant_id.to_string();
    let agent_id = agent.to_string();
    let context = denied
        .artifacts
        .as_object()
        .and_then(|value| value.get("context"))
        .and_then(|value| value.as_object())
        .expect("context");

    assert_eq!(
        context.get("tenant_id").and_then(|value| value.as_str()),
        Some(tenant_id.as_str())
    );
    assert_eq!(
        context.get("agent_id").and_then(|value| value.as_str()),
        Some(agent_id.as_str())
    );
    assert_eq!(
        context.get("tick_id").and_then(|value| value.as_u64()),
        Some(1)
    );
}

#[test]
fn quota_enforces_http_requests_per_minute() {
    let quotas = QuotaPolicy {
        max_http_requests_per_minute: Some(2),
        ..QuotaPolicy::default()
    };
    let mut tenant = basic_tenant(quotas);
    let agent = AgentId::new();
    let now = OffsetDateTime::now_utc();

    tenant.begin_tick(1, now);
    let usage = QuotaUsage {
        actions: 1,
        http_requests: 1,
        ..QuotaUsage::default()
    };

    assert!(tenant.record_usage(&agent, usage, now).allowed);
    assert!(tenant.record_usage(&agent, usage, now).allowed);

    let denied = tenant.record_usage(&agent, usage, now);
    assert!(!denied.allowed);
    assert!(denied
        .reasons
        .contains(&"max_http_requests_per_minute".to_string()));

    let later = now + Duration::seconds(61);
    tenant.begin_tick(2, later);
    assert!(tenant.record_usage(&agent, usage, later).allowed);
}

#[test]
fn quota_enforces_byte_limits() {
    let quotas = QuotaPolicy {
        filesystem: AdapterQuota {
            max_read_bytes: Some(100),
            max_write_bytes: Some(50),
        },
        network: AdapterQuota {
            max_read_bytes: Some(80),
            max_write_bytes: Some(40),
        },
        ..QuotaPolicy::default()
    };
    let mut tenant = basic_tenant(quotas);
    let agent = AgentId::new();
    let now = OffsetDateTime::now_utc();

    tenant.begin_tick(1, now);
    let first = QuotaUsage {
        actions: 1,
        filesystem_read_bytes: 60,
        filesystem_write_bytes: 40,
        network_read_bytes: 50,
        network_write_bytes: 20,
        ..QuotaUsage::default()
    };
    assert!(tenant.record_usage(&agent, first, now).allowed);

    let second = QuotaUsage {
        actions: 1,
        filesystem_read_bytes: 50,
        network_read_bytes: 40,
        ..QuotaUsage::default()
    };
    let denied = tenant.record_usage(&agent, second, now);
    assert!(!denied.allowed);
    assert!(denied
        .reasons
        .iter()
        .any(|reason| reason == "filesystem_read_bytes" || reason == "network_read_bytes"));
}

#[test]
fn quota_enforces_action_duration() {
    let quotas = QuotaPolicy {
        max_action_duration_ms: Some(120),
        ..QuotaPolicy::default()
    };
    let mut tenant = basic_tenant(quotas);
    let agent = AgentId::new();
    let now = OffsetDateTime::now_utc();

    tenant.begin_tick(1, now);
    let usage = QuotaUsage {
        actions: 1,
        action_duration_ms: 200,
        ..QuotaUsage::default()
    };
    let denied = tenant.record_usage(&agent, usage, now);
    assert!(!denied.allowed);
    assert!(denied
        .reasons
        .contains(&"max_action_duration_ms".to_string()));
}

#[test]
fn tenant_registry_denies_unknown_tenant() {
    let registry = TenantRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let action = Action {
        name: "noop".to_string(),
        params: serde_json::json!({}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };

    let policy = registry.verify_policy(&tenant_id, &agent_id, &action, Some("adapter"));
    assert!(!policy.allowed);
    assert!(policy.reasons.contains(&"tenant_not_found".to_string()));

    let quota = registry.verify_quota(&tenant_id, &agent_id, QuotaUsage::single_action());
    assert!(!quota.allowed);
    assert!(quota.reasons.contains(&"tenant_not_found".to_string()));
}

#[test]
fn quota_http_window_rolls_without_new_tick() {
    let quotas = QuotaPolicy {
        max_http_requests_per_minute: Some(1),
        ..QuotaPolicy::default()
    };
    let mut tenant = basic_tenant(quotas);
    let agent = AgentId::new();
    let now = OffsetDateTime::now_utc();

    tenant.begin_tick(1, now);
    let usage = QuotaUsage {
        actions: 1,
        http_requests: 1,
        ..QuotaUsage::default()
    };
    assert!(tenant.record_usage(&agent, usage, now).allowed);

    let later = now + Duration::seconds(61);
    assert!(tenant.record_usage(&agent, usage, later).allowed);
}

#[test]
fn agent_context_tracks_interpreter_and_head() {
    let mut agent = AgentContext::new(
        AgentId::new(),
        TenantId::new(),
        AgentRuntimeConfig::default(),
    );
    agent.attach_interpreter("python".to_string());
    assert_eq!(agent.interpreter_handles.len(), 1);

    let store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(store, SnapshotPolicy::default());
    let commit = graph
        .commit(
            StateData {
                bytes: vec![1],
                content_type: None,
            },
            StateMetadata {
                created_at: OffsetDateTime::now_utc(),
                label: None,
                tenant_id: None,
                agent_id: None,
                run_id: None,
                trace_event_id: None,
            },
        )
        .expect("commit");
    let node_id = commit.node_id;
    agent.set_state_head(node_id.clone());
    assert_eq!(agent.state_head, Some(node_id));
}

#[test]
fn agent_context_delegated_authority_allows_only_explicit_scope() {
    let agent = AgentContext::new(
        AgentId::new(),
        TenantId::new(),
        AgentRuntimeConfig::default(),
    )
    .with_delegated_authority(splendor_types::DelegatedAuthority {
        allowed_actions: vec!["query".to_string()],
        allowed_adapters: vec!["sql".to_string()],
        allowed_permissions: vec!["finance.read".to_string()],
    });
    let allowed = Action {
        name: "query".to_string(),
        params: serde_json::json!({}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: vec!["finance.read".to_string()],
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    assert!(agent.verify_delegated_action(&allowed, Some("sql")).allowed);

    let denied = Action {
        name: "publish".to_string(),
        params: serde_json::json!({}),
        side_effect_class: SideEffectClass::External,
        cost_estimate: None,
        required_permissions: vec!["artifact.publish".to_string()],
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let result = agent.verify_delegated_action(&denied, Some("artifact"));
    assert!(!result.allowed);
    assert!(result
        .reasons
        .contains(&"delegated_action_not_allowed".to_string()));
}
