#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="${SPLENDOR_E2E_OUTPUT:-target/splendor-e2e}"
case "${OUTPUT_DIR}" in
  /*) ;;
  *) OUTPUT_DIR="$(pwd)/${OUTPUT_DIR}" ;;
esac
REPORT="${OUTPUT_DIR}/0.03-kernel-e2e-report.json"
SOURCE_ID="$(git rev-parse --short HEAD 2>/dev/null || printf 'workspace-unknown')"

mkdir -p "${OUTPUT_DIR}"
rm -f "${REPORT}"

export SPLENDOR_E2E=1
export SPLENDOR_E2E_MODE="local-deterministic"
export SPLENDOR_E2E_OUTPUT="${OUTPUT_DIR}"
export SPLENDOR_E2E_SOURCE="${SOURCE_ID}"
export SPLENDOR_E2E_COMMANDS="bash scripts/verify-0.03-kernel-e2e.sh|cargo test -p splendor-daemon --test integration_kernel_e2e_0_03 -- --nocapture"

cargo test -p splendor-daemon --test integration_kernel_e2e_0_03 -- --nocapture

test -s "${REPORT}"
