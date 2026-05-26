use super::*;
use crate::VerificationResult;

#[test]
fn node_online_state_is_derived_from_heartbeat_age() {
    let observed_at = OffsetDateTime::UNIX_EPOCH + Duration::minutes(10);
    let stale_after = Duration::seconds(30);
    let offline_after = Duration::seconds(120);

    assert_eq!(
        NodeOnlineState::from_heartbeat(
            Some(observed_at - Duration::seconds(5)),
            observed_at,
            stale_after,
            offline_after,
        ),
        NodeOnlineState::Online
    );
    assert_eq!(
        NodeOnlineState::from_heartbeat(
            Some(observed_at - Duration::seconds(60)),
            observed_at,
            stale_after,
            offline_after,
        ),
        NodeOnlineState::Stale
    );
    assert_eq!(
        NodeOnlineState::from_heartbeat(
            Some(observed_at - Duration::seconds(120)),
            observed_at,
            stale_after,
            offline_after,
        ),
        NodeOnlineState::Offline
    );
    assert_eq!(
        NodeOnlineState::from_heartbeat(None, observed_at, stale_after, offline_after),
        NodeOnlineState::Offline
    );
}

#[test]
fn run_status_vocabulary_matches_canonical_states() {
    let names = RunStatus::ALL
        .iter()
        .map(|status| status.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "pending",
            "running",
            "paused",
            "waiting_for_approval",
            "interrupted",
            "resuming",
            "completed",
            "failed",
            "cancelled",
            "denied",
            "expired",
        ]
    );
}

#[test]
fn run_status_counts_preserve_all_status_rows() {
    let counts = RunStatusCounts::from_statuses([
        RunStatus::Running,
        RunStatus::Running,
        RunStatus::WaitingForApproval,
        RunStatus::Failed,
    ]);

    assert_eq!(counts.count(RunStatus::Running), 2);
    assert_eq!(counts.count(RunStatus::WaitingForApproval), 1);
    assert_eq!(counts.count(RunStatus::Failed), 1);
    assert_eq!(counts.count(RunStatus::Pending), 0);
    assert_eq!(counts.counts.len(), RunStatus::ALL.len());
}

#[test]
fn quota_and_denial_signals_preserve_identity_and_verifier() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let recorded_at = OffsetDateTime::UNIX_EPOCH + Duration::seconds(5);
    let result = VerificationResult {
        allowed: false,
        reasons: vec!["max_actions_per_tick".to_string()],
        artifacts: serde_json::json!({"context": "quota"}),
    };
    let usage = QuotaUsage::single_action();

    let quota = QuotaSignal::from_verification(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        usage,
        Some("quota".to_string()),
        &result,
        recorded_at,
    );
    assert_eq!(quota.tenant_id, tenant_id);
    assert_eq!(quota.agent_id, agent_id);
    assert_eq!(quota.run_id, run_id);
    assert_eq!(quota.verifier.as_deref(), Some("quota"));
    assert!(!quota.allowed);

    let denial = DenialSignal::from_verification(
        quota.tenant_id.clone(),
        quota.agent_id.clone(),
        quota.run_id.clone(),
        Some("quota".to_string()),
        Some("filesystem.write".to_string()),
        &result,
        recorded_at,
    );
    assert_eq!(denial.tenant_id, quota.tenant_id);
    assert_eq!(denial.agent_id, quota.agent_id);
    assert_eq!(denial.run_id, quota.run_id);
    assert_eq!(denial.verifier.as_deref(), Some("quota"));
    assert_eq!(denial.action_name.as_deref(), Some("filesystem.write"));
}

#[test]
fn trace_sync_lag_is_visible_from_watermarks() {
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let sync = TraceSyncTelemetry::from_watermarks(
        node_id.clone(),
        instance_id.clone(),
        None,
        Some(7),
        Some(10),
        Some(OffsetDateTime::UNIX_EPOCH),
        Some(TraceSyncFailure {
            category: FailureCategory::TraceSyncFailed,
            message: "store_unavailable".to_string(),
            failed_at: OffsetDateTime::UNIX_EPOCH + Duration::seconds(1),
        }),
    );

    assert_eq!(sync.node_id, node_id);
    assert_eq!(sync.instance_id, instance_id);
    assert_eq!(sync.lag_events, 3);
    assert_eq!(
        sync.last_failure.as_ref().map(|failure| failure.category),
        Some(FailureCategory::TraceSyncFailed)
    );
}

#[test]
fn telemetry_snapshot_is_observational_only() {
    let snapshot = FleetTelemetrySnapshot::new(FleetId::new(), OffsetDateTime::UNIX_EPOCH);

    assert_eq!(snapshot.schema_version, FLEET_TELEMETRY_SCHEMA_VERSION);
    assert_eq!(snapshot.authority, TelemetryAuthority::ObservationalOnly);
    assert!(!snapshot.authorizes_runtime_permissions());
}

#[test]
fn fleet_telemetry_round_trips_with_snake_case_statuses() {
    let fleet_id = FleetId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let observed_at = OffsetDateTime::UNIX_EPOCH + Duration::minutes(1);
    let mut snapshot = FleetTelemetrySnapshot::new(fleet_id.clone(), observed_at);
    snapshot.nodes.push(NodeTelemetry::from_heartbeat(
        fleet_id,
        node_id.clone(),
        Some(observed_at),
        observed_at,
        Duration::seconds(30),
        Duration::seconds(90),
        vec![instance_id.clone()],
    ));
    snapshot.instances.push(InstanceTelemetry::new(
        node_id,
        instance_id,
        "splendor-0.03-dev",
        TelemetryRuntimeMode::Resident,
        vec!["trace.sync".to_string()],
        observed_at,
    ));

    let payload = serde_json::to_value(&snapshot).expect("serialize");
    assert_eq!(payload["nodes"][0]["online_state"].as_str(), Some("online"));
    assert_eq!(
        payload["instances"][0]["runtime_mode"].as_str(),
        Some("resident")
    );

    let decoded: FleetTelemetrySnapshot = serde_json::from_value(payload).expect("deserialize");
    assert_eq!(decoded, snapshot);
}
