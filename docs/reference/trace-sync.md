# Trace Sync Reference

Trace sync is the 0.03-S6 reference path for moving trace records from a local
runtime buffer into a central index without changing the trace event contract.
It uses the existing `TraceRecord` shape from `splendor-store`, including
`run_id`, `sequence`, `payload`, `event_hash`, and `prev_event_hash`.

## Primitive strengthened

- Trace store
- Replay/audit support
- Fleet/node identity metadata where available

## Public contracts

Implemented in `crates/splendor-store`:

- `TraceSyncScope`
- `TraceSyncBatch`
- `TraceSyncReport`
- `TraceSyncError`
- `CentralTraceIndex`
- `InMemoryCentralTraceIndex`

Implemented in `crates/splendor-kernel`:

- `TraceDurabilityPolicy`
- `TraceDurabilityState`
- `TraceDurabilityStatus`
- `TraceDurabilityGateway`

## TraceSyncScope

`TraceSyncScope` attaches source identity to a batch:

```rust
pub struct TraceSyncScope {
    pub fleet_id: Option<String>,
    pub node_id: Option<String>,
    pub instance_id: Option<String>,
    pub tenant_id: Option<String>,
    pub agent_id: Option<String>,
    pub run_id: String,
    pub work_order_id: Option<String>,
}
```

`run_id` is required. Other fields are optional because earlier/later 0.03
sprints own the canonical fleet, node, instance, and work-order schemas. Trace
aggregation indexes those identifiers when the syncing runtime already has them;
it does not invent a fleet registry or placement model.

## TraceSyncBatch

`TraceSyncBatch` contains records read from a local trace buffer:

```rust
pub struct TraceSyncBatch {
    pub scope: TraceSyncScope,
    pub records: Vec<TraceRecord>,
}
```

The local `TraceStore` is the local buffer. A batch can be built from an existing
store range:

```rust
let batch = TraceSyncBatch::from_store(scope, &local_store, 0, 100)?;
central_index.sync_batch(batch)?;
```

`start` is inclusive and `end` is exclusive, matching `TraceStore::read_range`.

## Sync validation

The central index validates a batch before accepting any record:

1. The batch must contain at least one record.
2. Every record must match `TraceSyncScope.run_id`.
3. If the serialized trace payload has a root `run_id`, it must match the
   record's `run_id`.
4. Incoming records must be contiguous and ordered by `sequence`.
5. The first non-duplicate record must link to the existing central watermark or
   start at sequence `0`.
6. `prev_event_hash` must match the expected previous event hash.
7. `event_hash` is recomputed from the previous hash and normalized payload.
8. Existing central `(run_id, sequence)` records are accepted only when the
   incoming payload and hashes match exactly.

The sync path never repairs, renumbers, or rewrites trace records.

## Duplicate sync

Duplicate sync attempts are idempotent. If the central index already has a
record with the same `(run_id, sequence)`, payload, `event_hash`, and
`prev_event_hash`, that record is counted in `duplicate_records` and is not
inserted again.

## Rejection and quarantine

`TraceSyncError` reports fail-closed rejection reasons:

- `EmptyBatch`
- `RunIdentityMismatch`
- `PayloadRunIdentityMismatch`
- `MissingSegment`
- `ChainMismatch`
- `HashMismatch`
- `CentralConflict`
- `SequenceOverflow`
- `Poisoned`
- `Store`

Corruption and conflict errors are retained as `TraceQuarantineEntry` values in
the in-memory reference index for inspection. Missing segments are reported as a
clear error and are not quarantined because the expected recovery is to sync the
missing range first.

## Trace durability before side effects

0.03-S6 adds `TraceDurabilityGateway`, an `ActionGateway` wrapper. When local
policy sets `TraceDurabilityPolicy.require_central_sync_for_side_effects = true`,
non-read-only actions are denied before adapter execution unless
`TraceDurabilityState` shows that the central index is caught up with the local
trace buffer and no sync error is present.

This preserves the gateway boundary: the durability decision is represented as a
denied `ActionOutcome`, not as a side-effect path outside the gateway.

## Replay behavior

Trace sync does not execute policies, adapters, messages, or actions. It copies
validated `TraceRecord` data into a central index. Replay remains inspect-only by
default and can use the same record payloads and hash-chain metadata after sync.

## Security and failure notes

- Sync failure never authorizes a side effect.
- A runtime that requires central trace durability must wrap its action gateway
  with `TraceDurabilityGateway` or an equivalent fail-closed verifier.
- Queryable identity fields are metadata for audit and lookup. They do not grant
  permissions and do not replace signed work orders or gateway verification.

## Non-goals

- No analytics dashboard.
- No long-term warehouse design.
- No governance audit workflow.
- No remote transport or broker protocol.
- No fleet registry or placement engine.
