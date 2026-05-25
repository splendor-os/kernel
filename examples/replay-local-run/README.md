# Replay Local Run

This example documents the 0.01-dev replay contract using the local basic loop
fixture. Replay reads persisted trace/state data and emits reconstructed tick
JSON lines; it does **not** invoke policy code or adapters by default.

## Run

```bash
bash scripts/verify-0.01-baseline.sh
./target/debug/splendorctl replay --db ./examples/local-basic-loop/data/trace.db --state-db ./examples/local-basic-loop/data/state.db --run 22222222-2222-2222-2222-222222222222 --include-state
```

## Expected replay output

- A `replay_start` line with the run ID.
- One `tick` line containing the percepts, proposed action, verification result,
  action outcome, state hash, and snapshot byte length.
- The filesystem adapter is not called during replay. Existing output files are
  inspected only as external evidence, not rewritten by replay.

## Failure behavior

Replay fails closed when the trace database is missing, the state database is
missing, the requested run is absent, a trace event cannot be decoded, a trace
sequence is missing/corrupted, or a referenced snapshot cannot be loaded.
