# Trace Store

Trace stores persist the ordered event stream for each run and provide integrity
hashes for auditability. Implementations live in `crates/splendor-store`.

## TraceRecord

**Fields**
- `run_id` (`String`): run identifier.
- `sequence` (`u64`): monotonic sequence number.
- `payload` (`serde_json::Value`): serialized trace event payload.
- `recorded_at` (`OffsetDateTime`): timestamp at storage time.
- `event_hash` (`ContentHash`): hash derived from the previous hash and payload.
- `prev_event_hash` (`Option<ContentHash>`): previous event hash in the chain.

## Integrity Chain

`event_hash` is computed as:

```
event_hash = blake3(prev_hash_string || payload_bytes)
```

Where `prev_hash_string` is the `ContentHash` string form (`algorithm:value`) of
the previous event. For the first record, `prev_event_hash` is `None` and the
hash is computed from payload bytes alone. For `LoopTickCompleted`, the payload
is normalized with `integrity` removed so the hash does not include itself.

## TraceStore

Synchronous storage interface:

```
append(run_id, payload) -> sequence
read(run_id) -> Vec<TraceRecord>
read_range(run_id, start, end) -> Vec<TraceRecord>
```

## AsyncTraceStore

Async wrapper with the same semantics, returning futures for each operation.

## InMemoryTraceStore

Holds trace records in memory keyed by `run_id`. The `sequence` is derived from
vector length at append time.

## SqliteTraceStore

SQLite-backed store that persists trace records on disk. The schema includes:

- `trace_events`: `run_id`, `sequence`, `payload`, `recorded_at`, `event_hash_*`,
  and `prev_hash_*` columns.

## Trace Export Tool

Use `splendorctl trace export --db <path> --run <id>` to emit JSON Lines for a
run. Each line is a serialized `TraceRecord`.

## Example

```rust
use splendor_store::{InMemoryTraceStore, TraceStore};

let store = InMemoryTraceStore::default();
let seq = TraceStore::append(&store, "run-1", serde_json::json!({"event": 1}))
    .expect("append");
assert_eq!(seq, 0);
let records = TraceStore::read(&store, "run-1").expect("read");
assert_eq!(records.len(), 1);
```
