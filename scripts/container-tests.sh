#!/usr/bin/env bash
set -euo pipefail

IMAGE="${SPLENDOR_CONTAINER_IMAGE:-splendor:ci}"
BUILD_IMAGE="${SPLENDOR_CONTAINER_BUILD:-1}"
IMAGE_VERSION="${SPLENDOR_IMAGE_VERSION:-0.02-dev}"

if [[ "${BUILD_IMAGE}" == "1" ]]; then
  docker build \
    --build-arg "SPLENDOR_IMAGE_VERSION=${IMAGE_VERSION}" \
    -t "${IMAGE}" \
    .
fi

docker run --rm "${IMAGE}" sh -c '
set -eu

RUN_ID="22222222-2222-2222-2222-222222222222"
TRACE_DB="examples/local-basic-loop/data/trace.db"
STATE_DB="examples/local-basic-loop/data/state.db"

test "$(id -u)" != "0"
test "$(id -un)" = "splendor"

splendorctl --version
python -c "import splendor; print(f\"splendor python sdk {splendor.__version__} ({splendor.__baseline__})\")"

rm -f "${TRACE_DB}" "${STATE_DB}" examples/local-basic-loop/data/tick_*.txt
splendorctl run --config ./examples/local-basic-loop/config.yaml --cycles 1
splendorctl trace export --db "${TRACE_DB}" --run "${RUN_ID}" >/tmp/splendor-container-trace.jsonl
splendorctl state head --db "${TRACE_DB}" --run "${RUN_ID}" >/tmp/splendor-container-state-head.json
splendorctl replay --db "${TRACE_DB}" --state-db "${STATE_DB}" --run "${RUN_ID}" >/tmp/splendor-container-replay.jsonl

test -s /tmp/splendor-container-trace.jsonl
test -s /tmp/splendor-container-state-head.json
test -s /tmp/splendor-container-replay.jsonl
'

runtime_user="$(docker inspect "${IMAGE}" --format '{{.Config.User}}')"
image_license="$(docker inspect "${IMAGE}" --format '{{index .Config.Labels "org.opencontainers.image.licenses"}}')"
image_version="$(docker inspect "${IMAGE}" --format '{{index .Config.Labels "org.opencontainers.image.version"}}')"

if [[ "${runtime_user}" != "splendor" ]]; then
  printf 'expected Docker image to run as splendor user, got %s\n' "${runtime_user}" >&2
  exit 1
fi

if [[ "${image_license}" != "Apache-2.0 OR MIT" ]]; then
  printf 'expected OCI license label to be Apache-2.0 OR MIT, got %s\n' "${image_license}" >&2
  exit 1
fi

if [[ -z "${image_version}" ]]; then
  printf 'expected OCI version label to be set\n' >&2
  exit 1
fi

daemon_container="$(docker run -d "${IMAGE}" splendor-daemon)"
sleep 2
daemon_logs="$(docker logs "${daemon_container}" 2>&1)"
docker rm -f "${daemon_container}" >/dev/null

if [[ "${daemon_logs}" != *"local-only insecure dev mode"* || "${daemon_logs}" != *"127.0.0.1:8077"* ]]; then
  printf 'expected daemon to preserve local-only insecure-mode warning, got:\n%s\n' "${daemon_logs}" >&2
  exit 1
fi
