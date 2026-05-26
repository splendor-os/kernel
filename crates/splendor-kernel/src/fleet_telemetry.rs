//! # Fleet Telemetry Collector
//!
//! A minimal in-memory collector for 0.03-S8. It ingests operational reports and
//! returns read-only fleet telemetry snapshots. The collector is deliberately not
//! connected to action authorization, work-order validation, or gateway policy.

use splendor_types::{
    DenialSignal, FailureSignal, FleetId, FleetTelemetrySnapshot, InstanceId, InstanceTelemetry,
    NodeId, NodeTelemetry, QueueTelemetry, QuotaSignal, RunId, RunStatusCounts, RunTelemetry,
    TraceSyncTelemetry,
};
use std::collections::HashMap;
use time::{Duration, OffsetDateTime};

/// Heartbeat age thresholds used to derive node online/stale/offline state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryThresholds {
    /// Heartbeat age at or above this duration is stale.
    pub stale_after: Duration,
    /// Heartbeat age at or above this duration is offline.
    pub offline_after: Duration,
}

impl Default for TelemetryThresholds {
    fn default() -> Self {
        Self {
            stale_after: Duration::seconds(30),
            offline_after: Duration::seconds(120),
        }
    }
}

/// Minimal operational collector for fleet telemetry reports.
#[derive(Clone, Debug)]
pub struct FleetTelemetryCollector {
    fleet_id: FleetId,
    thresholds: TelemetryThresholds,
    heartbeats: HashMap<NodeId, OffsetDateTime>,
    instances: HashMap<InstanceId, InstanceTelemetry>,
    runs: HashMap<RunId, RunTelemetry>,
    queues: HashMap<InstanceId, QueueTelemetry>,
    quota_signals: Vec<QuotaSignal>,
    denial_signals: Vec<DenialSignal>,
    trace_sync: HashMap<(NodeId, InstanceId), TraceSyncTelemetry>,
    failures: Vec<FailureSignal>,
}

impl FleetTelemetryCollector {
    /// Creates a collector with default heartbeat thresholds.
    pub fn new(fleet_id: FleetId) -> Self {
        Self::with_thresholds(fleet_id, TelemetryThresholds::default())
    }

    /// Creates a collector with explicit heartbeat thresholds.
    pub fn with_thresholds(fleet_id: FleetId, thresholds: TelemetryThresholds) -> Self {
        Self {
            fleet_id,
            thresholds,
            heartbeats: HashMap::new(),
            instances: HashMap::new(),
            runs: HashMap::new(),
            queues: HashMap::new(),
            quota_signals: Vec::new(),
            denial_signals: Vec::new(),
            trace_sync: HashMap::new(),
            failures: Vec::new(),
        }
    }

    /// Records the latest heartbeat timestamp for a node.
    pub fn ingest_node_heartbeat(&mut self, node_id: NodeId, heartbeat_at: OffsetDateTime) {
        self.heartbeats.insert(node_id, heartbeat_at);
    }

    /// Inserts or replaces a runtime instance report.
    pub fn upsert_instance(&mut self, instance: InstanceTelemetry) {
        self.instances
            .insert(instance.instance_id.clone(), instance);
    }

    /// Inserts or replaces a run status report.
    pub fn upsert_run(&mut self, run: RunTelemetry) {
        self.runs.insert(run.run_id.clone(), run);
    }

    /// Inserts or replaces a queue status report.
    pub fn upsert_queue(&mut self, queue: QueueTelemetry) {
        self.queues.insert(queue.instance_id.clone(), queue);
    }

    /// Records a quota signal. This is observational only.
    pub fn record_quota_signal(&mut self, signal: QuotaSignal) {
        self.quota_signals.push(signal);
    }

    /// Records a denial signal. This is observational only.
    pub fn record_denial_signal(&mut self, signal: DenialSignal) {
        self.denial_signals.push(signal);
    }

    /// Inserts or replaces per node/instance trace sync telemetry.
    pub fn upsert_trace_sync(&mut self, sync: TraceSyncTelemetry) {
        self.trace_sync
            .insert((sync.node_id.clone(), sync.instance_id.clone()), sync);
    }

