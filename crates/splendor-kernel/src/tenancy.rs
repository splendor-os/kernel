//! # Tenancy and Quotas
//!
//! Tenant and agent contexts model isolation boundaries and quota enforcement.
//! The quota ledger tracks per-tick usage per agent and ensures one agent cannot
//! spend another agent's local runtime budget before actions are executed.

use splendor_gateway::TenantAccess;
use splendor_store::StateNodeId;
use splendor_types::{Action, AgentId, QuotaUsage, TenantId, VerificationResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

/// Structured source label embedded in agent-isolation denial artifacts.
pub const AGENT_ISOLATION_LEDGER_SOURCE: &str = "agent_isolation_ledger";
/// Structured source label embedded in quota-denial artifacts.
pub const QUOTA_LEDGER_SOURCE: &str = "quota_ledger";

/// Policy describing what a tenant is allowed to do.
#[derive(Clone, Debug, Default)]
pub struct TenantPolicy {
    /// Explicit action names that are permitted.
    pub allowed_actions: Vec<String>,
    /// Adapter identifiers that may be used.
    pub allowed_adapters: Vec<String>,
    /// Permission tokens granted to the tenant.
    pub allowed_permissions: Vec<String>,
}

/// Agent-scoped permission and message grants.
///
/// The tenant policy remains the upper bound for action names, adapters, and
/// tenant-wide permission tokens. This profile is the narrower agent runtime
/// context ledger used by 0.02-S3 to prevent one local agent from spending or
/// inheriting another agent's permissions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentIsolationPolicy {
    /// Permission tokens explicitly granted to this agent runtime context.
    pub allowed_permissions: Vec<String>,
    /// Message schemas this agent may send through the local router.
    pub allowed_message_schemas: Vec<String>,
    /// Local recipients this agent may address through the local router.
    pub allowed_message_recipients: Vec<AgentId>,
}

impl AgentIsolationPolicy {
    /// Verifies the permission subset for an action proposed by this agent.
    pub fn verify_action_permissions(
        &self,
        tenant_id: &TenantId,
        agent_id: &AgentId,
        required_permissions: &[String],
    ) -> VerificationResult {
        if required_permissions.is_empty() {
            return VerificationResult::allow();
        }

        let missing = required_permissions
            .iter()
            .filter(|permission| !allowlisted(&self.allowed_permissions, permission))
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return VerificationResult::allow();
        }

        VerificationResult {
            allowed: false,
            reasons: vec!["agent_permission_denied".to_string()],
            artifacts: serde_json::json!({
                "context": agent_ledger_context(tenant_id, agent_id),
                "permissions": {
                    "required": required_permissions,
                    "missing": missing,
                    "allowed": &self.allowed_permissions,
                }
            }),
        }
    }

    /// Verifies that this agent may send a schema to a local recipient.
    pub fn verify_message(
        &self,
        source_agent_id: &AgentId,
        target_agent_id: &AgentId,
        schema: &str,
    ) -> VerificationResult {
        let mut reasons = Vec::new();
        let mut artifacts = serde_json::Map::new();

        if !allowlisted(&self.allowed_message_schemas, schema) {
            reasons.push("message_schema_not_allowed".to_string());
            artifacts.insert(
                "schema".to_string(),
                serde_json::json!({
                    "requested": schema,
                    "allowed": &self.allowed_message_schemas,
                }),
            );
        }

        if !self
            .allowed_message_recipients
            .iter()
            .any(|allowed| allowed == target_agent_id)
        {
            reasons.push("message_recipient_not_allowed".to_string());
            artifacts.insert(
                "recipient".to_string(),
                serde_json::json!({
                    "requested": target_agent_id.to_string(),
                    "allowed": self
                        .allowed_message_recipients
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                }),
            );
        }

        if reasons.is_empty() {
            return VerificationResult::allow();
        }

        artifacts.insert(
            "context".to_string(),
            serde_json::json!({
                "source": AGENT_ISOLATION_LEDGER_SOURCE,
                "agent_id": source_agent_id.to_string(),
                "target_agent_id": target_agent_id.to_string(),
                "schema": schema,
            }),
        );

        VerificationResult {
            allowed: false,
            reasons,
            artifacts: serde_json::Value::Object(artifacts),
        }
    }
}

