# Resident Trace Sync Example

This example documents the 0.03-S6 reference trace aggregation path. A resident
or remote Splendor instance writes trace events to a local `TraceStore`, then
syncs a validated `TraceSyncBatch` into a central `CentralTraceIndex`.

## What this proves

- Local trace records can be buffered and synced later.
- Partial sync preserves per-run sequence ordering.
- Duplicate sync attempts are idempotent.
- Missing segments, run mismatches, and corrupted hash chains fail closed.
- A local policy can require central trace durability before side-effectful
  actions are allowed through the gateway.

## Smoke commands

Run the targeted sprint tests:

```bash
cargo test -p splendor-store trace_sync
cargo test -p splendor-kernel trace_durability
```

Run full Rust validation:

```bash
cargo test --workspace
```

## Minimal Rust shape

```rust,no_run
use splendor_store::{
    CentralTraceIndex, InMemoryCentralTraceIndex, InMemoryTraceStore, TraceStore,
    TraceSyncBatch, TraceSyncScope,
};

let local_trace_store = InMemoryTraceStore::default();
TraceStore::append(&local_trace_store, "run-1", serde_json::json!({"run_id": "run-1"}))?;

let scope = TraceSyncScope {
    fleet_id: Some("fleet-dev".to_string()),
    node_id: Some("node-local".to_string()),
    instance_id: Some("instance-local".to_string()),
    tenant_id: Some("tenant-dev".to_string()),
    agent_id: Some("agent-dev".to_string()),
    run_id: "run-1".to_string(),
    work_order_id: Some("wo-dev".to_string()),
};

let central_index = InMemoryCentralTraceIndex::default();
let batch = TraceSyncBatch::from_store(scope, &local_trace_store, 0, 1)?;
central_index.sync_batch(batch)?;
```

## Durability-gated side effects

When a local policy requires central trace durability, wrap the real gateway with
`TraceDurabilityGateway`. If the central watermark is behind the local trace
buffer or the last sync failed, non-read-only actions are denied before adapter
execution.

This is a gateway denial, not a separate side-effect path.

## Non-goals

- No network transport implementation.
- No analytics UI or trace warehouse.
- No governance approval/audit product.
- No fleet registry or placement behavior.
