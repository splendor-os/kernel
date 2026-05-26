//! # Fleet Telemetry Types
//!
//! Minimal operational telemetry contracts for 0.03-S8. These structures are
//! read-only projections over fleet heartbeats, instance reports, run status,
//! quota/denial signals, and trace-sync state. They intentionally do not grant
//! permissions, authorize work orders, or replace gateway verification.

use crate::{AgentId, FleetId, InstanceId, NodeId, QuotaUsage, RunId, TenantId, TraceId};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

/// Canonical schema tag for 0.03-S8 fleet telemetry snapshots.
pub const FLEET_TELEMETRY_SCHEMA_VERSION: &str = "splendor.fleet_telemetry.v1";

/// Health state derived from node heartbeat recency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeOnlineState {
    /// Heartbeat age is below the stale threshold.
    Online,
    /// Heartbeat age is at or above the stale threshold but below offline.
    Stale,
    /// No heartbeat exists, or heartbeat age is at or above offline.
    Offline,
}

impl NodeOnlineState {
    /// Derives node state from heartbeat data and explicit thresholds.
    pub fn from_heartbeat(
        last_heartbeat_at: Option<OffsetDateTime>,
        observed_at: OffsetDateTime,
        stale_after: Duration,
        offline_after: Duration,
    ) -> Self {
        let Some(last_heartbeat_at) = last_heartbeat_at else {
            return Self::Offline;
        };
        if last_heartbeat_at >= observed_at {
            return Self::Online;
        }

        let age = observed_at - last_heartbeat_at;
        if age >= offline_after {
            Self::Offline
        } else if age >= stale_after {
            Self::Stale
        } else {
            Self::Online
        }
    }

    /// Stable string form used in docs and serialized examples.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Stale => "stale",
            Self::Offline => "offline",
        }
    }
}

/// Runtime deployment mode for an instance report.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// One or a small number of scoped runs, then exit or return to pool.
    Ephemeral,
    /// Long-lived resident runtime process.
    Resident,
    /// Dedicated sensitive runtime boundary.
    Dedicated,
    /// Customer VPC runtime target.
    CustomerVpc,
    /// On-premises runtime target.
    OnPrem,
    /// Edge device runtime target.
    EdgeDevice,
    /// Physical robot or device runtime target.
    PhysicalRobot,
    /// Desktop sidecar runtime target.
    DesktopSidecar,
    /// Bounded custom mode label for future compatibility.
    Custom(String),
}

/// Canonical run lifecycle states for fleet telemetry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run is known but not yet executing.
    Pending,
    /// Run is actively executing.
    Running,
    /// Run is paused by lifecycle control.
    Paused,
    /// Run is waiting for approval.
    WaitingForApproval,
    /// Run was interrupted before completion.
    Interrupted,
    /// Run is resuming from prior state.
    Resuming,
    /// Run completed successfully.
    Completed,
    /// Run failed.
    Failed,
    /// Run was cancelled.
    Cancelled,
    /// Run was denied before or during execution.
    Denied,
    /// Run expired.
    Expired,
}

impl RunStatus {
    /// All canonical statuses required by Splendor0.03-dev failure handling.
    pub const ALL: [RunStatus; 11] = [
        RunStatus::Pending,
        RunStatus::Running,
        RunStatus::Paused,
        RunStatus::WaitingForApproval,
        RunStatus::Interrupted,
        RunStatus::Resuming,
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Denied,
        RunStatus::Expired,
    ];

    /// Stable string form used in rules and serialized examples.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::Interrupted => "interrupted",
            Self::Resuming => "resuming",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Denied => "denied",
            Self::Expired => "expired",
        }
    }
}

/// Count for one run status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunStatusCount {
    /// Canonical status being counted.
    pub status: RunStatus,
    /// Number of current runs in this status.
    pub count: u32,
}

/// Run counts grouped by canonical status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunStatusCounts {
    /// Counts in canonical `RunStatus::ALL` order.
    pub counts: Vec<RunStatusCount>,
}

impl RunStatusCounts {
    /// Creates zero counts for every canonical status.
    pub fn canonical_empty() -> Self {
        Self {
            counts: RunStatus::ALL
                .iter()
                .copied()
                .map(|status| RunStatusCount { status, count: 0 })
                .collect(),
        }
    }