impl TenantPolicy {
    /// Verifies action access against allowlists and permissions.
    pub fn verify_action(
        &self,
        action_name: &str,
        adapter: Option<&str>,
        required_permissions: &[String],
    ) -> VerificationResult {
        let mut reasons = Vec::new();
        let mut artifacts = serde_json::Map::new();

        if !allowlisted(&self.allowed_actions, action_name) {
            reasons.push("action_not_allowed".to_string());
            artifacts.insert(
                "action".to_string(),
                serde_json::json!({
                    "requested": action_name,
                    "allowed": &self.allowed_actions,
                }),
            );
        }

        if let Some(adapter) = adapter {
            if !allowlisted(&self.allowed_adapters, adapter) {
                reasons.push("adapter_not_allowed".to_string());
                artifacts.insert(
                    "adapter".to_string(),
                    serde_json::json!({
                        "requested": adapter,
                        "allowed": &self.allowed_adapters,
                    }),
                );
            }
        }

        if !required_permissions.is_empty() {
            let missing = required_permissions
                .iter()
                .filter(|permission| !allowlisted(&self.allowed_permissions, permission))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                reasons.push("permission_denied".to_string());
                artifacts.insert(
                    "permissions".to_string(),
                    serde_json::json!({
                        "required": required_permissions,
                        "missing": missing,
                        "allowed": &self.allowed_permissions,
                    }),
                );
            }
        }

        if reasons.is_empty() {
            return VerificationResult::allow();
        }

        VerificationResult {
            allowed: false,
            reasons,
            artifacts: serde_json::Value::Object(artifacts),
        }
    }
}

/// Read/write byte caps for a specific adapter category.
#[derive(Clone, Debug, Default)]
pub struct AdapterQuota {
    /// Maximum bytes read per tick.
    pub max_read_bytes: Option<u64>,
    /// Maximum bytes written per tick.
    pub max_write_bytes: Option<u64>,
}

/// Quota configuration applied per tenant.
#[derive(Clone, Debug, Default)]
pub struct QuotaPolicy {
    /// Maximum actions allowed per tick.
    pub max_actions_per_tick: Option<u32>,
    /// Maximum duration in milliseconds for a single action.
    pub max_action_duration_ms: Option<u64>,
    /// Filesystem adapter byte budgets.
    pub filesystem: AdapterQuota,
    /// Network adapter byte budgets.
    pub network: AdapterQuota,
    /// Maximum HTTP requests allowed per minute.
    pub max_http_requests_per_minute: Option<u32>,
}

/// Runtime configuration scoped to an agent instance.
#[derive(Clone, Debug, Default)]
pub struct AgentRuntimeConfig {
    /// Human-friendly label for the agent instance.
    pub label: Option<String>,
    /// Additional metadata tags.
    pub metadata: HashMap<String, String>,
    /// Agent-scoped permission and local-message grants.
    pub isolation: AgentIsolationPolicy,
}

/// Agent-level execution context bound to a tenant.
#[derive(Clone, Debug)]
pub struct AgentContext {
    /// Agent identifier.
    pub agent_id: AgentId,
    /// Tenant identifier that owns the agent.
    pub tenant_id: TenantId,
    /// Interpreter handles assigned to the agent.
    pub interpreter_handles: Vec<String>,
    /// Current state graph head node.
    pub state_head: Option<StateNodeId>,
    /// Runtime configuration for the agent.
    pub config: AgentRuntimeConfig,
}

impl AgentContext {
    /// Creates a new agent context with explicit identifiers.
    pub fn new(agent_id: AgentId, tenant_id: TenantId, config: AgentRuntimeConfig) -> Self {
        Self {
            agent_id,
            tenant_id,
            interpreter_handles: Vec::new(),
            state_head: None,
            config,
        }
    }

    /// Attaches an interpreter handle to the agent.
    pub fn attach_interpreter(&mut self, handle: impl Into<String>) {
        self.interpreter_handles.push(handle.into());
    }

    /// Updates the head pointer for the agent state graph.
    pub fn set_state_head(&mut self, state_node_id: StateNodeId) {
        self.state_head = Some(state_node_id);
    }
}

