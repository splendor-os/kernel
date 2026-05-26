# Central Trace Index Reference

The central trace index is the 0.03-S6 aggregation target for local
`TraceRecord` batches. The reference implementation is
`InMemoryCentralTraceIndex` in `crates/splendor-store`.

## Interface

```rust
pub trait CentralTraceIndex: Send + Sync {
    fn sync_batch(&self, batch: TraceSyncBatch) -> Result<TraceSyncReport, TraceSyncError>;
    fn query(&self, query: &TraceIndexQuery) -> Result<Vec<TraceIndexRecord>, TraceSyncError>;
    fn latest_sequence(&self, run_id: &str) -> Result<Option<u64>, TraceSyncError>;
    fn quarantined(&self) -> Result<Vec<TraceQuarantineEntry>, TraceSyncError>;
}
```

The index is append-only at the `(run_id, sequence)` level. A duplicate record is
accepted only as an idempotent no-op when its payload and integrity hashes match
the existing central record.

## Indexed record

```rust
pub struct TraceIndexRecord {
    pub scope: TraceSyncScope,
    pub record: TraceRecord,
    pub tick_id: Option<u64>,
    pub action_id: Option<String>,
    pub action_name: Option<String>,
}
```

The `record` field is the original local trace record. `tick_id`, `action_id`,
and `action_name` are extracted from the serialized payload where available.
The current trace event schema has action names in action-related events; action
IDs are indexed if a payload includes them, but this sprint does not change the
trace event schema to add action IDs.

## Query dimensions

`TraceIndexQuery` can filter by:

- `fleet_id`
- `node_id`
- `instance_id`
- `tenant_id`
- `agent_id`
- `run_id`
- `tick_id`
- `action`
- `action_id`
- `work_order_id`

All fields are optional. Filters only match records where the corresponding
value is available. `action` matches either an extracted action name or action
identifier.

## Ordering

Queries return records in central storage order per run, which is sorted by
monotonic trace `sequence` after validation. The sync path rejects input batches
that skip sequence numbers; it does not sort malformed input to make it pass.

## Conflict handling

The central index rejects:

- records for a different run than the sync scope;
- payloads that claim a different root `run_id`;
- missing sequence segments;
- mismatched `prev_event_hash` values;
- recomputed `event_hash` mismatches;
- conflicting central records for the same `(run_id, sequence)`.

Corrupted or conflicting batches are visible through `quarantined()` for audit
and debugging. Missing segments are returned as `MissingSegment` so callers can
sync the missing range before retrying.

## Example

```rust
use splendor_store::{
    CentralTraceIndex, InMemoryCentralTraceIndex, TraceIndexQuery, TraceSyncBatch,
    TraceSyncScope,
};

let scope = TraceSyncScope::new("run-1");
let batch = TraceSyncBatch::from_store(scope, &local_trace_store, 0, 10)?;
let index = InMemoryCentralTraceIndex::default();
index.sync_batch(batch)?;

let records = index.query(&TraceIndexQuery {
    run_id: Some("run-1".to_string()),
    ..TraceIndexQuery::default()
})?;
```

## Compatibility notes

- The central index stores the same `TraceRecord` payloads produced by 0.01/0.02
  trace stores.
- Optional source identity strings are compatibility placeholders until the full
  0.03 fleet identity and work-order surfaces are present on `dev`.
- This reference index is not a warehouse, dashboard, search product, or
  governance audit exporter.
