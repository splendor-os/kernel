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
- `tenants` (array, required): tenant definitions.
- `agents` (array, required): agent definitions.
- `adapters` (object, optional): adapter configuration.

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
