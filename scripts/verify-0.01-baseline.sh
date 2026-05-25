#!/usr/bin/env bash
set -euo pipefail

TRACE_DB="examples/local-basic-loop/data/trace.db"
STATE_DB="examples/local-basic-loop/data/state.db"
RUN_ID="22222222-2222-2222-2222-222222222222"

mkdir -p examples/local-basic-loop/data
rm -f "${TRACE_DB}" "${STATE_DB}" examples/local-basic-loop/data/tick_*.txt

cargo build -p splendorctl
./target/debug/splendorctl --version
./target/debug/splendorctl run --config ./examples/local-basic-loop/config.yaml --cycles 1
./target/debug/splendorctl trace export --db "${TRACE_DB}" --run "${RUN_ID}" >/tmp/splendor-0.01-trace.jsonl
./target/debug/splendorctl state head --db "${TRACE_DB}" --run "${RUN_ID}" >/tmp/splendor-0.01-state-head.json
./target/debug/splendorctl replay --db "${TRACE_DB}" --state-db "${STATE_DB}" --run "${RUN_ID}" >/tmp/splendor-0.01-replay.jsonl

test -s /tmp/splendor-0.01-trace.jsonl
test -s /tmp/splendor-0.01-state-head.json
test -s /tmp/splendor-0.01-replay.jsonl
