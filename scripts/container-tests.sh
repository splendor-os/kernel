#!/usr/bin/env bash
set -euo pipefail

export PATH="/usr/local/cargo/bin:${PATH}"
export CARGO_TARGET_DIR="/tmp/splendor-target"

/opt/splendor-venv/bin/python -m pytest python/tests --cov=splendor --cov-report=term-missing --cov-fail-under=95
cargo test --workspace
