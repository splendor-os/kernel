from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Iterable, Optional

import copy
import hashlib
import threading
import time
import uuid


CANONICAL_ID_FIELDS = (
    "fleet_id",
    "node_id",
    "instance_id",
    "tenant_id",
    "agent_id",
    "run_id",
    "tick_id",
    "action_id",
    "state_node_id",
    "trace_event_id",
    "message_id",
)


def _new_uuid() -> str:
    return str(uuid.uuid4())


def _validate_uuid_id(value: str, field: str) -> str:
    try:
        parsed = uuid.UUID(str(value))
    except (TypeError, ValueError) as exc:
        raise ValueError(f"{field} is invalid") from exc
    if parsed.int == 0:
        raise ValueError(f"{field} is required")
    return str(parsed)


def _trace_event_id(run_id: str, sequence: int) -> str:
    run_id = _validate_uuid_id(run_id, "run_id")
    return str(uuid.uuid5(uuid.NAMESPACE_OID, f"{run_id}:{sequence}"))


@dataclass(frozen=True)
class KernelRuntimeConfig:
    name: str = "splendor"
    trace_sink: Optional[Callable[[dict[str, Any]], None]] = None
    tick_interval: Optional[float] = None


@dataclass
class QuotaPolicy:
    max_actions_per_tick: Optional[int] = None
    max_action_duration_ms: Optional[int] = None
    max_filesystem_read_bytes: Optional[int] = None
    max_filesystem_write_bytes: Optional[int] = None
    max_network_read_bytes: Optional[int] = None
    max_network_write_bytes: Optional[int] = None
    max_http_requests_per_minute: Optional[int] = None


@dataclass
class QuotaUsage:
    actions: int = 0
    action_duration_ms: int = 0
    filesystem_read_bytes: int = 0
    filesystem_write_bytes: int = 0
    network_read_bytes: int = 0
    network_write_bytes: int = 0
    http_requests: int = 0

    @classmethod
    def single_action(cls) -> "QuotaUsage":
        return cls(actions=1)

    def accumulate(self, other: "QuotaUsage") -> None:
        self.actions = min(self.actions + other.actions, 2**31 - 1)
        self.action_duration_ms = min(
            self.action_duration_ms + other.action_duration_ms, 2**63 - 1
        )
        self.filesystem_read_bytes = min(
            self.filesystem_read_bytes + other.filesystem_read_bytes, 2**63 - 1
        )
        self.filesystem_write_bytes = min(
            self.filesystem_write_bytes + other.filesystem_write_bytes, 2**63 - 1
        )
        self.network_read_bytes = min(
            self.network_read_bytes + other.network_read_bytes, 2**63 - 1
        )
        self.network_write_bytes = min(
            self.network_write_bytes + other.network_write_bytes, 2**63 - 1
        )
        self.http_requests = min(self.http_requests + other.http_requests, 2**31 - 1)


@dataclass
class VerificationResult:
    allowed: bool
    reasons: list[str] = field(default_factory=list)
    artifacts: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def allow(cls) -> "VerificationResult":
        return cls(True)

    @classmethod
    def deny(
        cls, reason: str, artifacts: Optional[dict[str, Any]] = None
    ) -> "VerificationResult":
        return cls(False, [reason], artifacts or {})


@dataclass
class Constraint:
    id: str
    kind: str
    scope: str
    predicate: str
    obligation: Optional[str] = None


@dataclass
class Percept:
    schema: str
    payload: dict[str, Any]
    provenance: dict[str, Any]
    timestamp: float


@dataclass
class Action:
    name: str
    params: dict[str, Any]
    side_effect_class: str
    cost_estimate: Optional[dict[str, Any]] = None
    required_permissions: list[str] = field(default_factory=list)
    preconditions: list[str] = field(default_factory=list)
    postconditions: list[str] = field(default_factory=list)


