# Fleet Telemetry Basic Example

This example documents the 0.03-S8 fleet telemetry reference path. It is a
minimal in-memory operational collector, not a dashboard, metrics backend,
autoscaler, scheduler, or permission source.

## What it demonstrates

- Create distinct `FleetId`, `NodeId`, and `InstanceId` values.
- Ingest node heartbeats and derive `online`, `stale`, or `offline` state.
- Report instance runtime version, mode, capabilities, and current run counts.
- Record a canonical `RunStatus` for a run.
- Surface queue depth, quota/denial signals, and trace sync lag.
- Confirm telemetry is `observational_only` and cannot authorize runtime
  permissions or side effects.

## Minimal Rust fixture

```rust
use splendor_kernel::{
    AgentId, DenialSignal, FleetId, FleetTelemetryCollector, InstanceId,
    InstanceTelemetry, NodeId, QueueTelemetry, QuotaSignal, QuotaUsage, RunId,
    RunStatus, RunTelemetry, RuntimeMode, TenantId, TraceSyncTelemetry,
    VerificationResult,
};
use time::OffsetDateTime;

let now = OffsetDateTime::now_utc();
let fleet_id = FleetId::new();
let node_id = NodeId::new();
let instance_id = InstanceId::new();
let tenant_id = TenantId::new();
let agent_id = AgentId::new();
let run_id = RunId::new();

let mut collector = FleetTelemetryCollector::new(fleet_id);
collector.ingest_node_heartbeat(node_id.clone(), now);
collector.upsert_instance(InstanceTelemetry::new(
    node_id.clone(),
    instance_id.clone(),
    "splendor-0.03-dev",
    RuntimeMode::Resident,
    vec!["trace.sync".to_string(), "work_order.dispatch".to_string()],
    now,
));
collector.upsert_run(RunTelemetry {
    tenant_id: tenant_id.clone(),
    agent_id: agent_id.clone(),
    run_id: run_id.clone(),
    node_id: node_id.clone(),
    instance_id: instance_id.clone(),
    status: RunStatus::Running,
    updated_at: now,
});
collector.upsert_queue(QueueTelemetry {
    node_id: node_id.clone(),
    instance_id: instance_id.clone(),
    queued_runs: 1,
    queued_messages: 2,
    updated_at: now,
});

let denied = VerificationResult {
    allowed: false,
    reasons: vec!["max_actions_per_tick".to_string()],
    artifacts: serde_json::json!({"verifier": "quota"}),
};
collector.record_quota_signal(QuotaSignal::from_verification(
    tenant_id.clone(),
    agent_id.clone(),
    run_id.clone(),
    QuotaUsage::single_action(),
    Some("quota".to_string()),
    &denied,
    now,
));
collector.record_denial_signal(DenialSignal::from_verification(
    tenant_id,
    agent_id,
    run_id,
    Some("quota".to_string()),
    Some("artifact.publish".to_string()),
    &denied,
    now,
));
collector.upsert_trace_sync(TraceSyncTelemetry::from_watermarks(
    node_id,
    instance_id,
    None,
    Some(7),
    Some(10),
    Some(now),
    None,
));

let snapshot = collector.snapshot(now);
assert_eq!(snapshot.nodes[0].online_state.as_str(), "online");
assert_eq!(snapshot.instances[0].current_run_counts.count(RunStatus::Running), 1);
assert_eq!(snapshot.trace_sync[0].lag_events, 3);
assert!(!snapshot.authorizes_runtime_permissions());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Expected telemetry behavior

- Heartbeat recency drives only telemetry state:
  - fresh heartbeat -> `online`
  - heartbeat at/after stale threshold -> `stale`
  - missing heartbeat or heartbeat at/after offline threshold -> `offline`
- Instance reports include runtime version, runtime mode, capabilities, and
  run counts computed from known run telemetry.
- Run status uses only the canonical 0.03 lifecycle states:
  `pending`, `running`, `paused`, `waiting_for_approval`, `interrupted`,
  `resuming`, `completed`, `failed`, `cancelled`, `denied`, and `expired`.
- Quota and denial signals preserve tenant, agent, run, verifier, reasons, and
  artifacts.
- Trace sync lag/failure is visible per node/instance.

## Security and side-effect boundary

Telemetry is read-only operational data. It does not execute policy, verifiers,
gateways, adapters, work-order validation, state commits, or trace sync actions.
Any side effect still must go through the Action Gateway and verifier chain.

## Validation commands

```bash
cargo test -p splendor-types fleet_telemetry
cargo test -p splendor-kernel fleet_telemetry
cargo test -p splendor-types -p splendor-kernel
```

## Non-goals

- No UI dashboard.
- No anomaly detection.
- No billing metrics.
- No fleet autoscaler.
- No observability-vendor coupling.
- No hidden permission or placement authority.
