# State Handoff Basic

This example documents the 0.03-S7 state handoff reference path. It is a local
unit-test-backed flow, not a multi-host migration demo.

## What it proves

1. A source state graph commits state and creates a snapshot.
2. The source exports a `StateHandoff` envelope from that snapshot.
3. `KernelRuntime::record_state_handoff_exported` records the source trace event
   and writes the source trace ID into the handoff.
4. The receiver imports only when the signed work order authorizes the same
   tenant, agent, run, and `splendor.runs.resume` scope.
5. Snapshot ID, state hash, parent linkage, previous receiver head, and source
   trace continuity are verified before the receiver head changes.
6. Read-only references can be attached for inspection but cannot be mutated.
7. Replay emits a `handoff_boundary` record and does not import state or execute
   side effects.

## Smoke commands

```bash
cargo test -p splendor-store state_store_exports_and_imports_handoff_snapshot_with_parent_linkage
cargo test -p splendor-kernel state_graph_imports_valid_handoff_with_work_order_authority
cargo test -p splendor-kernel read_only_state_reference_cannot_be_mutated_by_receiver
cargo test -p splendorctl replay_identifies_state_handoff_boundary
```

## Expected trace behavior

- Source side: `state.handoff.exported`.
- Receiver success: `state.handoff.imported` with previous and receiver state
  heads.
- Receiver failure: `state.handoff.import_failed` before the receiver head
  changes.
- Read-only reference: `state.reference.read_only`.

## Not included

- No remote transport.
- No fleet scheduler or placement decision.
- No automatic conflict merge.
- No CRDT or distributed mutable state.
- No runtime migration engine.
