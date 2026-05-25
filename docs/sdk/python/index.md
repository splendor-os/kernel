# Python SDK 0.01-dev

The Python SDK is an ergonomic local wrapper for the 0.01 runtime primitives. It
is not an enforcement bypass: policy code proposes actions, and
`KernelRuntime.run_once` performs the gateway-style checks before adapter
callbacks execute.

## Install for local development

```bash
python -m pip install -e python/
```

or run examples from the repository with:

```bash
PYTHONPATH=python python examples/python-sdk-basic/example.py
```

## Minimal flow

```python
from splendor import KernelRuntime

runtime = KernelRuntime()
tenant_id = runtime.create_tenant(
    allowed_actions=["noop"],
    allowed_adapters=["noop"],
)
agent_id = runtime.create_agent(tenant_id, snapshot_interval=1)

runtime.register_perceptor(agent_id, lambda agent: [])
runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
runtime.register_policy(
    agent_id,
    lambda state, percepts: [
        {"name": "noop", "adapter": "noop", "side_effect_class": "read_only", "params": {}}
    ],
)

outcome = runtime.run_once(agent_id)
assert outcome.action_outcomes[0].status == "executed"
```

## Public local hooks

- `create_tenant(...)`: tenant policy, allowed adapters/actions, and quotas.
- `create_agent(...)`: local agent context, initial state, run ID, snapshot cadence.
- `register_perceptor(agent_id, callback)`: percept intake.
- `register_policy(agent_id, callback)`: action proposal and next-state callback.
- `register_constraints(agent_id, callback)`: denial/fail-closed constraints.
- `register_adapter(adapter_id, callback)`: gated adapter callback.
- `subscribe_traces(run_id, callback)`: event subscription.
- `tail_traces(run_id)`: inspect recorded trace events.
- `replay_run(run_id)`: inspect-only replay from stored in-memory traces.

## Non-goals

- No distributed SDK client in 0.01-dev.
- No Python control-plane scheduler.
- No plugin marketplace pattern.
- No side-effect execution outside `KernelRuntime.run_once` in official examples.
