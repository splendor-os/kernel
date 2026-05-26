# 0.03-S8 — Fleet Telemetry

## 1. Objective

Implement a minimal fleet telemetry model and in-memory collector that aggregate
node health, instance runtime reports, canonical run status, queue depth,
quota/denial signals, trace-sync state, and failure signals for operational
inspection. This strengthens the fleet/node identity, runtime context, quota,
trace-store, and docs/tests primitives without adding a dashboard or runtime
authority path.

## 2. Functional scope

- Adds `FleetId`, `NodeId`, and `InstanceId` typed identities in
  `splendor-types` for telemetry scope separation.
- Adds fleet telemetry contracts in `splendor-types`:
  `FleetTelemetrySnapshot`, `NodeTelemetry`, `NodeOnlineState`,
  `InstanceTelemetry`, `RuntimeMode`, `RunTelemetry`, `RunStatus`,
  `RunStatusCounts`, `QueueTelemetry`, `QuotaSignal`, `DenialSignal`,
  `TraceSyncTelemetry`, `TraceSyncFailure`, `FailureSignal`, and
  `FailureCategory`.
- Adds `splendor_kernel::FleetTelemetryCollector` as the minimal ingestion and
  query collector for heartbeats, instance reports, run states, queues,
  quota/denial signals, trace-sync telemetry, and failures.
- Adds reference and example documentation.

## 3. Non-goals

- No UI dashboard.
- No anomaly detection engine.
- No billing metrics.
- No fleet autoscaler.
- No observability-vendor integration.
- No runtime permission, placement, work-order, verifier, gateway, or adapter
  authority derived from telemetry.
- No remote fleet transport, distributed consensus, or durable telemetry store.

## 4. Public contracts changed

New `splendor-types` exports:

- `FleetId`, `NodeId`, `InstanceId`
- `FLEET_TELEMETRY_SCHEMA_VERSION`
- `FleetTelemetrySnapshot`, `TelemetryAuthority`
- `NodeTelemetry`, `NodeOnlineState`
- `InstanceTelemetry`, `RuntimeMode`
- `RunTelemetry`, `RunStatus`, `RunStatusCounts`, `RunStatusCount`
- `QueueTelemetry`
- `QuotaSignal`, `DenialSignal`
- `TraceSyncTelemetry`, `TraceSyncFailure`
- `FailureSignal`, `FailureCategory`

New `splendor-kernel` exports:

- `FleetTelemetryCollector`
- `TelemetryThresholds`

New docs/examples:

- `docs/reference/fleet-telemetry.md`
- `docs/milestones/0.03-dev/S8-fleet-telemetry.md`
- `examples/fleet-telemetry-basic/README.md`

## 5. Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Fleet/node identity | Adds typed fleet, node, and instance IDs for telemetry. |
| Runtime context | Reports instance runtime mode, version, capabilities, queues, and run counts. |
| Quota | Adds telemetry signal shape preserving tenant/agent/run/verifier identity. |
| Verifier | Adds denial signal shape preserving verifier and reason artifacts. |
| Trace store | Adds trace-sync telemetry for lag/failure visibility by node/instance. |
| Replay | Documents inspect-only reconstruction; no side effects. |
| Gateway | No behavior change. Telemetry cannot authorize or bypass gateway execution. |
| State graph | No state commits or state format changes. Telemetry snapshots are observational projections. |

## 6. Trace events added or changed

No trace event variants were added or renamed. Fleet telemetry observes existing
trace-linked runtime facts and records trace sync status through
`TraceSyncTelemetry`:

- `last_synced_trace_id`
- `last_synced_sequence`
- `source_high_watermark`
- `lag_events`
- `last_failure`

Quota and denial telemetry preserve verifier reasons/artifacts so existing
`action.denied`, `action.failed`, and `outcome.recorded` traces can be correlated
by the runtime paths that emitted them.

## 7. State behavior added or changed

No agent state graph nodes are created or modified by this sprint. The collector
stores in-memory operational reports only to build snapshots. That collector
state does not affect policy decisions, verifier outcomes, gateway behavior,
adapter execution, work-order acceptance, or placement authority.

## 8. Verifier/gateway behavior added or changed

No verifier chain or Action Gateway behavior changed. Telemetry reports quota and
denial results that were already produced elsewhere; it never turns an observed
signal into an allow/deny decision. Side effects continue to require gateway
submission and verifier checks.

## 9. Replay behavior

Replay remains inspect-only. A replay or audit tool may inspect stored telemetry
snapshots or reconstruct equivalent views from recorded traces, heartbeats, and
sync fixtures. Replay must not send heartbeats, re-sync traces, retry denied
actions, execute adapters, or mutate fleet/node/instance state.

## 10. Failure behavior

| Failure / signal | Behavior |
| --- | --- |
| Missing heartbeat | Node reports `offline`. |
| Stale heartbeat | Node reports `stale` at/after `stale_after`. |
| Offline heartbeat age | Node reports `offline` at/after `offline_after`. |
| Quota denial | `QuotaSignal` and optional `DenialSignal` preserve tenant, agent, run, verifier, reasons, and artifacts. |
| Trace sync lag | `TraceSyncTelemetry.lag_events` exposes the lag per node/instance. |
| Trace sync failure | `TraceSyncFailure` records category, message, and timestamp. |
| Telemetry misuse as authority | Snapshot authority remains `observational_only`; helper returns `false` for runtime authorization. |

## 11. Test evidence

| Requirement / criterion | Evidence |
| --- | --- |
| FR-0.03-11 fleet telemetry aggregation | `crates/splendor-kernel/tests/unit/fleet_telemetry_tests.rs` |
| Node online/stale/offline from heartbeat data | `collector_reports_node_online_stale_and_offline_from_heartbeats`; `node_online_state_is_derived_from_heartbeat_age` |
| Instance version/mode/capabilities/run counts | `collector_reports_instance_runtime_capabilities_and_run_counts` |
| Canonical run states | `run_status_vocabulary_matches_canonical_states`; `run_status_counts_preserve_all_status_rows` |
| Quota/denial identity and verifier | `collector_preserves_quota_denial_identity_and_verifier`; `quota_and_denial_signals_preserve_identity_and_verifier` |
| Trace sync lag/failure per node/instance | `collector_reports_trace_sync_lag_and_failure_per_instance`; `trace_sync_lag_is_visible_from_watermarks` |
| Telemetry not authority | `collector_tracks_queue_status_without_authority`; `telemetry_snapshot_is_observational_only` |
| Serialization contract | `fleet_telemetry_round_trips_with_snake_case_statuses`; `round_trip_core_types` |

Targeted validation command:

```bash
cargo test -p splendor-types -p splendor-kernel
```

## 12. Example commands or fixtures

Example fixture documentation: `examples/fleet-telemetry-basic/README.md`.

Useful commands:

```bash
cargo test -p splendor-types fleet_telemetry
cargo test -p splendor-kernel fleet_telemetry
cargo test --workspace
```

## 13. Future extension notes

- 0.03 fleet APIs can expose `FleetTelemetrySnapshot` through authenticated,
  scoped endpoints without changing the collector semantics.
- Durable telemetry storage can persist snapshots or source reports, but must not
  replace trace-store integrity or state graph commits.
- Dashboards, anomaly detection, autoscaling, billing metrics, and vendor metric
  exporters can consume snapshots later without becoming runtime authority.
- Governance milestones may correlate telemetry failures with approvals or
  circuit breakers, but circuit breakers must be explicit governance state, not
  implicit telemetry interpretation.
