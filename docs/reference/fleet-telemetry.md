# Fleet Telemetry Reference

Fleet telemetry is the 0.03-S8 operational projection for resident/fleet
execution. It aggregates heartbeat health, runtime instance reports, run status,
queue depth, quota/denial signals, trace-sync state, and failure signals into a
minimal read-only surface.

Telemetry is **not** an authority source. It must never grant permissions,
validate work orders, approve placement, or bypass the Action Gateway. It reports
what happened or what is currently observed; runtime enforcement remains with
work-order validation, tenant/agent policy, verifier chains, quotas, and the
Action Gateway.

## Public Rust contracts

The reference contracts are exported from `splendor-types` and re-exported by
`splendor-kernel` where useful:

- identity: `FleetId`, `NodeId`, `InstanceId`
- snapshot: `FleetTelemetrySnapshot`
- node health: `NodeTelemetry`, `NodeOnlineState`
- instance status: `InstanceTelemetry`, `RuntimeMode`
- run status: `RunTelemetry`, `RunStatus`, `RunStatusCounts`, `RunStatusCount`
- queues: `QueueTelemetry`
- quota/denial signals: `QuotaSignal`, `DenialSignal`
- trace sync: `TraceSyncTelemetry`, `TraceSyncFailure`
- failures: `FailureSignal`, `FailureCategory`
- authority marker: `TelemetryAuthority::ObservationalOnly`
- collector: `splendor_kernel::FleetTelemetryCollector`

The schema version string is:

```text
splendor.fleet_telemetry.v1
```

## Identity model

Telemetry preserves the distributed identity separation required by 0.03-dev:

| ID | Purpose |
| --- | --- |
| `FleetId` | Governed fleet boundary. |
| `NodeId` | Physical, virtual, or logical host. |
| `InstanceId` | Splendor runtime process/instance on a node. |
| `TenantId` | Tenant authority boundary for run/quota/denial signals. |
| `AgentId` | Agent runtime identity for run/quota/denial signals. |
| `RunId` | Run lifecycle identity. |
| `TraceId` | Trace event identity used in sync/failure references. |

Telemetry must not overload these IDs or infer tenant/agent/run authority from a
fleet, node, or instance health report.

## Node health

`NodeTelemetry` reports:

- `fleet_id`
- `node_id`
- `online_state`
- `last_heartbeat_at`
- `observed_at`
- `instance_ids`

`NodeOnlineState` is derived from heartbeat recency:

| State | Meaning |
| --- | --- |
| `online` | Last heartbeat is newer than the stale threshold. |
| `stale` | Last heartbeat is at or beyond stale threshold but before offline threshold. |
| `offline` | Heartbeat is missing or at/beyond offline threshold. |

The reference collector defaults to `stale_after = 30s` and
`offline_after = 120s`. Callers can provide explicit `TelemetryThresholds`.

## Instance telemetry

`InstanceTelemetry` reports:

- `node_id`
- `instance_id`
- `runtime_version`
- `runtime_mode`
- `capabilities`
- `current_run_counts`
- `reported_at`

`RuntimeMode` values are operational labels, not permissions:

```text
ephemeral
resident
dedicated
customer_vpc
on_prem
edge_device
physical_robot
desktop_sidecar
custom
```

`RuntimeMode::Custom(String)` serializes as the Rust enum's tagged custom value;
use a bounded label and do not use it to imply permissions.

The reference collector computes `current_run_counts` from known `RunTelemetry`
entries for each instance when producing a snapshot.

## Canonical run statuses

`RunStatus` uses the 0.03-dev canonical lifecycle states:

```text
pending
running
paused
waiting_for_approval
interrupted
resuming
completed
failed
cancelled
denied
expired
```

`RunStatusCounts::canonical_empty()` includes every status with a zero count so
fleet views can distinguish "known status with zero runs" from "unknown status".

## Queue telemetry

`QueueTelemetry` reports minimal queue depth per node/instance:

- `queued_runs`
- `queued_messages`
- `updated_at`

Queue telemetry is operational only. It is not a scheduler, broker, autoscaler,
or permission signal.

## Quota and denial signals

`QuotaSignal` preserves:

- `tenant_id`
- `agent_id`
- `run_id`
- `verifier`
- `allowed`
- `usage`
- `reasons`
- `artifacts`
- `recorded_at`

`DenialSignal` preserves:

- `tenant_id`
- `agent_id`
- `run_id`
- `verifier`
- `action_name`
- `reasons`
- `artifacts`
- `recorded_at`

