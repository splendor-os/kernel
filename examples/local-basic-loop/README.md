# Local Basic Loop

This is the 0.01-dev reference quickstart. It runs one local agent tick through:

```text
percept -> policy -> constraints -> gateway -> filesystem adapter -> outcome -> state commit -> trace
```

The filesystem write is mediated by the action gateway; the example does not
perform side effects directly from policy code.

## Run exactly

From a clean checkout:

```bash
cargo build -p splendorctl
rm -f ./examples/local-basic-loop/data/trace.db ./examples/local-basic-loop/data/state.db ./examples/local-basic-loop/data/tick_*.txt
./target/debug/splendorctl --version
./target/debug/splendorctl run --config ./examples/local-basic-loop/config.yaml --cycles 1
./target/debug/splendorctl trace export --db ./examples/local-basic-loop/data/trace.db --run 22222222-2222-2222-2222-222222222222
./target/debug/splendorctl state head --db ./examples/local-basic-loop/data/trace.db --run 22222222-2222-2222-2222-222222222222
./target/debug/splendorctl replay --db ./examples/local-basic-loop/data/trace.db --state-db ./examples/local-basic-loop/data/state.db --run 22222222-2222-2222-2222-222222222222
```

Equivalent CI smoke path:

```bash
bash scripts/verify-0.01-baseline.sh
```

## Expected evidence

- `data/tick_1.txt` is written by the gated filesystem adapter.
- `trace export` includes `RunStarted`, `LoopTickStarted`, `PerceptsReceived`,
  `StateLoaded`, `PolicyInvoked`, `PolicyCompleted`, `CandidatesProposed`,
  `ConstraintsEvaluated`, `ActionVerificationStarted`, `ActionVerificationCompleted`,
  `ActionExecuted`, `OutcomeRecorded`, `StateCommitted`, and
  `LoopTickCompleted`.
- `state head` prints the latest state hash from the `StateCommitted` event.
- `replay` reconstructs the tick from trace/state data and does not repeat the
  filesystem write.

## Non-goals

- No fleet registry or remote messaging.
- No approval/governance workflow engine.
- No physical/device orchestration.
