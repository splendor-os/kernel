import time

import splendor

from splendor import KernelRuntime, KernelRuntimeConfig, QuotaPolicy
from splendor.runtime import (
    Action,
    ActionCandidate,
    Constraint,
    Percept,
    QuotaLedger,
    QuotaUsage,
    TenantPolicy,
    VerificationResult,
)


def test_record_trace() -> None:
    runtime = KernelRuntime(KernelRuntimeConfig(name="test"))
    event = runtime.record_trace("boot", "hello")
    assert event["sequence"] == 0
    assert event["message"] == "hello"
    assert event["runtime"] == "test"


def test_default_trace_sink_prints(capsys) -> None:
    runtime = KernelRuntime()
    event = runtime.record_trace("boot", "hello")
    captured = capsys.readouterr()
    assert "hello" in captured.out
    assert event["runtime"] == "splendor"


def test_custom_trace_sink_receives_events() -> None:
    events = []

    def sink(event: dict[str, object]) -> None:
        events.append(event)

    runtime = KernelRuntime(KernelRuntimeConfig(name="custom", trace_sink=sink))
    first = runtime.record_trace("tick", "one")
    second = runtime.record_trace("tick", "two")

    assert runtime.name == "custom"
    assert first["sequence"] == 0
    assert second["sequence"] == 1
    assert len(events) == 2
    assert events[1]["message"] == "two"


def test_run_once_executes_policy_action() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id, state=b"\x00", snapshot_interval=1)

    runtime.register_adapter(
        "noop",
        lambda action: {"output": {"ok": True}, "satisfied_postconditions": []},
    )
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        action = {
            "name": "noop",
            "params": {},
            "side_effect_class": "read_only",
            "adapter": "noop",
        }
        return [action], b"\x01"

    runtime.register_policy(agent_id, policy)
    outcome = runtime.run_once(agent_id)

    assert outcome.tick_id == 1
    assert outcome.state == b"\x01"
    assert outcome.action_outcomes[0].status == "executed"


def test_constraints_deny_actions() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)

    runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
            }
        ]

    runtime.register_policy(agent_id, policy)
    runtime.register_constraints(agent_id, lambda state, percepts, actions: False)
    outcome = runtime.run_once(agent_id)

    assert outcome.action_outcomes[0].status == "denied"


def test_start_and_stop_runs_ticks() -> None:
    runtime = KernelRuntime(KernelRuntimeConfig(tick_interval=0.01))
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)

    runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
            }
        ]

    runtime.register_policy(agent_id, policy)
    runtime.start(agent_id)
    time.sleep(0.03)
    runtime.stop(agent_id)
    assert runtime.agent_tick(agent_id) >= 1


def test_trace_subscription_and_tail() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)
    run_id = runtime.agent_run_id(agent_id)

    runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
            }
        ]

    runtime.register_policy(agent_id, policy)
    events: list[dict[str, object]] = []
    runtime.subscribe_traces(run_id, lambda event: events.append(event))
    runtime.run_once(agent_id)

    assert any(event["kind"] == "LoopTickCompleted" for event in events)
    tail = list(runtime.tail_traces(run_id))
    assert len(tail) >= len(events)
    assert tail[0]["run_id"] == run_id


def test_quota_denial_is_recorded() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
        quotas=QuotaPolicy(max_actions_per_tick=0),
    )
    agent_id = runtime.create_agent(tenant_id)

    runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
            }
        ]

    runtime.register_policy(agent_id, policy)
    outcome = runtime.run_once(agent_id)
    assert outcome.action_outcomes[0].status == "denied"
    assert "max_actions_per_tick" in outcome.action_outcomes[0].verification.reasons


def test_missing_adapter_is_failure() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
            }
        ]

    runtime.register_policy(agent_id, policy)
    outcome = runtime.run_once(agent_id)
    assert outcome.action_outcomes[0].status == "failed"
    assert outcome.action_outcomes[0].error == "adapter not registered"


def test_create_agent_requires_tenant() -> None:
    runtime = KernelRuntime()
    try:
        runtime.create_agent("missing")
    except ValueError as exc:
        assert "tenant not found" in str(exc)
    else:
        raise AssertionError("expected error")


def test_normalize_percepts_and_actions() -> None:
    runtime = KernelRuntime()

    percept = Percept(
        schema="sensor",
        payload={"k": 1},
        provenance={"source": "test"},
        timestamp=1.0,
    )
    normalized = runtime._normalize_percept(percept)
    assert normalized["schema"] == "sensor"

    normalized = runtime._normalize_percept({"payload": {"v": 2}})
    assert normalized["schema"] == ""
    assert normalized["payload"]["v"] == 2

    normalized = runtime._normalize_percept("raw")
    assert normalized["schema"] == "unknown"
    assert normalized["payload"]["value"] == "raw"

    action = Action(name="noop", params={}, side_effect_class="read_only")
    candidate = ActionCandidate(action=action, adapter="noop")
    assert runtime._normalize_action(candidate) is candidate

    normalized = runtime._normalize_action(action)
    assert normalized.action.name == "noop"

    normalized = runtime._normalize_action(
        {
            "name": "noop",
            "params": {},
            "side_effect_class": "read_only",
            "usage": {"actions": 2, "http_requests": 1},
            "adapter": "noop",
            "satisfied_preconditions": ["ready"],
        }
    )
    assert normalized.usage.actions == 2
    assert normalized.usage.http_requests == 1
    assert normalized.satisfied_preconditions == ["ready"]

    try:
        runtime._normalize_action(object())
    except ValueError as exc:
        assert "invalid action" in str(exc)
    else:
        raise AssertionError("expected error")


