# 0.03-S7 — State handoff v0

## Objective

Strengthen the state graph, trace store, and replay primitives so state can move
between instances only through explicit snapshot handoff or read-only reference
boundaries. This sprint implements FR-0.03-09 and the handoff-related trace
continuity portion of FR-0.03-10.

## Functional scope

- Added `StateHandoff`, `StateReference`, authority, snapshot, and trace-context
  schemas.
- Added state-store snapshot export/import with hash and parent-linkage
  validation.
- Added state-graph import validation for signed work-order authority, tenant,
  agent, run, previous head, and source trace continuity.
- Added read-only reference attachment and explicit mutation denial.
- Added replay recognition of handoff boundaries.

## Non-goals

- No distributed mutable state.
- No CRDT system.
- No automatic conflict resolution or merge.
- No full runtime migration engine.
- No fleet scheduler, placement, governance, or physical/edge behavior.

## Public contracts changed

- Rust schemas: `StateHandoff`, `StateReference`, `StateReferenceMode`,
  `StateHandoffTraceContext`.
- State store trait: `get_node`, `export_snapshot`, `import_handoff_snapshot`.
- State graph API: `export_handoff`, `import_handoff`,
  `attach_read_only_reference`, and `commit_from_read_only_reference`.
- Trace event variants: `StateHandoffExported`, `StateHandoffImported`,
  `StateHandoffImportFailed`, and `ReadOnlyStateReferenced`.
- `splendorctl replay` emits `handoff_boundary` records.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | none |
| Verifier | none |
| State graph | added explicit snapshot import and read-only reference behavior |
| Trace store | added handoff trace event payloads |
| Replay | added handoff boundary inspection |
| Message | none |
| Work order | reused signed run-resume/state-read authority checks |
| Governance | none |

## Trace behavior

New canonical event classes:

- `state.handoff.exported`
- `state.handoff.imported`
- `state.handoff.import_failed`
- `state.reference.read_only`

Source export and receiver import events share `handoff_id`. Receiver events
include `previous_state_node_id` and, for successful imports,
`receiver_state_node_id`. Import failures record a reason before any receiver
head update.

## State behavior

- Export reads an existing snapshot and source node metadata.
- Import verifies snapshot ID, byte hash, source node ID, parent IDs, work order,
  source trace continuity, and expected previous receiver head.
- Receiver state head updates only after validation and local node/snapshot
  persistence succeed.
- Read-only references are stored separately from the mutable head and cannot be
  committed from.

## Gateway and verifier behavior

No adapter or side-effect path was added. Handoff authority validation fails
closed on unsigned, expired, revoked, wrong-scope, wrong-tenant, wrong-agent,
wrong-run, or wrong-work-order inputs.

## Replay behavior

Replay remains inspect-only. It surfaces `handoff_boundary` JSON lines for
handoff events and never imports state, calls policy, invokes the gateway, or
executes adapters.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | state handoff schema and trace context round trips | `cargo test -p splendor-types state_handoff` |
| unit | snapshot export/import validates parent linkage and hash | `cargo test -p splendor-store state_store_exports_and_imports_handoff_snapshot_with_parent_linkage` |
| negative | corrupted snapshot is rejected before import | `cargo test -p splendor-store state_store_rejects_corrupted_handoff_snapshot_before_import` |
| negative | mismatched authority, stale head, missing trace fail closed | `cargo test -p splendor-kernel state_graph_rejects_*handoff*` |
| negative | unsigned, expired, revoked, wrong-scope, wrong-work-order, and unsupported schema fail closed | `cargo test -p splendor-kernel state_graph_rejects_invalid_handoff_work_orders_and_schema` |
| state | failed import leaves receiver head unchanged | `cargo test -p splendor-kernel state_graph_failed_handoff_import_leaves_receiver_head_unchanged` |
| replay | replay identifies handoff boundary | `cargo test -p splendorctl replay_identifies_state_handoff_boundary` |

Full workspace evidence should be gathered with `cargo test --workspace` before
merge.

## Example or fixture

See `examples/state-handoff-basic/README.md` for a minimal source/receiver flow.

## Future extension notes

Later migration work can wrap this primitive with scheduler and remote transport
logic. Fork/merge can extend the schema only with deterministic conflict rules;
0.03-S7 deliberately keeps ownership narrow and explicit.