@dataclass
class ActionCandidate:
    action: Action
    adapter: Optional[str] = None
    usage: QuotaUsage = field(default_factory=QuotaUsage.single_action)
    satisfied_preconditions: list[str] = field(default_factory=list)


@dataclass
class ActionOutcome:
    status: str
    verification: VerificationResult
    post_verification: Optional[VerificationResult]
    output: Optional[dict[str, Any]]
    error: Optional[str]
    action_id: Optional[str] = None


@dataclass
class TickOutcome:
    tick_id: int
    action_outcomes: list[ActionOutcome]
    duration_ms: int
    needs_intervention: bool
    state: bytes


@dataclass
class TenantPolicy:
    allowed_actions: list[str]
    allowed_adapters: list[str]
    allowed_permissions: list[str] = field(default_factory=list)


class QuotaLedger:
    def __init__(self) -> None:
        self.tick_id = 0
        self.tick_usage = QuotaUsage()
        self.per_agent_usage: dict[str, QuotaUsage] = {}
        self.http_window_start: float = time.time()
        self.http_requests: int = 0

    def begin_tick(self, tick_id: int) -> None:
        if tick_id != self.tick_id:
            self.tick_id = tick_id
            self.tick_usage = QuotaUsage()
            self.per_agent_usage = {}

    def record_usage(
        self,
        agent_id: str,
        usage: QuotaUsage,
        quotas: QuotaPolicy,
        now: float,
    ) -> VerificationResult:
        reasons: list[str] = []
        artifacts: dict[str, Any] = {}

        if quotas.max_actions_per_tick is not None:
            if self.tick_usage.actions + usage.actions > quotas.max_actions_per_tick:
                reasons.append("max_actions_per_tick")
                artifacts["actions_per_tick"] = {
                    "limit": quotas.max_actions_per_tick,
                    "observed": self.tick_usage.actions + usage.actions,
                }

        if quotas.max_action_duration_ms is not None:
            if usage.action_duration_ms > quotas.max_action_duration_ms:
                reasons.append("max_action_duration_ms")
                artifacts["action_duration_ms"] = {
                    "limit": quotas.max_action_duration_ms,
                    "observed": usage.action_duration_ms,
                }

        if quotas.max_filesystem_read_bytes is not None:
            if (
                self.tick_usage.filesystem_read_bytes + usage.filesystem_read_bytes
                > quotas.max_filesystem_read_bytes
            ):
                reasons.append("max_filesystem_read_bytes")
                artifacts["filesystem_read_bytes"] = {
                    "limit": quotas.max_filesystem_read_bytes,
                    "observed": self.tick_usage.filesystem_read_bytes
                    + usage.filesystem_read_bytes,
                }

        if quotas.max_filesystem_write_bytes is not None:
            if (
                self.tick_usage.filesystem_write_bytes + usage.filesystem_write_bytes
                > quotas.max_filesystem_write_bytes
            ):
                reasons.append("max_filesystem_write_bytes")
                artifacts["filesystem_write_bytes"] = {
                    "limit": quotas.max_filesystem_write_bytes,
                    "observed": self.tick_usage.filesystem_write_bytes
                    + usage.filesystem_write_bytes,
                }

        if quotas.max_network_read_bytes is not None:
            if (
                self.tick_usage.network_read_bytes + usage.network_read_bytes
                > quotas.max_network_read_bytes
            ):
                reasons.append("max_network_read_bytes")
                artifacts["network_read_bytes"] = {
                    "limit": quotas.max_network_read_bytes,
                    "observed": self.tick_usage.network_read_bytes
                    + usage.network_read_bytes,
                }

        if quotas.max_network_write_bytes is not None:
            if (
                self.tick_usage.network_write_bytes + usage.network_write_bytes
                > quotas.max_network_write_bytes
            ):
                reasons.append("max_network_write_bytes")
                artifacts["network_write_bytes"] = {
                    "limit": quotas.max_network_write_bytes,
                    "observed": self.tick_usage.network_write_bytes
                    + usage.network_write_bytes,
                }

        if quotas.max_http_requests_per_minute is not None:
            if now - self.http_window_start >= 60:
                self.http_window_start = now
                self.http_requests = 0
            if (
                self.http_requests + usage.http_requests
                > quotas.max_http_requests_per_minute
            ):
                reasons.append("max_http_requests_per_minute")
                artifacts["http_requests_per_minute"] = {
                    "limit": quotas.max_http_requests_per_minute,
                    "observed": self.http_requests + usage.http_requests,
                }

        if reasons:
            return VerificationResult(
                False,
                reasons,
                {"quota": {"context": {"agent_id": agent_id}, **artifacts}},
            )

        self.tick_usage.accumulate(usage)
        per_agent = self.per_agent_usage.get(agent_id)
        if per_agent is None:
            per_agent = QuotaUsage()
            self.per_agent_usage[agent_id] = per_agent
        per_agent.accumulate(usage)
        self.http_requests += usage.http_requests
        return VerificationResult.allow()


