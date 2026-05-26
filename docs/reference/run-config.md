# Run Config

`splendorctl run` consumes a YAML/JSON config file that describes tenants,
agents, policies, and adapter settings.

## Top-level fields

- `trace_db` (string, required): SQLite path for trace events.
- `state_db` (string, required): SQLite path for state snapshots.
- `run_id` (string, optional): UUID to use for all agents unless overridden.
- `tick_budget_ms` (number, optional): tick budget in milliseconds.
- `tick_interval_ms` (number, optional): pacing interval per scheduler cycle.
- `cycles` (number, optional): default cycles to run when `--cycles` is omitted.
- `allow_unsigned_local_run` (bool, optional, default `false`): explicit
  local-development compatibility bypass for legacy unsigned configs. When
  omitted or `false`, `splendorctl run` requires a signed `work_order` and
  rejects missing work-order authority before state creation, policy invocation,
  gateway verification, or adapter execution.
- `tenants` (array, required): tenant definitions.
- `agents` (array, required): agent definitions.
- `adapters` (object, optional): adapter configuration.
- `work_order` (object, required unless `allow_unsigned_local_run: true`): signed
  work-order envelope for the local resident run. When present, it is validated
  before state store creation, perceptor collection, policy invocation, gateway
  verification, or adapter execution.

## Tenant fields

- `id` (string, required): UUID for the tenant.
- `allowed_actions` (array, required): action names allowed for this tenant.
- `allowed_adapters` (array, required): adapter ids allowed for this tenant.
- `allowed_permissions` (array, optional): permissions allowlist.
- `quotas` (object, optional): quota limits.

### Quota fields

- `max_actions_per_tick` (number)
- `max_action_duration_ms` (number)
- `max_filesystem_read_bytes` (number)
- `max_filesystem_write_bytes` (number)
- `max_network_read_bytes` (number)
- `max_network_write_bytes` (number)
- `max_http_requests_per_minute` (number)

## Agent fields

- `tenant_id` (string, required): UUID that matches a tenant.
- `id` (string, optional): UUID for the agent.
- `run_id` (string, optional): UUID for the agent run (overrides top-level).
- `snapshot_interval` (number, optional): snapshot every N ticks.
- `initial_state` (string, optional): initial state bytes (UTF-8 string).
- `resume` (bool, optional): resume from last snapshot in trace store.
- `percepts` (array, optional): static percept list.
- `policy` (object, required): policy configuration.

## Work-order fields

`work_order` is the 0.03-S3 local ingestion path for the canonical
`WorkOrderEnvelope` documented in [`work-orders.md`](work-orders.md). The CLI
uses it as scoped run authority, not as broad caller credentials.

Required fields:

- `schema_version`: `splendor.work_order.v1`.
- `work_order_id`: manager-issued work-order identifier, distinct from run IDs.
- `tenant_id`: tenant authorized by the work order.
- `agent_id`: agent authorized by the work order.
- `objective`: audit-readable objective string.
- `allowed_actions`, `allowed_adapters`, `allowed_permissions`: runtime scope
  delegated by the work order. These narrow tenant policy allowlists.
- `data_refs`: explicit data references in scope.
- `quotas`: per-run delegated quota limits. These narrow tenant quotas.
- `placement`: compatibility hints. `splendorctl` validates the target against
  `expected_placement_target`, defaulting to `local_resident`; it does not
  perform fleet placement.
- `issued_at`, `expires_at`: RFC3339 timestamps.
- `revocation`: `active` or a revoked marker.
- `signature`: detached signature metadata with `key_id` and `signature`.
- `verification_secret`: local fixture/test secret used to verify the reference
  `blake3-keyed-v1` signature. Do not put production secrets in config files.
- `expected_placement_target` (optional): local target compatibility override.

Invalid, unsigned/missing, expired, revoked, malformed, or incompatible work
orders fail closed before runtime execution. Missing work orders may emit a
sanitized `WorkOrderRejected` audit trace when an explicit run ID is available.
Signature validation failures emit a sanitized `WorkOrderRejected` audit trace
event without signature material or the verification secret. Valid work orders
emit `WorkOrderAccepted` and attach `work_order_id` to agent runtime metadata.

`allow_unsigned_local_run: true` exists only so older local quickstarts remain
runnable. It prints a warning and should not be used for resident, fleet,
remote, or production operation.

### Percepts

Each perceptor entry emits a static percept:

- `schema` (string)
- `payload` (object)
- `source` (string)
- `detail` (string, optional)

## Policy types

### `static`

Executes a fixed list of actions every tick.

```
policy:
  type: static
  actions:
    - name: write_file
      adapter: filesystem
      side_effect_class: filesystem
      params:
        path: "hello.txt"
        contents: "hi"
```

### `increment`

Increments a single-byte counter in state and optionally uses `{counter}`
substitution in action params.

```
policy:
  type: increment
  action:
    name: write_file
    adapter: filesystem
    side_effect_class: filesystem
    params:
      path: "tick_{counter}.txt"
      contents: "hello {counter}"
```

## Action fields

- `name` (string, required)
- `adapter` (string, optional): adapter id override.
- `params` (object, required)
- `side_effect_class` (string, optional): `filesystem`, `network`, `read_only`, `external`.
- `required_permissions` (array, optional)
- `preconditions` (array, optional)
- `postconditions` (array, optional)
- `usage` (object, optional): quota usage hints.
- `satisfied_preconditions` (array, optional)

### Usage fields

- `actions` (number)
- `action_duration_ms` (number)
- `filesystem_read_bytes` (number)
- `filesystem_write_bytes` (number)
- `network_read_bytes` (number)
- `network_write_bytes` (number)
- `http_requests` (number)

## Adapter config

```
adapters:
  filesystem:
    base_dir: ./data
    max_read_bytes: 1048576
    max_write_bytes: 1048576
    max_list_entries: 1000
  http:
    allowed_domains:
      - example.com
    allowed_methods:
      - GET
    max_request_bytes: 1048576
    max_response_bytes: 1048576
    timeout_ms: 5000
```

## Example

See `examples/single_agent_loop/config.yaml`.
