use super::*;
use splendor_types::{
    AgentId, DenialSignal, FailureCategory, FleetId, InstanceId, InstanceTelemetry, NodeId,
    NodeOnlineState, QueueTelemetry, QuotaSignal, QuotaUsage, RunId, RunStatus, RunTelemetry,
    TelemetryAuthority, TelemetryRuntimeMode, TenantId, TraceSyncFailure, TraceSyncTelemetry,
    VerificationResult,
};

fn thresholds() -> TelemetryThresholds {
    TelemetryThresholds {
        stale_after: time::Duration::seconds(30),
        offline_after: time::Duration::seconds(120),
    }
}

fn instance_report(
    node_id: NodeId,
    instance_id: InstanceId,
    now: time::OffsetDateTime,
) -> InstanceTelemetry {
    InstanceTelemetry::new(
        node_id,
        instance_id,
        "splendor-0.03-dev",
        TelemetryRuntimeMode::Resident,
        vec!["trace.sync".to_string(), "work_order.dispatch".to_string()],
        now,
    )
}

#[test]
fn collector_reports_node_online_stale_and_offline_from_heartbeats() {
    let fleet_id = FleetId::new();
    let observed_at = time::OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(10);
    let online_node = NodeId::new();
    let stale_node = NodeId::new();
    let offline_node = NodeId::new();

    let mut collector = FleetTelemetryCollector::with_thresholds(fleet_id, thresholds());
    collector.ingest_node_heartbeat(
        online_node.clone(),
        observed_at - time::Duration::seconds(10),
    );
    collector.ingest_node_heartbeat(
        stale_node.clone(),
        observed_at - time::Duration::seconds(60),
    );
    collector.ingest_node_heartbeat(
        offline_node.clone(),
        observed_at - time::Duration::seconds(150),
    );

    let snapshot = collector.snapshot(observed_at);

    assert_eq!(
        snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == online_node)
            .map(|node| node.online_state),
        Some(NodeOnlineState::Online)
    );
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == stale_node)
            .map(|node| node.online_state),
        Some(NodeOnlineState::Stale)
    );
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == offline_node)
            .map(|node| node.online_state),
        Some(NodeOnlineState::Offline)
    );
}

#[test]
fn collector_reports_instance_runtime_capabilities_and_run_counts() {
    let fleet_id = FleetId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let now = time::OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(3);

    let mut collector = FleetTelemetryCollector::new(fleet_id);
    collector.ingest_node_heartbeat(node_id.clone(), now);
    collector.upsert_instance(instance_report(node_id.clone(), instance_id.clone(), now));
    collector.upsert_run(RunTelemetry {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        run_id: RunId::new(),
        node_id: node_id.clone(),
        instance_id: instance_id.clone(),
        status: RunStatus::Running,
        updated_at: now,
    });
    collector.upsert_run(RunTelemetry {
        tenant_id,
        agent_id,
        run_id: RunId::new(),
        node_id,
        instance_id: instance_id.clone(),
        status: RunStatus::WaitingForApproval,
        updated_at: now,
    });

    let snapshot = collector.snapshot(now);
    let instance = snapshot
        .instances
        .iter()
        .find(|instance| instance.instance_id == instance_id)
        .expect("instance telemetry");

    assert_eq!(instance.runtime_version, "splendor-0.03-dev");
    assert_eq!(&instance.runtime_mode, &TelemetryRuntimeMode::Resident);
    assert!(instance.capabilities.contains(&"trace.sync".to_string()));
    assert_eq!(instance.current_run_counts.count(RunStatus::Running), 1);
    assert_eq!(
        instance
            .current_run_counts
            .count(RunStatus::WaitingForApproval),
        1
    );
    assert_eq!(instance.current_run_counts.count(RunStatus::Denied), 0);
}

#[test]
fn collector_preserves_quota_denial_identity_and_verifier() {
    let fleet_id = FleetId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let now = time::OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(4);
    let verification = VerificationResult {
        allowed: false,
        reasons: vec!["max_actions_per_tick".to_string()],
        artifacts: serde_json::json!({
            "quota": {"limit": 1, "requested": 2},
            "context": {
                "tenant_id": tenant_id.to_string(),
                "agent_id": agent_id.to_string()
            }
        }),
    };

    let mut collector = FleetTelemetryCollector::new(fleet_id);
    collector.record_quota_signal(QuotaSignal::from_verification(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        QuotaUsage::single_action(),
        Some("quota".to_string()),
        &verification,
        now,
    ));
    collector.record_denial_signal(DenialSignal::from_verification(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        Some("quota".to_string()),
        Some("artifact.publish".to_string()),
        &verification,
        now,
    ));

    let snapshot = collector.snapshot(now);
    let quota = snapshot.quota_signals.first().expect("quota signal");
    let denial = snapshot.denial_signals.first().expect("denial signal");

    assert_eq!(quota.tenant_id, tenant_id);
    assert_eq!(quota.agent_id, agent_id);
    assert_eq!(quota.run_id, run_id);
    assert_eq!(quota.verifier.as_deref(), Some("quota"));
    assert!(!quota.allowed);
    assert_eq!(denial.tenant_id, quota.tenant_id);
    assert_eq!(denial.agent_id, quota.agent_id);
    assert_eq!(denial.run_id, quota.run_id);
    assert_eq!(denial.verifier.as_deref(), Some("quota"));
}

#[test]
fn collector_reports_trace_sync_lag_and_failure_per_instance() {
    let fleet_id = FleetId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let now = time::OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(5);
    let mut collector = FleetTelemetryCollector::new(fleet_id);

    collector.upsert_trace_sync(TraceSyncTelemetry::from_watermarks(
        node_id.clone(),
        instance_id.clone(),
        None,
        Some(5),
        Some(9),
        Some(now),
        Some(TraceSyncFailure {
            category: FailureCategory::TraceSyncFailed,
            message: "central_store_unavailable".to_string(),
            failed_at: now,
        }),
    ));

    let snapshot = collector.snapshot(now);
    let sync = snapshot.trace_sync.first().expect("trace sync telemetry");

    assert_eq!(sync.node_id, node_id);
    assert_eq!(sync.instance_id, instance_id);
    assert_eq!(sync.lag_events, 4);
    assert_eq!(
        sync.last_failure.as_ref().map(|failure| failure.category),
        Some(FailureCategory::TraceSyncFailed)
    );
}

#[test]
fn collector_tracks_queue_status_without_authority() {
    let fleet_id = FleetId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let now = time::OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(6);
    let mut collector = FleetTelemetryCollector::new(fleet_id);
    collector.upsert_queue(QueueTelemetry {
        node_id,
        instance_id,
        queued_runs: 2,
        queued_messages: 3,
        updated_at: now,
    });

    let snapshot = collector.snapshot(now);

    assert_eq!(
        snapshot.queues.first().map(|queue| queue.queued_runs),
        Some(2)
    );
    assert_eq!(
        snapshot.queues.first().map(|queue| queue.queued_messages),
        Some(3)
    );
    assert_eq!(snapshot.authority, TelemetryAuthority::ObservationalOnly);
    assert!(!snapshot.authorizes_runtime_permissions());
}