    /// Counts an iterator of statuses while preserving canonical status rows.
    pub fn from_statuses(statuses: impl IntoIterator<Item = RunStatus>) -> Self {
        let mut counts = Self::canonical_empty();
        for status in statuses {
            counts.increment(status);
        }
        counts
    }

    /// Increments one status count.
    pub fn increment(&mut self, status: RunStatus) {
        if let Some(entry) = self.counts.iter_mut().find(|entry| entry.status == status) {
            entry.count = entry.count.saturating_add(1);
        } else {
            self.counts.push(RunStatusCount { status, count: 1 });
        }
    }

    /// Returns the count for a status, or zero if absent.
    pub fn count(&self, status: RunStatus) -> u32 {
        self.counts
            .iter()
            .find(|entry| entry.status == status)
            .map(|entry| entry.count)
            .unwrap_or(0)
    }
}

impl Default for RunStatusCounts {
    fn default() -> Self {
        Self::canonical_empty()
    }
}

/// Per-node telemetry derived from heartbeat data and associated instances.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeTelemetry {
    /// Fleet that owns this node telemetry projection.
    pub fleet_id: FleetId,
    /// Node identity distinct from instance, tenant, agent, and run IDs.
    pub node_id: NodeId,
    /// State derived from heartbeat age.
    pub online_state: NodeOnlineState,
    /// Last heartbeat timestamp when known.
    pub last_heartbeat_at: Option<OffsetDateTime>,
    /// Timestamp when the telemetry projection was observed.
    pub observed_at: OffsetDateTime,
    /// Instances currently associated with the node.
    pub instance_ids: Vec<InstanceId>,
}

impl NodeTelemetry {
    /// Builds node telemetry by classifying the heartbeat age.
    pub fn from_heartbeat(
        fleet_id: FleetId,
        node_id: NodeId,
        last_heartbeat_at: Option<OffsetDateTime>,
        observed_at: OffsetDateTime,
        stale_after: Duration,
        offline_after: Duration,
        instance_ids: Vec<InstanceId>,
    ) -> Self {
        Self {
            fleet_id,
            node_id,
            online_state: NodeOnlineState::from_heartbeat(
                last_heartbeat_at,
                observed_at,
                stale_after,
                offline_after,
            ),
            last_heartbeat_at,
            observed_at,
            instance_ids,
        }
    }
}

/// Per-instance runtime telemetry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstanceTelemetry {
    /// Node hosting this runtime instance.
    pub node_id: NodeId,
    /// Runtime instance identity.
    pub instance_id: InstanceId,
    /// Runtime build or semantic version label.
    pub runtime_version: String,
    /// Runtime deployment mode.
    pub runtime_mode: RuntimeMode,
    /// Capabilities advertised by this instance.
    pub capabilities: Vec<String>,
    /// Current run counts grouped by canonical run status.
    pub current_run_counts: RunStatusCounts,
    /// Timestamp when this instance report was updated.
    pub reported_at: OffsetDateTime,
}

impl InstanceTelemetry {
    /// Creates an instance report with zero run counts.
    pub fn new(
        node_id: NodeId,
        instance_id: InstanceId,
        runtime_version: impl Into<String>,
        runtime_mode: RuntimeMode,
        capabilities: Vec<String>,
        reported_at: OffsetDateTime,
    ) -> Self {
        Self {
            node_id,
            instance_id,
            runtime_version: runtime_version.into(),
            runtime_mode,
            capabilities,
            current_run_counts: RunStatusCounts::canonical_empty(),
            reported_at,
        }
    }
}

/// Per-run status projection used for fleet views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunTelemetry {
    /// Tenant boundary for the run.
    pub tenant_id: TenantId,
    /// Agent owning the run.
    pub agent_id: AgentId,
    /// Run identity.
    pub run_id: RunId,
    /// Node currently associated with this run.
    pub node_id: NodeId,
    /// Instance currently associated with this run.
    pub instance_id: InstanceId,
    /// Canonical run status.
    pub status: RunStatus,
    /// Timestamp when the status was updated.
    pub updated_at: OffsetDateTime,
}

/// Queue status for node/instance-local work buffers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueueTelemetry {
    /// Node identity for this queue projection.
    pub node_id: NodeId,
    /// Instance identity for this queue projection.
    pub instance_id: InstanceId,
    /// Number of queued run/work-order items.
    pub queued_runs: u32,
    /// Number of queued messages awaiting delivery or consumption.
    pub queued_messages: u32,
    /// Timestamp when queue telemetry was updated.
    pub updated_at: OffsetDateTime,
}

