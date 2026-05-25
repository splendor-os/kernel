# Single Agent Loop

This example runs a single agent with the filesystem adapter enabled. Each tick
writes a file into `examples/single_agent_loop/data` and records trace/state
data in SQLite.

The 0.01-dev reference quickstart is `examples/local-basic-loop`; this example
remains as a compatible two-cycle variant.

## Run

```
./target/debug/splendorctl run --config ./examples/single_agent_loop/config.yaml --cycles 2
```

## Inspect

```
./target/debug/splendorctl trace export --db ./examples/single_agent_loop/data/trace.db --run 22222222-2222-2222-2222-222222222222
```

```
./target/debug/splendorctl state head --db ./examples/single_agent_loop/data/trace.db --run 22222222-2222-2222-2222-222222222222
```

```
./target/debug/splendorctl replay --db ./examples/single_agent_loop/data/trace.db --state-db ./examples/single_agent_loop/data/state.db --run 22222222-2222-2222-2222-222222222222
```
