# Getting Started: Local Runtime

This is the exact 0.01-dev quickstart for the local Splendor kernel baseline.
It requires only Rust/Cargo and the repository checkout.

## 1. Build the CLI

```bash
cargo build -p splendorctl
```

## 2. Reset local example artifacts

```bash
rm -f ./examples/local-basic-loop/data/trace.db ./examples/local-basic-loop/data/state.db ./examples/local-basic-loop/data/tick_*.txt
```

## 3. Confirm version visibility

```bash
./target/debug/splendorctl --version
```

Expected shape:

```text
splendorctl 0.1.0 (Splendor0.01-dev)
```

## 4. Run one local tick

```bash
./target/debug/splendorctl run --config ./examples/local-basic-loop/config.yaml --cycles 1
```

The policy proposes a filesystem write, the gateway verifies the tenant policy
and quota, the filesystem adapter writes `examples/local-basic-loop/data/tick_1.txt`,
and the runtime commits state and trace records.

## 5. Export traces

```bash
./target/debug/splendorctl trace export --db ./examples/local-basic-loop/data/trace.db --run 22222222-2222-2222-2222-222222222222
```

The output is JSON Lines of `TraceRecord` values. The payloads include the local
tick sequence documented in `docs/reference/trace-events.md`.

## 6. Inspect state head

```bash
./target/debug/splendorctl state head --db ./examples/local-basic-loop/data/trace.db --run 22222222-2222-2222-2222-222222222222
```

The `state_hash` is the latest state node hash referenced by the run's
`StateCommitted` trace event.

## 7. Replay without side effects

```bash
./target/debug/splendorctl replay --db ./examples/local-basic-loop/data/trace.db --state-db ./examples/local-basic-loop/data/state.db --run 22222222-2222-2222-2222-222222222222
```

Replay reconstructs the tick from trace/state data and does not call the
filesystem adapter again.

## One-command smoke path

```bash
bash scripts/verify-0.01-baseline.sh
```

## Not available in 0.01-dev

- Typed local message router and multi-agent delegation are planned for 0.02.
- Fleet registry, signed work orders, remote messaging, and trace aggregation are
  planned for 0.03.
- Approval workflows, circuit breakers, kill switches, and policy TTL governance
  are planned for 0.04.
- Physical/edge device orchestration is planned for 0.05.