/// Failure categories surfaced by fleet telemetry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    /// Heartbeat crossed the stale threshold.
    HeartbeatStale,
    /// Heartbeat crossed the offline threshold or was missing.
    HeartbeatOffline,
    /// Runtime instance is unavailable.
    InstanceUnavailable,
    /// A run entered the failed state.
    RunFailed,
    /// Quota verifier denied or constrained work.
    QuotaDenied,
    /// Non-quota verifier denied work.
    VerificationDenied,
    /// Trace synchronization is lagging behind the source watermark.
    TraceSyncLag,
    /// Trace synchronization failed.
    TraceSyncFailed,
    /// State handoff failed validation or import.
    StateHandoffFailed,
    /// Work order was rejected before run start/resume.
    WorkOrderRejected,
    /// Action gateway or adapter failure surfaced to telemetry.
    GatewayFailed,
    /// Bounded custom category for extension without vendor coupling.
    Other,
}

/// Failure signal with identity scope for fleet-level inspection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FailureSignal {
    /// Failure category taxonomy value.
    pub category: FailureCategory,
    /// Node involved, when applicable.
    pub node_id: Option<NodeId>,
    /// Instance involved, when applicable.
    pub instance_id: Option<InstanceId>,
    /// Tenant involved, when applicable.
    pub tenant_id: Option<TenantId>,
    /// Agent involved, when applicable.
    pub agent_id: Option<AgentId>,
    /// Run involved, when applicable.
    pub run_id: Option<RunId>,
    /// Verifier that surfaced the failure, when applicable.
    pub verifier: Option<String>,
    /// Human-readable summary or machine reason code.
    pub message: String,
    /// Trace event linked to the failure, when available.
    pub trace_id: Option<TraceId>,
    /// Timestamp when the failure was recorded.
    pub recorded_at: OffsetDateTime,
}

/// Quota signal scoped to tenant/agent/run and verifier identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuotaSignal {
    /// Tenant boundary for the quota signal.
    pub tenant_id: TenantId,
    /// Agent that consumed or requested quota.
    pub agent_id: AgentId,
    /// Run associated with the signal.
    pub run_id: RunId,
    /// Verifier that produced the signal, normally `quota`.
    pub verifier: Option<String>,
    /// Whether the quota check allowed the reported usage.
    pub allowed: bool,
    /// Reported or requested usage.
    pub usage: QuotaUsage,
    /// Reason codes from the verification result.
    pub reasons: Vec<String>,
    /// Structured verification evidence.
    pub artifacts: serde_json::Value,
    /// Timestamp when the quota signal was recorded.
    pub recorded_at: OffsetDateTime,
}

impl QuotaSignal {
    /// Converts a verification result into a quota telemetry signal.
    pub fn from_verification(
        tenant_id: TenantId,
        agent_id: AgentId,
        run_id: RunId,
        usage: QuotaUsage,
        verifier: impl Into<Option<String>>,
        result: &crate::VerificationResult,
        recorded_at: OffsetDateTime,
    ) -> Self {
        Self {
            tenant_id,
            agent_id,
            run_id,
            verifier: verifier.into(),
            allowed: result.allowed,
            usage,
            reasons: result.reasons.clone(),
            artifacts: result.artifacts.clone(),
            recorded_at,
        }
    }
}

/// Denial signal scoped to tenant/agent/run and verifier identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DenialSignal {
    /// Tenant boundary for the denial.
    pub tenant_id: TenantId,
    /// Agent whose proposed work was denied.
    pub agent_id: AgentId,
    /// Run associated with the denial.
    pub run_id: RunId,
    /// Verifier that denied the work, when known.
    pub verifier: Option<String>,
    /// Optional action name associated with the denial.
    pub action_name: Option<String>,
    /// Reason codes from the verification result.
    pub reasons: Vec<String>,
    /// Structured verification evidence.
    pub artifacts: serde_json::Value,
    /// Timestamp when the denial signal was recorded.
    pub recorded_at: OffsetDateTime,
}

impl DenialSignal {
    /// Converts a verification result into a denial telemetry signal.
    pub fn from_verification(
        tenant_id: TenantId,
        agent_id: AgentId,
        run_id: RunId,
        verifier: impl Into<Option<String>>,
        action_name: impl Into<Option<String>>,
        result: &crate::VerificationResult,
        recorded_at: OffsetDateTime,
    ) -> Self {
        Self {
            tenant_id,
            agent_id,
            run_id,
            verifier: verifier.into(),
            action_name: action_name.into(),
            reasons: result.reasons.clone(),
            artifacts: result.artifacts.clone(),
            recorded_at,
        }
    }
}

