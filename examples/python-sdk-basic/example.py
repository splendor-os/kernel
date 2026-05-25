from __future__ import annotations

from splendor import Action, KernelRuntime


runtime = KernelRuntime()
tenant_id = runtime.create_tenant(
    allowed_actions=["allowed", "failed"],
    allowed_adapters=["local", "missing"],
)
agent_id = runtime.create_agent(tenant_id, snapshot_interval=1)
run_id = runtime.agent_run_id(agent_id)
events: list[dict[str, object]] = []
adapter_calls = {"count": 0}


def local_adapter(action: Action) -> dict[str, object]:
    adapter_calls["count"] += 1
    return {"output": {"handled": action.name}, "satisfied_postconditions": []}


def policy(state: bytes, percepts: list[dict[str, object]]):
    return [
        {
            "name": "allowed",
            "adapter": "local",
            "side_effect_class": "read_only",
            "params": {},
        },
        {
            "name": "not_allowed",
            "adapter": "local",
            "side_effect_class": "read_only",
            "params": {},
        },
        {
            "name": "failed",
            "adapter": "missing",
            "side_effect_class": "read_only",
            "params": {},
        },
    ], b"sdk-complete"


runtime.register_adapter("local", local_adapter)
runtime.register_perceptor(
    agent_id,
    lambda agent: [
        {
            "schema": "splendor.example.sdk_input.v1",
            "payload": {"objective": "show allowed, denied, and failed actions"},
            "provenance": {"source": "examples/python-sdk-basic"},
        }
    ],
)
runtime.register_policy(agent_id, policy)
runtime.subscribe_traces(run_id, events.append)

outcome = runtime.run_once(agent_id)
replay = runtime.replay_run(run_id)

print("statuses", [item.status for item in outcome.action_outcomes])
print("trace_events", [event["kind"] for event in events])
print("replay_events", len(replay))
print("adapter_calls", adapter_calls["count"])
