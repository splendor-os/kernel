# Resident Node Registration Example

This example documents the 0.03-S2 node and instance registry path. It is a
local, in-memory reference flow: no signed work-order ingestion, placement
engine, remote transport, trace aggregation, governance workflow, or device
safety verifier is involved.

## What it demonstrates

- Register a resident node with `NodeId`, `NodeKind`, tenant/fleet scope,
  capabilities, constraints, runtime version, and health metadata.
- Register a Splendor runtime `InstanceId` under that node with runtime mode,
  hosted tenants, and supported features.
- Emit management audit events for registration and heartbeat changes.
- Update heartbeat health without overwriting static registration fields.
- Compute stale heartbeat status deterministically at an explicit timestamp.

Registration metadata does not authorize side effects. Any future run dispatched
to a registered node must still be authorized by a signed work order, scoped
tenant/agent policy, and the Action Gateway verifier chain.

## Minimal Rust fixture

```rust
use splendor_kernel::{
    HeartbeatFreshness, InMemoryManagementAuditSink, InMemoryNodeRegistry,
    NodeRegistry, NodeRegistryConfig,
};
use splendor_types::*;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(10);
let fleet_id = FleetId::new();
let tenant_id = TenantId::new();
let node_id = NodeId::new();

let audit = Arc::new(InMemoryManagementAuditSink::default());
let registry = InMemoryNodeRegistry::with_audit_sink(
    NodeRegistryConfig {
        stale_after: Duration::seconds(30),
    },
    audit.clone(),
)?;

let node = NodeRegistration {
    node_id: node_id.clone(),
    kind: NodeKind::new("edge.appliance")?,
    scope: RegistryScope::fleet_tenant(fleet_id, tenant_id.clone()),
    capability_document: CapabilityDocument::new(
        vec![
            "runtime.resident".to_string(),
            "trace.buffer.local".to_string(),
            "http.egress.restricted".to_string(),
        ],
        serde_json::json!({"data_locality": "on_prem"}),
    )?,
    runtime_version: "splendor-0.03-dev".to_string(),
    health: NodeHealth {
        status: HealthStatus::Healthy,
        observed_at: now,
        metadata: serde_json::json!({"network": "online"}),
    },
    registered_at: now,
};
registry.register_node(node)?;

let instance_id = InstanceId::new();
let instance = InstanceRegistration {
    instance_id: instance_id.clone(),
    node_id: node_id.clone(),
    runtime_mode: RuntimeMode::Resident,
    hosted_tenants: vec![tenant_id],
    supported_features: vec![
        "local.message.router".to_string(),
        "state.graph".to_string(),
    ],
    runtime_version: "splendor-0.03-dev".to_string(),
    health: InstanceHealth {
        status: HealthStatus::Healthy,
        observed_at: now,
        metadata: serde_json::json!({"active_runtime_contexts": 0}),
    },
    registered_at: now,
};
registry.register_instance(instance)?;

let heartbeat_at = now + Duration::seconds(5);
registry.record_node_heartbeat(NodeHeartbeat {
    node_id: node_id.clone(),
    health: NodeHealth {
        status: HealthStatus::Degraded,
        observed_at: heartbeat_at,
        metadata: serde_json::json!({"network": "limited"}),
    },
    recorded_at: heartbeat_at,
})?;

let fresh = registry.node_health_status_at(&node_id, now + Duration::seconds(29))?;
assert_eq!(fresh.freshness, HeartbeatFreshness::Fresh);

let stale = registry.node_health_status_at(&node_id, now + Duration::seconds(35))?;
assert_eq!(stale.freshness, HeartbeatFreshness::Stale);

let events = audit.events()?;
assert_eq!(events[0].kind.event_class(), "node.registered");
assert_eq!(events[1].kind.event_class(), "instance.registered");
assert_eq!(events[2].kind.event_class(), "node.heartbeat_recorded");
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Validation commands

```bash
cargo test -p splendor-types
cargo test -p splendor-kernel node_registry
```

Before merging the sprint, run the full Rust checks:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Expected audit behavior

Successful registration/heartbeat emits management audit events in operation
order:

1. `node.registered`
2. `instance.registered`
3. `node.heartbeat_recorded`
4. `instance.heartbeat_recorded`, when an instance heartbeat is recorded

Invalid capability documents, duplicate IDs, unknown parent nodes, timestamp
regressions, and audit sink failures fail closed without applying the mutation.

## State and replay behavior

The registry does not create state graph nodes. It stores explicit management
metadata and mutable health records. Replay can inspect audit events but must not
re-register nodes, refresh heartbeats, start runs, or execute side effects.

## Non-goals

- No signed work orders.
- No scheduler placement decisions.
- No remote fleet transport.
- No central trace aggregation.
- No fleet telemetry dashboard.
- No robotics/device safety verifier.