/// Tenant-scoped execution context with quota enforcement state.
#[derive(Debug)]
pub struct TenantContext {
    /// Tenant identifier for this context.
    pub tenant_id: TenantId,
    /// Policy defining permissions and allowlists.
    pub policy: TenantPolicy,
    /// Quota policy enforced by the kernel.
    pub quotas: QuotaPolicy,
    /// Agent-scoped permission/message profiles for local runtime contexts.
    agent_policies: HashMap<AgentId, AgentIsolationPolicy>,
    /// Usage ledger tracking quota consumption.
    ledger: QuotaLedger,
}

impl TenantContext {
    /// Creates a new tenant context and initializes its ledger.
    pub fn new(tenant_id: TenantId, policy: TenantPolicy, quotas: QuotaPolicy) -> Self {
        Self {
            tenant_id,
            policy,
            quotas,
            agent_policies: HashMap::new(),
            ledger: QuotaLedger::default(),
        }
    }

    /// Registers or replaces the agent isolation profile for a local agent.
    pub fn register_agent_policy(&mut self, agent_id: AgentId, policy: AgentIsolationPolicy) {
        self.agent_policies.insert(agent_id, policy);
    }

    /// Registers an agent context using its runtime isolation configuration.
    pub fn register_agent_context(&mut self, agent: &AgentContext) {
        self.register_agent_policy(agent.agent_id.clone(), agent.config.isolation.clone());
    }

    /// Resets per-tick quota counters.
    pub fn begin_tick(&mut self, tick_id: u64, now: OffsetDateTime) {
        self.ledger.begin_tick(tick_id, now);
    }

    /// Records quota usage for an agent and returns a verification result.
    pub fn record_usage(
        &mut self,
        agent_id: &AgentId,
        usage: QuotaUsage,
        now: OffsetDateTime,
    ) -> VerificationResult {
        self.ledger
            .record_usage(&self.quotas, &self.tenant_id, agent_id, usage, now)
    }

    /// Verifies action access against the tenant policy.
    pub fn verify_action(
        &self,
        agent_id: &AgentId,
        action_name: &str,
        adapter: Option<&str>,
        required_permissions: &[String],
    ) -> VerificationResult {
        let tenant_result = self
            .policy
            .verify_action(action_name, adapter, required_permissions);
        let agent_result = if required_permissions.is_empty() {
            VerificationResult::allow()
        } else {
            self.agent_policies
                .get(agent_id)
                .map(|policy| {
                    policy.verify_action_permissions(
                        &self.tenant_id,
                        agent_id,
                        required_permissions,
                    )
                })
                .unwrap_or_else(|| missing_agent_policy(&self.tenant_id, agent_id))
        };

        combine_policy_results(tenant_result, agent_result)
    }

    /// Returns the current tick identifier tracked by the ledger.
    pub fn current_tick(&self) -> u64 {
        self.ledger.tick_id
    }

    /// Returns the latest per-tick usage totals.
    pub fn tick_usage(&self) -> QuotaUsage {
        self.ledger.tick_usage
    }
}

/// Shared registry for tenant contexts.
#[derive(Clone, Debug, Default)]
pub struct TenantRegistry {
    inner: Arc<Mutex<HashMap<TenantId, TenantContext>>>,
}

impl TenantRegistry {
    /// Creates an empty tenant registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a tenant context.
    pub fn insert(&self, tenant: TenantContext) {
        let mut guard = self.inner.lock().expect("tenant registry lock");
        guard.insert(tenant.tenant_id.clone(), tenant);
    }

    /// Executes a read-only closure against a tenant context.
    pub fn with_tenant<R>(
        &self,
        tenant_id: &TenantId,
        f: impl FnOnce(&TenantContext) -> R,
    ) -> Option<R> {
        let guard = self.inner.lock().expect("tenant registry lock");
        guard.get(tenant_id).map(f)
    }

    /// Executes a mutable closure against a tenant context.
    pub fn with_tenant_mut<R>(
        &self,
        tenant_id: &TenantId,
        f: impl FnOnce(&mut TenantContext) -> R,
    ) -> Option<R> {
        let mut guard = self.inner.lock().expect("tenant registry lock");
        guard.get_mut(tenant_id).map(f)
    }

    /// Starts a new tick across all tenants.
    pub fn begin_tick(&self, tick_id: u64, now: OffsetDateTime) {
        let mut guard = self.inner.lock().expect("tenant registry lock");
        for tenant in guard.values_mut() {
            tenant.begin_tick(tick_id, now);
        }
    }
}