def test_policy_output_variants() -> None:
    runtime = KernelRuntime()
    action = {"name": "noop", "params": {}, "side_effect_class": "read_only"}
    candidates, next_state = runtime._normalize_policy_output(
        {"actions": [action], "state": b"\x02"}, b"\x01"
    )
    assert candidates[0].action.name == "noop"
    assert next_state == b"\x02"

    candidates, next_state = runtime._normalize_policy_output(
        ([action], b"\x03"), b"\x01"
    )
    assert next_state == b"\x03"
    assert candidates[0].action.name == "noop"

    candidates, next_state = runtime._normalize_policy_output([], b"\x01")
    assert candidates == []
    assert next_state == b"\x01"


def test_constraints_dict_and_verification_result() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)

    runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
            }
        ]

    runtime.register_policy(agent_id, policy)
    runtime.register_constraints(
        agent_id,
        lambda state, percepts, actions: {
            "allowed": False,
            "reasons": ["constraints_denied"],
            "constraints": [
                {"id": "c1", "kind": "hard", "scope": "action", "predicate": "no"},
                "raw",
            ],
        },
    )
    outcome = runtime.run_once(agent_id)
    assert outcome.action_outcomes[0].status == "denied"

    runtime.register_constraints(
        agent_id, lambda state, percepts, actions: VerificationResult.allow()
    )
    outcome = runtime.run_once(agent_id)
    assert outcome.action_outcomes[0].status == "executed"


def test_policy_verification_paths() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["ok"],
        allowed_adapters=["adapter"],
    )
    agent_id = runtime.create_agent(tenant_id)
    runtime.register_adapter("adapter", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {"name": "bad", "params": {}, "side_effect_class": "read_only"},
            {
                "name": "ok",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "missing",
            },
            {
                "name": "ok",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "adapter",
                "required_permissions": ["admin"],
            },
        ]

    runtime.register_policy(agent_id, policy)
    outcome = runtime.run_once(agent_id)
    reasons = [outcome.action_outcomes[i].verification.reasons[0] for i in range(3)]
    assert "action_not_allowed" in reasons
    assert "adapter_not_allowed" in reasons
    assert "permission_missing" in reasons


def test_preconditions_and_postconditions() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)
    runtime.register_adapter(
        "noop",
        lambda action: {"output": {"ok": True}, "satisfied_postconditions": []},
    )
    runtime.register_perceptor(agent_id, lambda agent: [])

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
                "preconditions": ["ready"],
                "satisfied_preconditions": [],
            },
            {
                "name": "noop",
                "params": {},
                "side_effect_class": "read_only",
                "adapter": "noop",
                "postconditions": ["done"],
            },
        ]

    runtime.register_policy(agent_id, policy)
    outcome = runtime.run_once(agent_id)
    assert outcome.action_outcomes[0].status == "denied"
    assert outcome.action_outcomes[0].verification.reasons == ["precondition_missing"]
    assert outcome.action_outcomes[1].status == "executed"
    assert outcome.needs_intervention


def test_quota_ledger_limits_and_reset() -> None:
    ledger = QuotaLedger()
    quotas = QuotaPolicy(
        max_actions_per_tick=0,
        max_action_duration_ms=0,
        max_filesystem_read_bytes=0,
        max_filesystem_write_bytes=0,
        max_network_read_bytes=0,
        max_network_write_bytes=0,
        max_http_requests_per_minute=0,
    )
    usage = QuotaUsage(
        actions=1,
        action_duration_ms=1,
        filesystem_read_bytes=1,
        filesystem_write_bytes=1,
        network_read_bytes=1,
        network_write_bytes=1,
        http_requests=1,
    )
    result = ledger.record_usage("agent", usage, quotas, time.time())
    assert not result.allowed
    assert "max_actions_per_tick" in result.reasons
    assert "max_http_requests_per_minute" in result.reasons

    ledger.begin_tick(1)
    allowed = ledger.record_usage(
        "agent",
        QuotaUsage(http_requests=0),
        QuotaPolicy(max_http_requests_per_minute=1),
        time.time() + 61,
    )
    assert allowed.allowed


def test_policy_output_accepts_action_dataclass() -> None:
    runtime = KernelRuntime()
    tenant_id = runtime.create_tenant(
        allowed_actions=["noop"],
        allowed_adapters=["noop"],
    )
    agent_id = runtime.create_agent(tenant_id)
    runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
    runtime.register_perceptor(agent_id, lambda agent: [])

    action = Action(name="noop", params={}, side_effect_class="read_only")

    def policy(state: bytes, percepts: list[dict[str, object]]):
        return [action]

    runtime.register_policy(agent_id, policy)
    outcome = runtime.run_once(agent_id)
    assert outcome.action_outcomes[0].status == "executed"


def test_verify_policy_direct() -> None:
    runtime = KernelRuntime()
    policy = TenantPolicy(allowed_actions=["ok"], allowed_adapters=["adapter"])
    action = Action(name="ok", params={}, side_effect_class="read_only")
    candidate = ActionCandidate(action=action, adapter="adapter")
    result = runtime._verify_policy(policy, candidate)
    assert result.allowed


def test_package_exports() -> None:
    assert "KernelRuntime" in splendor.__all__
    assert "KernelRuntimeConfig" in splendor.__all__
    assert "Action" in splendor.__all__
    assert "QuotaUsage" in splendor.__all__
    assert isinstance(splendor.__version__, str)
    assert splendor.__version__
