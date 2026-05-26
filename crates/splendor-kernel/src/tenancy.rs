//! # Tenancy and Quotas
//!
//! Tenant and agent contexts model isolation boundaries and quota enforcement.
//! The quota ledger tracks per-tick usage across agents and ensures limits are
//! respected before actions are executed.

use splendor_gateway::TenantAccess;
use splendor_store::StateNodeId;
use splendor_types::{Action, AgentId, QuotaUsage, TenantId, VerificationResult, WorkOrder};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

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

    /// Narrows this tenant policy to the authority delegated by a validated work
    /// order. Intersections intentionally never broaden tenant permissions.
    pub fn constrain_to_work_order(&self, work_order: &WorkOrder) -> Self {
        Self {
            allowed_actions: intersect_allowlists(
                &self.allowed_actions,
                &work_order.allowed_actions,
            ),
            allowed_adapters: intersect_allowlists(
                &self.allowed_adapters,
                &work_order.allowed_adapters,
            ),
            allowed_permissions: intersect_allowlists(
                &self.allowed_permissions,
                &work_order.allowed_permissions,
            ),
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

impl QuotaPolicy {
    /// Narrows this quota policy to work-order limits. Missing work-order limits
    /// leave existing tenant limits intact; present work-order limits choose the
    /// smaller value so scoped delegation cannot increase quota.
    pub fn constrain_to_work_order(&self, work_order: &WorkOrder) -> Self {
        let quotas = &work_order.quotas;
        Self {
            max_actions_per_tick: min_optional(
                self.max_actions_per_tick,
                quotas.max_actions_per_tick,
            ),
            max_action_duration_ms: min_optional(
                self.max_action_duration_ms,
                quotas.max_action_duration_ms,
            ),
            filesystem: AdapterQuota {
                max_read_bytes: min_optional(
                    self.filesystem.max_read_bytes,
                    quotas.max_filesystem_read_bytes,
                ),
                max_write_bytes: min_optional(
                    self.filesystem.max_write_bytes,
                    quotas.max_filesystem_write_bytes,
                ),
            },
            network: AdapterQuota {
                max_read_bytes: min_optional(
                    self.network.max_read_bytes,
                    quotas.max_network_read_bytes,
                ),
                max_write_bytes: min_optional(
                    self.network.max_write_bytes,
                    quotas.max_network_write_bytes,
                ),
            },
            max_http_requests_per_minute: min_optional(
                self.max_http_requests_per_minute,
                quotas.max_http_requests_per_minute,
            ),
        }
    }
}

/// Runtime configuration scoped to an agent instance.
#[derive(Clone, Debug, Default)]
pub struct AgentRuntimeConfig {
    /// Human-friendly label for the agent instance.
    pub label: Option<String>,
    /// Additional metadata tags.
    pub metadata: HashMap<String, String>,
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
            ledger: QuotaLedger::default(),
        }
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
        action_name: &str,
        adapter: Option<&str>,
        required_permissions: &[String],
    ) -> VerificationResult {
        self.policy
            .verify_action(action_name, adapter, required_permissions)
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
        action: &Action,
        adapter: Option<&str>,
    ) -> VerificationResult {
        self.with_tenant(tenant_id, |tenant| {
            tenant.verify_action(&action.name, adapter, &action.required_permissions)
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
    http_requests: u32,
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
            http_requests: 0,
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
            let next_total = self.tick_usage.actions.saturating_add(usage.actions);
            if next_total > limit {
                reasons.push("max_actions_per_tick".to_string());
                artifacts.insert(
                    "actions_per_tick".to_string(),
                    serde_json::json!({"limit": limit, "current": self.tick_usage.actions, "requested": usage.actions}),
                );
            }
        }

        check_bytes_quota(
            "filesystem_read_bytes",
            policy.filesystem.max_read_bytes,
            self.tick_usage.filesystem_read_bytes,
            usage.filesystem_read_bytes,
            &mut reasons,
            &mut artifacts,
        );
        check_bytes_quota(
            "filesystem_write_bytes",
            policy.filesystem.max_write_bytes,
            self.tick_usage.filesystem_write_bytes,
            usage.filesystem_write_bytes,
            &mut reasons,
            &mut artifacts,
        );
        check_bytes_quota(
            "network_read_bytes",
            policy.network.max_read_bytes,
            self.tick_usage.network_read_bytes,
            usage.network_read_bytes,
            &mut reasons,
            &mut artifacts,
        );
        check_bytes_quota(
            "network_write_bytes",
            policy.network.max_write_bytes,
            self.tick_usage.network_write_bytes,
            usage.network_write_bytes,
            &mut reasons,
            &mut artifacts,
        );

        if let Some(limit) = policy.max_http_requests_per_minute {
            let next_total = self.http_requests.saturating_add(usage.http_requests);
            if next_total > limit {
                reasons.push("max_http_requests_per_minute".to_string());
                artifacts.insert(
                    "http_requests_per_minute".to_string(),
                    serde_json::json!({
                        "limit": limit,
                        "current": self.http_requests,
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
        self.http_requests = self.http_requests.saturating_add(usage.http_requests);

        VerificationResult::allow()
    }

    fn roll_http_window(&mut self, now: OffsetDateTime) {
        if now - self.http_window_start >= Duration::minutes(1) {
            self.http_window_start = now;
            self.http_requests = 0;
        }
    }
}

fn allowlisted(allowlist: &[String], value: &str) -> bool {
    if allowlist.is_empty() {
        return false;
    }
    allowlist.iter().any(|item| item == value)
}

fn intersect_allowlists(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|item| right.iter().any(|candidate| candidate == *item))
        .cloned()
        .collect()
}

fn min_optional<T: Ord + Copy>(tenant_limit: Option<T>, work_order_limit: Option<T>) -> Option<T> {
    match (tenant_limit, work_order_limit) {
        (Some(tenant), Some(work_order)) => Some(tenant.min(work_order)),
        (Some(tenant), None) => Some(tenant),
        (None, Some(work_order)) => Some(work_order),
        (None, None) => None,
    }
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
