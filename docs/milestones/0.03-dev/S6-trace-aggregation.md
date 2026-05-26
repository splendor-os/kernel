# 0.03-S6 — Trace aggregation

## 1. Objective

Implement the 0.03-S6 trace aggregation primitive: local trace buffers can sync
validated `TraceRecord` batches to a central index without reordering events,
duplicating records, or weakening trace hash-chain integrity.

## 2. Functional scope

- Added a trace sync batch contract over the existing `TraceRecord` format.
- Added an in-memory central trace index reference implementation.
- Added sync validation for ordering, missing segments, run identity, hash-chain
  continuity, event-hash recomputation, duplicate idempotency, and central
  conflicts.
- Added query support by fleet, node, instance, tenant, agent, run, tick, action,
  action ID, and work order where those values are available.
- Added a trace durability gateway wrapper for local policies that require
  central trace sync before side-effectful actions execute.

## 3. Non-goals

- No analytics dashboard.
- No long-term data warehouse design.
- No governance audit product.
- No remote transport, broker, or retry scheduler.
- No fleet registry, placement engine, or full telemetry view.
- No public trace event schema change to add action IDs.

## 4. Public contracts changed

New `splendor-store` exports:

- `TraceSyncScope`
- `TraceSyncBatch`
- `TraceSyncReport`
- `TraceSyncError`
- `TraceIndexQuery`
- `TraceIndexRecord`
- `TraceQuarantineEntry`
- `CentralTraceIndex`
- `InMemoryCentralTraceIndex`

New `splendor-kernel` exports:

- `TraceDurabilityPolicy`
- `TraceDurabilityState`
- `TraceDurabilityStatus`
- `TraceDurabilityGateway`

Documentation added:

- `docs/reference/trace-sync.md`
- `docs/reference/central-trace-index.md`
- `examples/resident-trace-sync/README.md`

## 5. Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | added trace durability wrapper for sync-required policies |
| Verifier | durability check denies before adapter execution when configured |
| State graph | none |
| Trace store | added sync batch and central index aggregation |
| Replay | unchanged; synced records remain replay-safe trace records |
| Message | none |
| Work order | indexed as optional metadata when available |
| Governance | none |

## 6. Trace behavior

- No trace event class was renamed or replaced.
- Sync uses existing `TraceRecord` values and validates their hash-chain fields.
- Central queries preserve per-run sequence order.
- Corrupted or conflicting batches are rejected and quarantined for inspection.
- Missing segments are reported as `TraceSyncError::MissingSegment`.

## 7. State behavior

- No state nodes are created by trace sync.
- No state head is updated by trace sync.
- State snapshots referenced by trace payloads remain external to this sprint.

## 8. Verifier/gateway behavior

- `TraceDurabilityGateway` wraps an existing `ActionGateway`.
- When `TraceDurabilityPolicy.require_central_sync_for_side_effects` is true,
  non-read-only actions are denied if central trace sync is stale or has a last
  sync error.
- The wrapped gateway is not called for denied side-effectful actions.
- Read-only actions are not blocked by this durability policy.

## 9. Replay behavior

- Trace sync does not invoke policies, adapters, or gateways.
- Replay remains inspect-only by default.
- Central trace records retain the same payloads and integrity fields as local
  trace records, so replay validation can reason over the same data.

## 10. Failure behavior

- Empty batch: rejected.
- Wrong run scope: rejected and quarantined.
- Payload run mismatch: rejected and quarantined.
- Missing sequence segment: rejected with expected/actual sequence.
- Chain mismatch: rejected and quarantined.
- Event hash mismatch: rejected and quarantined.
- Central conflict: rejected and quarantined.
- Trace durability required but stale: side-effectful action denied before the
  inner gateway or adapter executes.

## 11. Test evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `local_buffer_sync_preserves_order_across_partial_sync` | partial sync/reconnect preserves run order | `cargo test -p splendor-store` |
| `duplicate_sync_attempts_are_idempotent` | duplicate sync is a no-op | `cargo test -p splendor-store` |
| `missing_segments_are_reported_clearly` | missing ranges fail closed | `cargo test -p splendor-store` |
| `corrupted_trace_chain_is_rejected_and_quarantined` | corrupted chain is rejected | `cargo test -p splendor-store` |
| `mismatched_run_identity_is_rejected_and_quarantined` | wrong run identity is rejected | `cargo test -p splendor-store` |
| `central_index_queries_available_identity_dimensions` | central query dimensions work where present | `cargo test -p splendor-store` |
| `side_effectful_action_is_denied_when_trace_sync_required_and_stale` | sync failure blocks side effects under durability policy | `cargo test -p splendor-kernel trace_durability` |

## 12. Example commands or fixtures

See `examples/resident-trace-sync/README.md`.

Targeted validation commands:

```bash
cargo test -p splendor-store
cargo test -p splendor-kernel trace_durability
```

Full repository validation remains:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pytest python/tests
```

## 13. Future extension notes

- A future remote transport can ship `TraceSyncBatch` values without changing the
  validation rules.
- A future persistent central index can implement `CentralTraceIndex` while
  preserving `(run_id, sequence)` idempotency and hash-chain validation.
- Later fleet/work-order schemas can replace optional identity strings at the
  call boundary without changing the central record invariants.
- Governance audit export can query the central index, but governance workflows
  are intentionally out of scope for this sprint.