impl TenantAccess for TenantRegistry {
    fn verify_policy(
        &self,
        tenant_id: &TenantId,
        agent_id: &AgentId,
        action: &Action,
        adapter: Option<&str>,
    ) -> VerificationResult {
        self.with_tenant(tenant_id, |tenant| {
            tenant.verify_action(
                agent_id,
                &action.name,
                adapter,
                &action.required_permissions,
            )
        })
        .unwrap_or_else(|| VerificationResult::deny("tenant_not_found"))
    }

    fn verify_quota(
        &self,
        tenant_id: &TenantId,
        agent_id: &AgentId,
        usage: QuotaUsage,
    ) -> VerificationResult {
        self.with_tenant_mut(tenant_id, |tenant| {
            tenant.record_usage(agent_id, usage, OffsetDateTime::now_utc())
        })
        .unwrap_or_else(|| VerificationResult::deny("tenant_not_found"))
    }
}

/// Tracks per-tenant quota usage across agents.
#[derive(Debug)]
pub struct QuotaLedger {
    tick_id: u64,
    tick_started_at: OffsetDateTime,
    tick_usage: QuotaUsage,
    per_agent_usage: HashMap<AgentId, QuotaUsage>,
    http_window_start: OffsetDateTime,
    per_agent_http_requests: HashMap<AgentId, u32>,
}

impl Default for QuotaLedger {
    fn default() -> Self {
        let epoch = OffsetDateTime::UNIX_EPOCH;
        Self {
            tick_id: 0,
            tick_started_at: epoch,
            tick_usage: QuotaUsage::default(),
            per_agent_usage: HashMap::new(),
            http_window_start: epoch,
            per_agent_http_requests: HashMap::new(),
        }
    }
}

impl QuotaLedger {
    /// Resets tick counters for a new tick.
    pub fn begin_tick(&mut self, tick_id: u64, now: OffsetDateTime) {
        self.tick_id = tick_id;
        self.tick_started_at = now;
        self.tick_usage = QuotaUsage::default();
        self.per_agent_usage.clear();
    }

    /// Records usage for a tenant and returns a verification result.
    pub fn record_usage(
        &mut self,
        policy: &QuotaPolicy,
        tenant_id: &TenantId,
        agent_id: &AgentId,
        usage: QuotaUsage,
        now: OffsetDateTime,
    ) -> VerificationResult {
        self.roll_http_window(now);
        let mut reasons = Vec::new();
        let mut artifacts = serde_json::Map::new();
        let current_agent_usage = self
            .per_agent_usage
            .get(agent_id)
            .copied()
            .unwrap_or_default();

        if let Some(limit) = policy.max_action_duration_ms {
            if usage.action_duration_ms > limit {
                reasons.push("max_action_duration_ms".to_string());
                artifacts.insert(
                    "action_duration_ms".to_string(),
                    serde_json::json!({"limit": limit, "actual": usage.action_duration_ms}),
                );
            }
        }

        if let Some(limit) = policy.max_actions_per_tick {
            let next_total = current_agent_usage.actions.saturating_add(usage.actions);
            if next_total > limit {
                reasons.push("max_actions_per_tick".to_string());
                artifacts.insert(
                    "actions_per_tick".to_string(),
                    serde_json::json!({"limit": limit, "current": current_agent_usage.actions, "requested": usage.actions}),
                );
            }
        }

        check_bytes_quota(
            "filesystem_read_bytes",
            policy.filesystem.max_read_bytes,
            current_agent_usage.filesystem_read_bytes,
            usage.filesystem_read_bytes,
            &mut reasons,
            &mut artifacts,
        );
        check_bytes_quota(
            "filesystem_write_bytes",
            policy.filesystem.max_write_bytes,
            current_agent_usage.filesystem_write_bytes,
            usage.filesystem_write_bytes,
            &mut reasons,
            &mut artifacts,
        );
        check_bytes_quota(
            "network_read_bytes",
            policy.network.max_read_bytes,
            current_agent_usage.network_read_bytes,
            usage.network_read_bytes,
            &mut reasons,
            &mut artifacts,
        );
        check_bytes_quota(
            "network_write_bytes",
            policy.network.max_write_bytes,
            current_agent_usage.network_write_bytes,
            usage.network_write_bytes,
            &mut reasons,
            &mut artifacts,
        );

        if let Some(limit) = policy.max_http_requests_per_minute {
            let current = self
                .per_agent_http_requests
                .get(agent_id)
                .copied()
                .unwrap_or_default();
            let next_total = current.saturating_add(usage.http_requests);
            if next_total > limit {
                reasons.push("max_http_requests_per_minute".to_string());
                artifacts.insert(
                    "http_requests_per_minute".to_string(),
                    serde_json::json!({
                        "limit": limit,
                        "current": current,
                        "requested": usage.http_requests,
                        "window_start": self.http_window_start.unix_timestamp(),
                    }),
                );
            }
        }

        if !reasons.is_empty() {
            artifacts.insert(
                "context".to_string(),
                serde_json::json!({
                    "source": QUOTA_LEDGER_SOURCE,
                    "tenant_id": tenant_id.to_string(),
                    "agent_id": agent_id.to_string(),
                    "tick_id": self.tick_id,
                    "tick_started_at": self.tick_started_at.unix_timestamp(),
                }),
            );
            return VerificationResult {
                allowed: false,
                reasons,
                artifacts: serde_json::Value::Object(artifacts),
            };
        }

        self.tick_usage.accumulate(usage);
        let entry = self.per_agent_usage.entry(agent_id.clone()).or_default();
        entry.accumulate(usage);
        let http_entry = self
            .per_agent_http_requests
            .entry(agent_id.clone())
            .or_default();
        *http_entry = http_entry.saturating_add(usage.http_requests);

        VerificationResult::allow()
    }

