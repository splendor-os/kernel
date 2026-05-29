# Circuit Breaker Basic Example

This example demonstrates the 0.04-S4 local config path for circuit breakers. A
policy proposes a filesystem write, but a tripped adapter-scoped breaker denies
the action inside the Action Gateway before the filesystem adapter executes.

## What this proves

- The breaker is scoped to the `filesystem` adapter.
- Gateway verification returns `ActionStatus::Denied` with
  `circuit_breaker_tripped`.
- The filesystem adapter is not called and no file is written.
- Replay reports the breaker ID and scope from stored trace artifacts.

## Run the complete local config

From the repository root:

```bash
cargo run -p splendorctl -- run --config examples/circuit-breaker-basic/config.yaml --cycles 1
cargo run -p splendorctl -- replay \
  --db examples/circuit-breaker-basic/trace.db \
  --state-db examples/circuit-breaker-basic/state.db \
  --run 44444444-4444-4444-4444-444444444444
```

The example uses `allow_unsigned_local_run: true` because it is a local developer
fixture. Production and daemon paths must use authenticated callers and signed
work orders.

## Minimal config fragment

Add the breaker to a `splendorctl run` config:

```yaml
allow_unsigned_local_run: true # local development only

circuit_breakers:
  - id: cb_filesystem_adapter
    scope: adapter
    value: filesystem
    state: tripped
    reason: filesystem disabled during incident
    authorized_by: operator:alice
```

A complete local config is provided in `config.yaml`. The relevant policy action
looks like this:

```yaml
policy:
  type: static
  actions:
    - name: write_file
      adapter: filesystem
      side_effect_class: filesystem
      params:
        path: blocked.txt
        contents: blocked
```

## Expected trace behavior

The tick contains the normal verification and denial sequence:

```text
verification.started
verification.completed
action.denied
outcome.recorded
state.committed
tick.completed
```

The `ActionDenied` event includes breaker evidence similar to:

```json
{
  "reasons": ["circuit_breaker_tripped"],
  "artifacts": {
    "circuit_breaker": {
      "circuit_breaker": {
        "breaker_id": "cb_filesystem_adapter",
        "scope": "adapter",
        "scope_value": "filesystem",
        "state": "tripped",
        "reason": "filesystem disabled during incident"
      }
    }
  }
}
```

## Replay check

Run replay against the trace and state stores:

```bash
splendorctl replay --db ./trace.db --state-db ./state.db --run <run-id>
```

Replay remains inspect-only and emits `side_effects_replayed: false`. The tick
output includes `circuit_breaker_denials` with the breaker ID, scope, and reason.

## Non-goals

- No approval workflow is involved.
- No escalation engine automatically trips the breaker.
- No central policy distribution or TTL cache is required.
- No daemon management API or UI is demonstrated.
