# Daemon Client Local Example

This example documents the 0.02-S5 local runtime daemon API. It is intentionally
local-only and uses explicit insecure development mode on loopback. Do not use it
as a production transport.

## Run the reproducible smoke test

```bash
cargo test -p splendor-daemon
```

The integration tests create a run, append a percept, start a tick, pause,
resume, stop, inspect state-head, page traces, submit a denied action through the
gateway, and start inspect-only replay without increasing adapter execution
count.

## Start the local daemon binary

```bash
cargo run -p splendor-daemon
```

The daemon binds to `127.0.0.1:8077` and prints a warning that explicit local-only
insecure development mode is active.

## Minimal request shape

The daemon expects JSON. Run creation requires a signed, scoped work order:

```json
{
  "tenant_id": "00000000-0000-0000-0000-000000000001",
  "agent_id": "00000000-0000-0000-0000-000000000002",
  "work_order": {
    "work_order_id": "wo_local_example",
    "tenant_id": "00000000-0000-0000-0000-000000000001",
    "agent_id": "00000000-0000-0000-0000-000000000002",
    "run_id": null,
    "allowed_scopes": ["runs_create"],
    "signature": { "key_id": "local-dev", "signature": "signed-for-dev" },
    "expires_at": "2099-01-01T00:00:00Z",
    "revocation": "active"
  },
  "credential": null,
  "audit_attribution": {
    "principal": {
      "app": { "app_principal_id": "app_local", "label": null },
      "client_principal_id": "client_local",
      "label": null
    },
    "credential_id": null,
    "requested_at": "2099-01-01T00:00:00Z"
  },
  "allowed_actions": ["allowed_action"],
  "allowed_adapters": ["daemon.local"],
  "allowed_permissions": [],
  "policy_actions": [],
  "registered_actions": [{ "name": "allowed_action", "adapter": "daemon.local" }],
  "allowed_percept_schemas": ["splendor.percept.test.v1"],
  "allowed_percept_sources": ["daemon-client-local"],
  "initial_state": { "seed": true },
  "snapshot_interval": 1
}
```

## Expected trace/state behavior

- `POST /runs` emits `RunStarted`.
- `POST /runs/:run_id/percepts` accepts only allowed schemas/provenance and emits
  `PerceptsAppended`.
- `POST /runs/:run_id/start` runs one tick; the queued percept appears in
  `PerceptsReceived`.
- `GET /runs/:run_id/state-head` returns a state node verified through the state
  store.
- `GET /runs/:run_id/traces?redaction_policy=none` returns ordered records.
- `POST /runs/:run_id/replay` returns `inspect_only` replay metadata and does not
  execute adapters.

## What is intentionally not allowed

- No anonymous non-dev daemon calls.
- No unauthenticated remote TCP binding.
- No `/actions` path that bypasses `VerifiedActionGateway`.
- No side-effectful replay mode.
- No fleet registry or scheduling.
