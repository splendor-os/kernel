# splendorctl

`splendorctl` is a minimal operational tool for running and inspecting local
Splendor runs. It supports trace export, replay, and config-driven execution.

## trace export

Exports a run as JSON Lines (`.jsonl`) to stdout.

**Usage**
```
splendorctl trace export --db <path> --run <run-id>
```

**Arguments**
- `--db`: path to the SQLite trace database.
- `--run`: run identifier to export.

**Output**
Each line is a serialized `TraceRecord` containing the trace payload, sequence,
timestamps, and integrity hashes.

## replay

Reconstructs a run from trace + state stores and emits JSON lines describing
each tick without executing side effects.

**Usage**
```
splendorctl replay --db <trace-path> --state-db <state-path> --run <run-id> [--from-snapshot <id>] [--include-state]
```

**Arguments**
- `--db`: path to the SQLite trace database.
- `--state-db`: path to the SQLite state database.
- `--run`: run identifier to replay.
- `--from-snapshot`: optional snapshot id to start replay.
- `--include-state`: include snapshot bytes in output.

## run

Runs a local agent loop using a YAML/JSON config file.

**Usage**
```
splendorctl run --config <path> [--cycles <n> | --forever]
```

**Arguments**
- `--config`: path to a run config (`.yaml`, `.yml`, or `.json`).
- `--cycles`: number of cycles to run (defaults to config `cycles` or 1).
- `--forever`: run until interrupted.

See `docs/reference/run-config.md` for the config format.

## Example

```
./target/debug/splendorctl trace export --db ./trace.db --run run-1
```

```
./target/debug/splendorctl replay --db ./trace.db --state-db ./state.db --run run-1
```

```
./target/debug/splendorctl run --config ./examples/single_agent_loop/config.yaml --cycles 2
```
