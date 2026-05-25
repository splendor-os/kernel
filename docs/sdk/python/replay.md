# Python SDK Replay

`KernelRuntime.replay_run(run_id)` provides local, in-memory replay for SDK
examples and tests.

## Contract

```python
events = runtime.replay_run(run_id)
```

The method returns a deep copy of recorded trace events after validating:

- the run exists;
- event sequences are contiguous from zero;
- every event belongs to the requested run.

## Side-effect suppression

Replay does not invoke perceptors, policies, constraints, verifiers, or adapters.
It only copies stored trace events. Therefore, a filesystem/network/database
adapter registered in the SDK is not called during replay.

## Demonstration

```python
calls = {"count": 0}

def adapter(action):
    calls["count"] += 1
    return {"output": {"ok": True}}

runtime.register_adapter("local", adapter)
runtime.run_once(agent_id)
assert calls["count"] == 1
runtime.replay_run(runtime.agent_run_id(agent_id))
assert calls["count"] == 1
```

See `examples/python-sdk-basic/` and
`test_replay_run_does_not_repeat_adapter_side_effects`.