    /// Records a failure signal.
    pub fn record_failure(&mut self, failure: FailureSignal) {
        self.failures.push(failure);
    }

    /// Builds a deterministic read-only telemetry snapshot.
    pub fn snapshot(&self, observed_at: OffsetDateTime) -> FleetTelemetrySnapshot {
        let mut snapshot = FleetTelemetrySnapshot::new(self.fleet_id.clone(), observed_at);
        snapshot.nodes = self.node_telemetry(observed_at);
        snapshot.instances = self.instance_telemetry();
        snapshot.runs = self.sorted_runs();
        snapshot.queues = self.sorted_queues();
        snapshot.quota_signals = self.sorted_quota_signals();
        snapshot.denial_signals = self.sorted_denial_signals();
        snapshot.trace_sync = self.sorted_trace_sync();
        snapshot.failures = self.sorted_failures();
        snapshot
    }

    fn node_telemetry(&self, observed_at: OffsetDateTime) -> Vec<NodeTelemetry> {
        let mut instance_ids_by_node: HashMap<NodeId, Vec<InstanceId>> = HashMap::new();
        for instance in self.instances.values() {
            instance_ids_by_node
                .entry(instance.node_id.clone())
                .or_default()
                .push(instance.instance_id.clone());
        }

        for node_id in self.heartbeats.keys() {
            instance_ids_by_node.entry(node_id.clone()).or_default();
        }

        let mut nodes = instance_ids_by_node
            .into_iter()
            .map(|(node_id, mut instance_ids)| {
                instance_ids.sort_by_key(|instance_id| instance_id.to_string());
                NodeTelemetry::from_heartbeat(
                    self.fleet_id.clone(),
                    node_id.clone(),
                    self.heartbeats.get(&node_id).copied(),
                    observed_at,
                    self.thresholds.stale_after,
                    self.thresholds.offline_after,
                    instance_ids,
                )
            })
            .collect::<Vec<_>>();
        nodes.sort_by_key(|node| node.node_id.to_string());
        nodes
    }

    fn instance_telemetry(&self) -> Vec<InstanceTelemetry> {
        let mut instances = self
            .instances
            .values()
            .cloned()
            .map(|mut instance| {
                let statuses = self
                    .runs
                    .values()
                    .filter(|run| run.instance_id == instance.instance_id)
                    .map(|run| run.status);
                instance.current_run_counts = RunStatusCounts::from_statuses(statuses);
                instance
            })
            .collect::<Vec<_>>();
        instances.sort_by_key(|instance| instance.instance_id.to_string());
        instances
    }

    fn sorted_runs(&self) -> Vec<RunTelemetry> {
        let mut runs = self.runs.values().cloned().collect::<Vec<_>>();
        runs.sort_by_key(|run| run.run_id.to_string());
        runs
    }

    fn sorted_queues(&self) -> Vec<QueueTelemetry> {
        let mut queues = self.queues.values().cloned().collect::<Vec<_>>();
        queues.sort_by_key(|queue| queue.instance_id.to_string());
        queues
    }

    fn sorted_quota_signals(&self) -> Vec<QuotaSignal> {
        let mut signals = self.quota_signals.clone();
        signals.sort_by_key(|signal| signal.recorded_at);
        signals
    }

    fn sorted_denial_signals(&self) -> Vec<DenialSignal> {
        let mut signals = self.denial_signals.clone();
        signals.sort_by_key(|signal| signal.recorded_at);
        signals
    }

    fn sorted_trace_sync(&self) -> Vec<TraceSyncTelemetry> {
        let mut sync = self.trace_sync.values().cloned().collect::<Vec<_>>();
        sync.sort_by_key(|sync| (sync.node_id.to_string(), sync.instance_id.to_string()));
        sync
    }

    fn sorted_failures(&self) -> Vec<FailureSignal> {
        let mut failures = self.failures.clone();
        failures.sort_by_key(|failure| failure.recorded_at);
        failures
    }
}

#[cfg(test)]
#[path = "../tests/unit/fleet_telemetry_tests.rs"]
mod tests;
