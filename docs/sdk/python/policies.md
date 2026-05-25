# Python SDK Policies and Perceptors

## Policy contract

A policy callback receives the current state bytes and normalized percepts:

```python
def policy(state: bytes, percepts: list[dict[str, object]]):
    return actions, next_state
```

Accepted return shapes:

- `([action_dict, ...], b"next-state")`
- `{"actions": [...], "state": b"next-state"}`
- `[action_dict, ...]` when state is unchanged

Policies propose actions only. They must not write files, call networks, mutate
databases, or perform other privileged side effects directly.

## Perceptor contract

Perceptors receive the `AgentContext` and return `Percept` objects or dictionaries:

```python
runtime.register_perceptor(
    agent_id,
    lambda agent: [
        {
            "schema": "splendor.example.input.v1",
            "payload": {"value": 1},
            "provenance": {"source": "example"},
        }
    ],
)
```

## Denial example

```python
runtime.register_constraints(agent_id, lambda state, percepts, actions: False)
outcome = runtime.run_once(agent_id)
assert outcome.action_outcomes[0].status == "denied"
```

## Trace evidence

Each run emits event dictionaries with `sequence`, `run_id`, `kind`, and
`payload`. The local SDK uses the same event classes as the 0.01 loop, including
`RunStarted`, `StateLoaded`, `PolicyInvoked`, `PolicyCompleted`, verification
events, action result events, `OutcomeRecorded`, `StateCommitted`, and
`LoopTickCompleted`.
Use `subscribe_traces` or `tail_traces` to inspect event classes and IDs rather
than opaque logs.