@dataclass
class TenantContext:
    tenant_id: str
    policy: TenantPolicy
    quotas: QuotaPolicy
    ledger: QuotaLedger = field(default_factory=QuotaLedger)


@dataclass
class AgentContext:
    agent_id: str
    tenant_id: str
    run_id: str
    state: bytes
    content_type: Optional[str]
    snapshot_interval: Optional[int]
    tick_id: int = 0
    perceptors: list[Callable[[AgentContext], Iterable[Any]]] = field(
        default_factory=list
    )
    policy: Optional[Callable[[bytes, list[dict[str, Any]]], Any]] = None
    constraints: Optional[Callable[..., Any]] = None


class KernelRuntime:
    def __init__(self, config: KernelRuntimeConfig | None = None) -> None:
        self._config = config or KernelRuntimeConfig()
        self._trace_sink = self._config.trace_sink or self._default_trace_sink
        self._sequence_by_run: dict[str, int] = {}
        self._trace_by_run: dict[str, list[dict[str, Any]]] = {}
        self._trace_subscribers: dict[str, list[Callable[[dict[str, Any]], None]]] = {}
        self._tenants: dict[str, TenantContext] = {}
        self._agents: dict[str, AgentContext] = {}
        self._adapters: dict[str, Callable[[Action], dict[str, Any]]] = {}
        self._snapshots: dict[str, bytes] = {}
        self._threads: dict[str, threading.Thread] = {}
        self._stops: dict[str, threading.Event] = {}

    @property
    def name(self) -> str:
        return self._config.name

    def create_tenant(
        self,
        *,
        tenant_id: Optional[str] = None,
        allowed_actions: Optional[list[str]] = None,
        allowed_adapters: Optional[list[str]] = None,
        quotas: Optional[QuotaPolicy] = None,
    ) -> str:
        tenant_id = _validate_uuid_id(tenant_id or _new_uuid(), "tenant_id")
        policy = TenantPolicy(
            allowed_actions=allowed_actions or [],
            allowed_adapters=allowed_adapters or [],
        )
        self._tenants[tenant_id] = TenantContext(
            tenant_id=tenant_id,
            policy=policy,
            quotas=quotas or QuotaPolicy(),
        )
        return tenant_id

    def create_agent(
        self,
        tenant_id: str,
        *,
        state: bytes = b"",
        content_type: Optional[str] = None,
        snapshot_interval: Optional[int] = None,
        run_id: Optional[str] = None,
    ) -> str:
        if tenant_id not in self._tenants:
            raise ValueError("tenant not found")
        agent_id = _new_uuid()
        run_id = _validate_uuid_id(run_id or _new_uuid(), "run_id")
        self._agents[agent_id] = AgentContext(
            agent_id=agent_id,
            tenant_id=tenant_id,
            run_id=run_id,
            state=state,
            content_type=content_type,
            snapshot_interval=snapshot_interval,
        )
        return agent_id

    def agent_run_id(self, agent_id: str) -> str:
        return self._agent_or_raise(agent_id).run_id

    def agent_state(self, agent_id: str) -> bytes:
        return self._agent_or_raise(agent_id).state

    def agent_tick(self, agent_id: str) -> int:
        return self._agent_or_raise(agent_id).tick_id

    def register_perceptor(
        self, agent_id: str, perceptor: Callable[[AgentContext], Iterable[Any]]
    ) -> None:
        self._agent_or_raise(agent_id).perceptors.append(perceptor)

    def register_policy(
        self,
        agent_id: str,
        policy: Callable[[bytes, list[dict[str, Any]]], Any],
    ) -> None:
        self._agent_or_raise(agent_id).policy = policy

    def register_constraints(
        self, agent_id: str, constraints: Callable[..., Any]
    ) -> None:
        self._agent_or_raise(agent_id).constraints = constraints

    def register_adapter(
        self, adapter_id: str, adapter: Callable[[Action], dict[str, Any]]
    ) -> None:
        self._adapters[adapter_id] = adapter

    def start(self, agent_id: str, tick_interval: Optional[float] = None) -> None:
        agent = self._agent_or_raise(agent_id)
        if agent_id in self._threads:
            return
        stop_event = threading.Event()
        self._stops[agent_id] = stop_event
        interval = (
            tick_interval if tick_interval is not None else self._config.tick_interval
        )

        def run_loop() -> None:
            while not stop_event.is_set():
                self.run_once(agent_id)
                if interval:
                    stop_event.wait(interval)

        thread = threading.Thread(target=run_loop, daemon=True)
        self._threads[agent_id] = thread
        thread.start()

    def stop(self, agent_id: str) -> None:
        stop_event = self._stops.get(agent_id)
        if stop_event is None:
            return
        stop_event.set()
        thread = self._threads.get(agent_id)
        if thread:
            thread.join(timeout=1)
        self._threads.pop(agent_id, None)
        self._stops.pop(agent_id, None)

    def run_once(self, agent_id: str) -> TickOutcome:
        agent = self._agent_or_raise(agent_id)
        tenant = self._tenants[agent.tenant_id]
        if agent.policy is None:
            raise ValueError("policy not registered")

        tick_id = agent.tick_id + 1
        agent.tick_id = tick_id
        tenant.ledger.begin_tick(tick_id)
        start = time.time()

        if self._sequence_by_run.get(agent.run_id, 0) == 0:
            self._record_trace(agent.run_id, "RunStarted", {})
        self._record_trace(agent.run_id, "LoopTickStarted", {"tick_id": tick_id})

        percepts = self._collect_percepts(agent)
        self._record_trace(
            agent.run_id,
            "PerceptsReceived",
            {"tick_id": tick_id, "count": len(percepts), "percepts": percepts},
        )

        self._record_trace(
            agent.run_id,
            "StateLoaded",
            {"tick_id": tick_id, "state_hash": hashlib.sha256(agent.state).hexdigest()},
        )
        policy_name = getattr(agent.policy, "__name__", "policy")
        self._record_trace(
            agent.run_id,
            "PolicyInvoked",
            {"tick_id": tick_id, "policy": policy_name},
        )
        policy_output = agent.policy(agent.state, percepts)
        actions, next_state = self._normalize_policy_output(policy_output, agent.state)
        self._record_trace(
            agent.run_id,
            "PolicyCompleted",
            {"tick_id": tick_id, "policy": policy_name},
        )
        self._record_trace(
            agent.run_id,
            "CandidatesProposed",
            {"tick_id": tick_id, "count": len(actions)},
        )

        constraint_eval = self._evaluate_constraints(agent, percepts, actions)
        self._record_trace(
            agent.run_id,
            "ConstraintsEvaluated",
            {
                "tick_id": tick_id,
                "constraints": [
                    constraint.__dict__ for constraint in constraint_eval[0]
                ],
                "result": constraint_eval[1].__dict__,
            },
        )

        outcomes: list[ActionOutcome] = []
        needs_intervention = False

        for candidate in actions:
            action_id = _new_uuid()
            self._record_trace(
                agent.run_id,
                "ActionVerificationStarted",
                {"tick_id": tick_id, "action_id": action_id, "action": candidate.action.name},
            )

            if not constraint_eval[1].allowed:
                outcome = ActionOutcome(
                    status="denied",
                    verification=constraint_eval[1],
                    post_verification=None,
                    output=None,
                    error=None,
                    action_id=action_id,
                )
                outcomes.append(outcome)
                self._record_trace(
                    agent.run_id,
                    "ActionVerificationCompleted",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": None,
                    },
                )
                self._record_trace(
                    agent.run_id,
                    "ActionDenied",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": outcome.verification.__dict__,
                    },
                )
                continue

            permission = self._verify_policy(tenant.policy, candidate)
            if not permission.allowed:
                outcome = ActionOutcome(
                    status="denied",
                    verification=permission,
                    post_verification=None,
                    output=None,
                    error="permission denied",
                    action_id=action_id,
                )
                outcomes.append(outcome)
                self._record_trace(
                    agent.run_id,
                    "ActionVerificationCompleted",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": None,
                    },
                )
                self._record_trace(
                    agent.run_id,
                    "ActionDenied",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": outcome.verification.__dict__,
                    },
                )
                continue

            quota = tenant.ledger.record_usage(
                agent.agent_id,
                candidate.usage,
                tenant.quotas,
                time.time(),
            )
            if not quota.allowed:
                outcome = ActionOutcome(
                    status="denied",
                    verification=quota,
                    post_verification=None,
                    output=None,
                    error="quota denied",
                    action_id=action_id,
                )
                outcomes.append(outcome)
                self._record_trace(
                    agent.run_id,
                    "ActionVerificationCompleted",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": None,
                    },
                )
                self._record_trace(
                    agent.run_id,
                    "ActionDenied",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": outcome.verification.__dict__,
                    },
                )
                continue

            precheck = self._verify_preconditions(candidate)
            if not precheck.allowed:
                outcome = ActionOutcome(
                    status="denied",
                    verification=precheck,
                    post_verification=None,
                    output=None,
                    error="precondition missing",
                    action_id=action_id,
                )
                outcomes.append(outcome)
                self._record_trace(
                    agent.run_id,
                    "ActionVerificationCompleted",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": None,
                    },
                )
                self._record_trace(
                    agent.run_id,
                    "ActionDenied",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": outcome.verification.__dict__,
                    },
                )
                continue

            output, satisfied_postconditions, error = self._execute_action(candidate)
            post = self._verify_postconditions(
                candidate.action, satisfied_postconditions
            )
            if post is not None and not post.allowed:
                needs_intervention = True

            outcome = ActionOutcome(
                status="executed" if error is None else "failed",
                verification=VerificationResult.allow(),
                post_verification=post,
                output=output,
                error=error,
                action_id=action_id,
            )
            outcomes.append(outcome)
            self._record_trace(
                agent.run_id,
                "ActionVerificationCompleted",
                {
                    "tick_id": tick_id,
                    "action_id": action_id,
                    "action": candidate.action.name,
                    "result": post.__dict__ if post is not None else None,
                },
            )
            if outcome.output is not None:
                self._record_trace(
                    agent.run_id,
                    "ActionExecuted",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "output": outcome.output,
                    },
                )
            if outcome.status == "failed" or (post is not None and not post.allowed):
                self._record_trace(
                    agent.run_id,
                    "ActionFailed",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "error": outcome.error or "action_failed",
                        "result": (
                            post or VerificationResult.deny("action_failed")
                        ).__dict__,
                    },
                )
            elif outcome.status != "executed":
                self._record_trace(
                    agent.run_id,
                    "ActionDenied",
                    {
                        "tick_id": tick_id,
                        "action_id": action_id,
                        "action": candidate.action.name,
                        "result": (
                            post or VerificationResult.deny("action_failed")
                        ).__dict__,
                    },
                )

        duration_ms = int((time.time() - start) * 1000)
        self._record_trace(
            agent.run_id,
            "OutcomeRecorded",
            {
                "tick_id": tick_id,
                "duration_ms": duration_ms,
                "needs_intervention": needs_intervention,
                "actions": [self._serialize_outcome(outcome) for outcome in outcomes],
            },
        )

        agent.state = next_state
        snapshot_id = None
        if agent.snapshot_interval and tick_id % agent.snapshot_interval == 0:
            snapshot_id = str(uuid.uuid4())
            self._snapshots[snapshot_id] = next_state
        state_hash = hashlib.sha256(next_state).hexdigest()
        state_node_id = f"sha256:{state_hash}"
        self._record_trace(
            agent.run_id,
            "StateCommitted",
            {
                "tick_id": tick_id,
                "state_hash": state_hash,
                "state_node_id": state_node_id,
                "snapshot_id": snapshot_id,
            },
        )
        self._record_trace(agent.run_id, "LoopTickCompleted", {"tick_id": tick_id})

        return TickOutcome(
            tick_id=tick_id,
            action_outcomes=outcomes,
            duration_ms=duration_ms,
            needs_intervention=needs_intervention,
            state=next_state,
        )

    def record_trace(self, kind: str, message: str) -> dict[str, Any]:
        event = {
            "sequence": self._sequence_by_run.get(self._config.name, 0),
            "kind": kind,
            "message": message,
            "runtime": self._config.name,
        }
        self._sequence_by_run[self._config.name] = event["sequence"] + 1
        self._trace_sink(event)
        return event

    def subscribe_traces(
        self, run_id: str, callback: Callable[[dict[str, Any]], None]
    ) -> None:
        self._trace_subscribers.setdefault(run_id, []).append(callback)

    def tail_traces(self, run_id: str) -> Iterable[dict[str, Any]]:
        return iter(self._trace_by_run.get(run_id, []))

    def replay_run(self, run_id: str) -> list[dict[str, Any]]:
        """Return a read-only replay view of stored trace events for a run.

        The Python SDK replay path reconstructs behavior from in-memory trace
        events only. It does not invoke policies, constraints, or adapters, so
        side effects recorded in the trace are never repeated during replay.
        """
        events = self._trace_by_run.get(run_id)
        if not events:
            raise ValueError("run not found")
        run_id = _validate_uuid_id(run_id, "run_id")
        for expected, event in enumerate(events):
            if event.get("sequence") != expected:
                raise ValueError(
                    f"trace sequence gap or corruption: expected {expected} "
                    f"but found {event.get('sequence')}"
                )
            if event.get("run_id") != run_id:
                raise ValueError("trace run mismatch")
            if event.get("trace_event_id") != _trace_event_id(run_id, expected):
                raise ValueError("trace event identity mismatch")
            identity = event.get("identity") or {}
            if identity.get("run_id") != run_id:
                raise ValueError("trace identity run mismatch")
        return copy.deepcopy(events)

    def _agent_or_raise(self, agent_id: str) -> AgentContext:
        agent = self._agents.get(agent_id)
        if agent is None:
            raise ValueError("agent not found")
        return agent

    def _record_trace(
        self,
        run_id: str,
        kind: str,
        payload: dict[str, Any],
    ) -> dict[str, Any]:
        run_id = _validate_uuid_id(run_id, "run_id")
        sequence = self._sequence_by_run.get(run_id, 0)
        event = {
            "sequence": sequence,
            "trace_event_id": _trace_event_id(run_id, sequence),
            "run_id": run_id,
            "identity": self._trace_identity(run_id, payload),
            "kind": kind,
            "payload": payload,
        }
        self._sequence_by_run[run_id] = sequence + 1
        self._trace_by_run.setdefault(run_id, []).append(event)
        self._trace_sink(event)
        for callback in self._trace_subscribers.get(run_id, []):
            callback(event)
        return event

    def _trace_identity(self, run_id: str, payload: dict[str, Any]) -> dict[str, Any]:
        identity: dict[str, Any] = {"run_id": run_id}
        for agent in self._agents.values():
            if agent.run_id == run_id:
                identity["tenant_id"] = _validate_uuid_id(agent.tenant_id, "tenant_id")
                identity["agent_id"] = _validate_uuid_id(agent.agent_id, "agent_id")
                break
        for field in ("tick_id", "action_id", "state_node_id", "message_id"):
            value = payload.get(field)
            if value is not None:
                identity[field] = value
        return identity

    def _collect_percepts(self, agent: AgentContext) -> list[dict[str, Any]]:
        percepts: list[dict[str, Any]] = []
        for perceptor in agent.perceptors:
            for value in perceptor(agent):
                percepts.append(self._normalize_percept(value))
        return percepts

    def _normalize_percept(self, value: Any) -> dict[str, Any]:
        if isinstance(value, Percept):
            return {
                "schema": value.schema,
                "payload": value.payload,
                "provenance": value.provenance,
                "timestamp": value.timestamp,
            }
        if isinstance(value, dict):
            return {
                "schema": value.get("schema", "") or "",
                "payload": value.get("payload", {}) or {},
                "provenance": value.get("provenance", {}) or {},
                "timestamp": value.get("timestamp", time.time()),
            }
        return {
            "schema": "unknown",
            "payload": {"value": value},
            "provenance": {},
            "timestamp": time.time(),
        }

    def _normalize_policy_output(
        self, output: Any, state: bytes
    ) -> tuple[list[ActionCandidate], bytes]:
        next_state = state
        raw_actions: Iterable[Any]
        if isinstance(output, tuple) and len(output) == 2:
            raw_actions, next_state = output
        elif isinstance(output, dict):
            raw_actions = output.get("actions", [])
            if "state" in output:
                next_state = output["state"]
        else:
            raw_actions = output or []

        candidates = [self._normalize_action(action) for action in raw_actions]
        return candidates, next_state

    def _normalize_action(self, value: Any) -> ActionCandidate:
        if isinstance(value, ActionCandidate):
            return value
        if isinstance(value, Action):
            action = value
            return ActionCandidate(
                action=action, satisfied_preconditions=list(action.preconditions)
            )
        if isinstance(value, dict):
            action = Action(
                name=value.get("name", ""),
                params=value.get("params", {}) or {},
                side_effect_class=value.get("side_effect_class", "unknown"),
                cost_estimate=value.get("cost_estimate"),
                required_permissions=value.get("required_permissions", []) or [],
                preconditions=value.get("preconditions", []) or [],
                postconditions=value.get("postconditions", []) or [],
            )
            usage = self._normalize_usage(value.get("usage"))
            adapter = value.get("adapter")
            satisfied = value.get("satisfied_preconditions", action.preconditions)
            return ActionCandidate(
                action=action,
                adapter=adapter,
                usage=usage,
                satisfied_preconditions=list(satisfied),
            )
        raise ValueError("invalid action")

    def _normalize_usage(self, value: Any) -> QuotaUsage:
        if isinstance(value, QuotaUsage):
            return value
        if isinstance(value, dict):
            return QuotaUsage(
                actions=int(value.get("actions", 1)),
                action_duration_ms=int(value.get("action_duration_ms", 0)),
                filesystem_read_bytes=int(value.get("filesystem_read_bytes", 0)),
                filesystem_write_bytes=int(value.get("filesystem_write_bytes", 0)),
                network_read_bytes=int(value.get("network_read_bytes", 0)),
                network_write_bytes=int(value.get("network_write_bytes", 0)),
                http_requests=int(value.get("http_requests", 0)),
            )
        return QuotaUsage.single_action()

    def _evaluate_constraints(
        self,
        agent: AgentContext,
        percepts: list[dict[str, Any]],
        actions: list[ActionCandidate],
    ) -> tuple[list[Constraint], VerificationResult]:
        constraints: list[Constraint] = []
        result = VerificationResult.allow()
        if agent.constraints is None:
            return constraints, result
        response = agent.constraints(agent.state, percepts, actions)
        if isinstance(response, VerificationResult):
            return constraints, response
        if isinstance(response, dict):
            allowed = bool(response.get("allowed", True))
            reasons = list(response.get("reasons", []))
            artifacts = dict(response.get("artifacts", {}))
            raw_constraints = response.get("constraints", [])
            constraints = [self._normalize_constraint(item) for item in raw_constraints]
            return constraints, VerificationResult(allowed, reasons, artifacts)
        if isinstance(response, bool):
            return (
                constraints,
                VerificationResult.allow()
                if response
                else VerificationResult.deny("constraints_denied"),
            )
        return constraints, result

    def _normalize_constraint(self, value: Any) -> Constraint:
        if isinstance(value, Constraint):
            return value
        if isinstance(value, dict):
            return Constraint(
                id=value.get("id", "constraint"),
                kind=value.get("kind", "hard"),
                scope=value.get("scope", "action"),
                predicate=value.get("predicate", ""),
                obligation=value.get("obligation"),
            )
        return Constraint(
            id="constraint",
            kind="hard",
            scope="action",
            predicate=str(value),
            obligation=None,
        )

    def _verify_policy(
        self, policy: TenantPolicy, candidate: ActionCandidate
    ) -> VerificationResult:
        if candidate.action.name not in policy.allowed_actions:
            return VerificationResult.deny("action_not_allowed")
        adapter = candidate.adapter or candidate.action.name
        if adapter not in policy.allowed_adapters:
            return VerificationResult.deny("adapter_not_allowed")
        missing = [
            perm
            for perm in candidate.action.required_permissions
            if perm not in policy.allowed_permissions
        ]
        if missing:
            return VerificationResult(
                False, ["permission_missing"], {"missing": missing}
            )
        return VerificationResult.allow()

    def _verify_preconditions(self, candidate: ActionCandidate) -> VerificationResult:
        missing = [
            condition
            for condition in candidate.action.preconditions
            if condition not in candidate.satisfied_preconditions
        ]
        if missing:
            return VerificationResult(
                False, ["precondition_missing"], {"missing": missing}
            )
        return VerificationResult.allow()

    def _verify_postconditions(
        self, action: Action, satisfied: Iterable[str]
    ) -> Optional[VerificationResult]:
        expected = action.postconditions
        if not expected:
            return VerificationResult.allow()
        missing = [condition for condition in expected if condition not in satisfied]
        if missing:
            return VerificationResult(
                False, ["postcondition_missing"], {"missing": missing}
            )
        return VerificationResult.allow()

    def _execute_action(
        self, candidate: ActionCandidate
    ) -> tuple[Optional[dict[str, Any]], list[str], Optional[str]]:
        adapter_id = candidate.adapter or candidate.action.name
        adapter = self._adapters.get(adapter_id)
        if adapter is None:
            return None, [], "adapter not registered"
        try:
            result = adapter(candidate.action)
            output = result.get("output", result)
            satisfied = result.get("satisfied_postconditions", [])
            return output, list(satisfied), None
        except Exception as exc:  # pragma: no cover - defensive
            return None, [], str(exc)

    def _serialize_outcome(self, outcome: ActionOutcome) -> dict[str, Any]:
        return {
            "status": outcome.status,
            "verification": outcome.verification.__dict__,
            "post_verification": outcome.post_verification.__dict__
            if outcome.post_verification
            else None,
            "output": outcome.output,
            "error": outcome.error,
            "action_id": outcome.action_id,
        }

    def _default_trace_sink(self, event: dict[str, Any]) -> None:
        print(event)
