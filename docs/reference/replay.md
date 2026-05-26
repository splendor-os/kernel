# Replay Reference

Replay reconstructs local 0.01-dev runs from persisted trace and state data. It
is inspect-only by default.

## CLI contract

```bash
splendorctl replay --db <trace-path> --state-db <state-path> --run <run-id> [--from-snapshot <id>] [--include-state]
```

Replay emits JSON Lines:

- `replay_start`: requested run and optional starting snapshot.
- `tick`: reconstructed policy name, percepts, candidate actions, verification
  result, action statuses, message lifecycle decisions, outcome payload,
  feedback/reward, state hash, and snapshot metadata.

## Side-effect suppression

Replay does not invoke perceptors, policies, gateways, verifiers, or adapters.
Filesystem, HTTP, network, database, webhook, shell, and external-service side
effects are never repeated by default.
Local message decisions are reconstructed from trace events; replay does not
re-deliver messages or mutate router inbox/outbox state.

There is no side-effectful replay mode in 0.01-dev. Future safe simulation modes
must be named explicitly, separately gated, and off by default.

## Validation

Before reconstructing ticks, replay validates:

- trace records are scoped to the requested run;
- sequence numbers are contiguous from zero;
- each serialized `TraceEvent` run and sequence match the stored record;
- each `trace_id` matches the deterministic run/sequence derivation;
- trace hash-chain continuity through `prev_event_hash`;
- referenced snapshots can be loaded from the state store.

## Failure modes

Replay fails with a clear error when:

- trace or state database path is missing;
- run ID is absent from the trace store;
- a trace record cannot be decoded;
- a trace segment is missing or corrupted;
- trace run/sequence/ID validation fails;
- the requested `--from-snapshot` is not in the trace history;
- a referenced snapshot is missing from the state store.

## Python SDK

`KernelRuntime.replay_run(run_id)` returns a deep copy of in-memory trace events
for local SDK runs. It validates event sequence/run scope and does not invoke
adapters again.