    fn roll_http_window(&mut self, now: OffsetDateTime) {
        if now - self.http_window_start >= Duration::minutes(1) {
            self.http_window_start = now;
            self.per_agent_http_requests.clear();
        }
    }
}

fn agent_ledger_context(tenant_id: &TenantId, agent_id: &AgentId) -> serde_json::Value {
    serde_json::json!({
        "source": AGENT_ISOLATION_LEDGER_SOURCE,
        "tenant_id": tenant_id.to_string(),
        "agent_id": agent_id.to_string(),
    })
}

fn missing_agent_policy(tenant_id: &TenantId, agent_id: &AgentId) -> VerificationResult {
    VerificationResult {
        allowed: false,
        reasons: vec!["agent_isolation_profile_missing".to_string()],
        artifacts: serde_json::json!({
            "context": agent_ledger_context(tenant_id, agent_id),
        }),
    }
}

fn combine_policy_results(
    tenant_result: VerificationResult,
    agent_result: VerificationResult,
) -> VerificationResult {
    if tenant_result.allowed && agent_result.allowed {
        return VerificationResult::allow();
    }

    let mut reasons = Vec::new();
    let mut artifacts = serde_json::Map::new();
    if !tenant_result.allowed {
        reasons.extend(tenant_result.reasons);
        if !tenant_result.artifacts.is_null() {
            artifacts.insert("tenant_policy".to_string(), tenant_result.artifacts);
        }
    }
    if !agent_result.allowed {
        reasons.extend(agent_result.reasons);
        if !agent_result.artifacts.is_null() {
            artifacts.insert(
                AGENT_ISOLATION_LEDGER_SOURCE.to_string(),
                agent_result.artifacts,
            );
        }
    }

    VerificationResult {
        allowed: false,
        reasons,
        artifacts: serde_json::Value::Object(artifacts),
    }
}

fn allowlisted(allowlist: &[String], value: &str) -> bool {
    if allowlist.is_empty() {
        return false;
    }
    allowlist.iter().any(|item| item == value)
}

fn check_bytes_quota(
    label: &str,
    limit: Option<u64>,
    current: u64,
    requested: u64,
    reasons: &mut Vec<String>,
    artifacts: &mut serde_json::Map<String, serde_json::Value>,
) {
    if let Some(limit) = limit {
        let next_total = current.saturating_add(requested);
        if next_total > limit {
            reasons.push(label.to_string());
            artifacts.insert(
                label.to_string(),
                serde_json::json!({"limit": limit, "current": current, "requested": requested}),
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/tenancy_tests.rs"]
mod tests;