/// Trace synchronization failure details.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceSyncFailure {
    /// Failure category, normally `TraceSyncFailed`.
    pub category: FailureCategory,
    /// Failure message or reason code.
    pub message: String,
    /// Timestamp when synchronization failed.
    pub failed_at: OffsetDateTime,
}

/// Per node/instance trace sync status.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceSyncTelemetry {
    /// Node whose traces are syncing.
    pub node_id: NodeId,
    /// Instance whose traces are syncing.
    pub instance_id: InstanceId,
    /// Last trace event synced, when known.
    pub last_synced_trace_id: Option<TraceId>,
    /// Last synced sequence number within the source run/stream, when known.
    pub last_synced_sequence: Option<u64>,
    /// Source high-watermark sequence number, when known.
    pub source_high_watermark: Option<u64>,
    /// Number of unsynced events implied by the watermarks.
    pub lag_events: u64,
    /// Last successful sync timestamp.
    pub last_sync_at: Option<OffsetDateTime>,
    /// Most recent sync failure, if any.
    pub last_failure: Option<TraceSyncFailure>,
}

impl TraceSyncTelemetry {
    /// Creates trace sync telemetry and derives lag from sequence watermarks.
    pub fn from_watermarks(
        node_id: NodeId,
        instance_id: InstanceId,
        last_synced_trace_id: Option<TraceId>,
        last_synced_sequence: Option<u64>,
        source_high_watermark: Option<u64>,
        last_sync_at: Option<OffsetDateTime>,
        last_failure: Option<TraceSyncFailure>,
    ) -> Self {
        let lag_events = match (source_high_watermark, last_synced_sequence) {
            (Some(high), Some(last)) => high.saturating_sub(last),
            (Some(high), None) => high.saturating_add(1),
            _ => 0,
        };
        Self {
            node_id,
            instance_id,
            last_synced_trace_id,
            last_synced_sequence,
            source_high_watermark,
            lag_events,
            last_sync_at,
            last_failure,
        }
    }
}

/// Explicit marker that telemetry is observational and non-authoritative.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryAuthority {
    /// Telemetry can be inspected but cannot authorize runtime behavior.
    ObservationalOnly,
}

/// Fleet-level operational telemetry projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FleetTelemetrySnapshot {
    /// Telemetry schema version.
    pub schema_version: String,
    /// Fleet boundary for this snapshot.
    pub fleet_id: FleetId,
    /// Snapshot observation timestamp.
    pub observed_at: OffsetDateTime,
    /// Explicit non-authority marker.
    pub authority: TelemetryAuthority,
    /// Node health projections.
    pub nodes: Vec<NodeTelemetry>,
    /// Runtime instance projections.
    pub instances: Vec<InstanceTelemetry>,
    /// Run status projections.
    pub runs: Vec<RunTelemetry>,
    /// Queue status projections.
    pub queues: Vec<QueueTelemetry>,
    /// Quota signals.
    pub quota_signals: Vec<QuotaSignal>,
    /// Denial signals.
    pub denial_signals: Vec<DenialSignal>,
    /// Trace sync status by node/instance.
    pub trace_sync: Vec<TraceSyncTelemetry>,
    /// Aggregated failure signals.
    pub failures: Vec<FailureSignal>,
}

impl FleetTelemetrySnapshot {
    /// Creates an empty read-only telemetry snapshot.
    pub fn new(fleet_id: FleetId, observed_at: OffsetDateTime) -> Self {
        Self {
            schema_version: FLEET_TELEMETRY_SCHEMA_VERSION.to_string(),
            fleet_id,
            observed_at,
            authority: TelemetryAuthority::ObservationalOnly,
            nodes: Vec::new(),
            instances: Vec::new(),
            runs: Vec::new(),
            queues: Vec::new(),
            quota_signals: Vec::new(),
            denial_signals: Vec::new(),
            trace_sync: Vec::new(),
            failures: Vec::new(),
        }
    }

    /// Telemetry never authorizes permissions, work orders, or gateway decisions.
    pub fn authorizes_runtime_permissions(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "../tests/unit/fleet_telemetry_tests.rs"]
mod tests;