These fields are intended to expose quota pressure and verifier denials without
changing verifier or gateway semantics. A telemetry denial signal does not deny a
future action by itself; future action decisions must still run the verifier
chain and fail closed there.

## Trace sync telemetry

`TraceSyncTelemetry` reports trace aggregation status per node/instance:

- `node_id`
- `instance_id`
- `last_synced_trace_id`
- `last_synced_sequence`
- `source_high_watermark`
- `lag_events`
- `last_sync_at`
- `last_failure`

`lag_events` is derived from source and synced sequence watermarks. A
`TraceSyncFailure` records category, message, and failure timestamp.

Telemetry references trace IDs and sync watermarks, but does not replace the
append-only trace store, hash-chain validation, or replay validation.

## Failure categories

`FailureCategory` defines the 0.03-S8 failure taxonomy:

```text
heartbeat_stale
heartbeat_offline
instance_unavailable
run_failed
quota_denied
verification_denied
trace_sync_lag
trace_sync_failed
state_handoff_failed
work_order_rejected
gateway_failed
other
```

`FailureSignal` carries the category plus optional node, instance, tenant, agent,
run, verifier, and trace identifiers.

## Snapshot lifecycle

The reference lifecycle is:

1. Ingest node heartbeats with `FleetTelemetryCollector::ingest_node_heartbeat`.
2. Upsert runtime instance reports with `upsert_instance`.
3. Upsert run status reports with `upsert_run`.
4. Upsert queue reports, quota signals, denial signals, trace sync status, and
   failure signals.
5. Call `snapshot(observed_at)` to produce a deterministic
   `FleetTelemetrySnapshot`.

The collector is in-memory and minimal. Durable telemetry storage, dashboards,
vendor metrics export, autoscaling, and anomaly detection are out of scope for
0.03-S8.

## Trace behavior

0.03-S8 adds no new trace event classes. Telemetry observes existing runtime
signals and trace sync state:

- quota/denial signals should be trace-linked by the runtime path that produced
  the verifier result when a `TraceId` is available;
- trace sync telemetry may refer to the latest synced `TraceId` and sequence;
- replay may inspect telemetry snapshots or reconstruct equivalent projections
  from trace/heartbeat fixtures without re-executing actions.

## State and replay behavior

Telemetry snapshots are observational summaries, not agent state graph commits.
The reference collector keeps in-memory report state solely to build snapshots;
that report state does not affect policy, verifier, gateway, or adapter behavior.

Replay remains inspect-only. Reconstructing telemetry from recorded traces or
fixtures must not send heartbeats, sync traces, retry adapters, or execute side
effects.

## Security notes

- `FleetTelemetrySnapshot.authority` is always
  `TelemetryAuthority::ObservationalOnly`.
- `FleetTelemetrySnapshot::authorizes_runtime_permissions()` always returns
  `false`.
- Telemetry cannot grant tenant, agent, adapter, action, or data permissions.
- Telemetry cannot validate or revoke work orders.
- Telemetry cannot bypass the Action Gateway or verifier chain.
- Fleet/node/instance health must not be treated as tenant/runtime authority.

## Minimal example

```rust
use splendor_kernel::{
    FleetId, FleetTelemetryCollector, InstanceId, InstanceTelemetry, NodeId,
    RunStatus, RunTelemetry, RuntimeMode, TenantId, AgentId, RunId,
};
use time::OffsetDateTime;

let now = OffsetDateTime::now_utc();
let fleet_id = FleetId::new();
let node_id = NodeId::new();
let instance_id = InstanceId::new();

let mut collector = FleetTelemetryCollector::new(fleet_id);
collector.ingest_node_heartbeat(node_id.clone(), now);
collector.upsert_instance(InstanceTelemetry::new(
    node_id.clone(),
    instance_id.clone(),
    "splendor-0.03-dev",
    RuntimeMode::Resident,
    vec!["trace.sync".to_string()],
    now,
));
collector.upsert_run(RunTelemetry {
    tenant_id: TenantId::new(),
    agent_id: AgentId::new(),
    run_id: RunId::new(),
    node_id,
    instance_id,
    status: RunStatus::Running,
    updated_at: now,
});

let snapshot = collector.snapshot(now);
assert!(!snapshot.authorizes_runtime_permissions());
```

## Validation commands

```bash
cargo test -p splendor-types fleet_telemetry
cargo test -p splendor-kernel fleet_telemetry
cargo test -p splendor-types -p splendor-kernel
```

## Compatibility notes

This is a 0.03-dev schema and may evolve before the 0.1 stable compatibility
line. Later fleet management APIs can serialize the same snapshot shape without
using telemetry as an enforcement authority.
