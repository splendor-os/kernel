# Splendor Kernel Milestones, FRs, and Isolated Implementation Sprints

**Date:** 2026-05-25  
**Target audience:** Splendor implementation agents, architecture agents, runtime agents, SDK agents, orchestration agents, integration agents, and review agents.  
**Primary intent:** Convert the Splendor kernel direction into clear functional requirements, isolated milestones, and executable sprint boundaries.

---

## 0. Source-of-Truth Position

Splendor must be implemented as an **AI autonomy runtime kernel / management layer**, not as a chat-agent framework, not as an enterprise SaaS control plane, and not as a robot's hard real-time controller.

The runtime model is:

```text
Percepts -> Policy -> Constraints -> Gateway -> Adapter -> Outcome -> State Commit -> Trace
```

Everything in this roadmap exists to strengthen this loop.

Splendor owns runtime primitives:

```text
percepts
agent identity
runtime contexts
state graph
trace store
ticks
policies
constraints
action gateway
verifiers
quotas
outcomes
feedback
rewards
messages
replay
runtime migration
fleet identity
local and remote orchestration boundaries
```

External systems such as Harmony, fleet consoles, enterprise control planes, or product UIs may schedule, configure, inspect, and govern Splendor runs. They must not replace Splendor's runtime enforcement boundary.

---

## 1. Non-Drift Rules for Implementation Agents

Implementation agents must follow these rules before adding or modifying any feature.

### 1.1 Do not build chat-first

Do not center Splendor around conversations, prompts, or assistants. A chat message may be a percept. It is not the runtime model.

### 1.2 Do not build an enterprise SaaS product inside the kernel

Splendor may expose APIs for organizations, work orders, approval routing, trace export, and policy distribution. It should not own billing, admin-console UX, low-code app building, enterprise workspace UI, or marketplace behavior.

### 1.3 Do not bypass the action gateway

No tool call, connector, filesystem write, network call, robot command, database mutation, email, ticket creation, artifact publish, or webhook should occur outside the gateway once a run is delegated to Splendor.

### 1.4 Do not treat traces as logs

Traces are runtime contract data. Every meaningful state transition, verifier decision, message, action result, denial, approval, migration, and failure must be traceable.

### 1.5 Do not hide state

Agent state must be explicit, versioned, scoped, replayable, and inspectable. Avoid hidden mutable shared state.

### 1.6 Do not map agents directly to containers

Use this rule:

```text
1 isolation domain = 1 Splendor instance
1 agent run = 1 runtime context / slot
1 sensitive or physical boundary = likely 1 dedicated instance
```

Do not assume:

```text
1 agent = 1 Docker image = 1 Kubernetes pod
```

### 1.7 Do not make distributed execution magical

Distributed Splendor requires explicit node identity, instance identity, agent identity, run identity, state ownership, message semantics, trace continuity, work-order signing, failure recovery, offline behavior, and replay semantics.

### 1.8 Do not make Splendor a low-level robot controller

Splendor may supervise autonomy and mediate high-level physical actions. It must not replace motor controllers, flight controllers, PLC safety systems, servo loops, ROS/device drivers, or firmware safety loops.

### 1.9 Fail closed

If policy, verifier, identity, approval, trace, state commit, or capability validation cannot complete, deny, pause, or request intervention. Do not silently allow side effects.

---

## 2. Required Implementation-Agent Workflow

Every implementation agent must follow this workflow.

### Step 1: Bind work to a milestone and FR IDs

Every task must state:

```text
Milestone: Splendor0.xx-dev
Sprint: 0.xx-Sn
Functional requirements touched: FR-...
Primitives strengthened: ...
```

If a task cannot be tied to at least one FR, do not implement it yet.

### Step 2: Declare non-goals before coding

At the start of the task or PR, explicitly list what is not being built. This prevents scope creep.

Example:

```text
Non-goals:
- no remote transport
- no governance workflow engine
- no production adapter certification
- no UI surface
```

### Step 3: Protect the kernel loop

Every implementation must preserve:

```text
percept intake
policy invocation
constraint / verifier evaluation
gateway mediation
adapter execution only through gateway
outcome recording
state commit
trace emission
replay semantics
```

### Step 4: Add tests before widening scope

Each sprint must include tests for:

```text
positive path
denial path
failure path
trace emission
state consistency
replay behavior
fail-closed behavior
permission / quota behavior where relevant
```

### Step 5: Update docs and examples in the same sprint

A sprint is not complete unless docs and examples match the implementation.

### Step 6: No cross-milestone leakage

Do not implement future milestone behavior opportunistically unless the current milestone explicitly requires a placeholder or schema hook.

Allowed:

```text
define a stable field for future remote identity
```

Not allowed:

```text
build remote fleet scheduling inside the local multi-agent milestone
```

---

## 3. Global Definition of Done

A feature is done only when all applicable checks pass.

```text
[ ] FR IDs are listed.
[ ] Non-goals are listed.
[ ] No side effect bypasses the gateway.
[ ] Trace events are emitted for all meaningful transitions.
[ ] State is committed or explicitly left unchanged with a reason.
[ ] Replay behavior is defined and tested.
[ ] Verifier failures fail closed.
[ ] Permissions and quotas are enforced at the correct identity scope.
[ ] Docs and examples are updated.
[ ] CLI / SDK / API behavior is consistent where applicable.
[ ] Compatibility impact is documented.
[ ] Migration impact is documented if schemas changed.
```

---

## 4. Clean Roadmap

```text
Splendor0.01-dev:
  Completed local kernel baseline:
  local runtime + scheduler + gateway + verifier chain + state graph + trace store + replay + Python SDK + CLI + filesystem/HTTP adapters.

Splendor0.02-dev:
  Local multi-agent runtime + daemon control:
  daemon security boundary + typed local messages + per-agent isolation + scoped local delegation + runtime daemon API + @splendor/types + @splendor/client.

Splendor0.03-dev:
  Resident nodes + fleet execution foundation:
  node/instance registry + signed work orders + capability advertisement + remote messaging + state handoff + trace aggregation + fleet telemetry.

Splendor0.04-dev:
  Governance workflows:
  approval gates + escalation policies + circuit breakers + policy TTL + kill switch + governance trace/replay + external control-plane adapter.

Splendor0.05-dev:
  Physical/edge orchestration:
  device node profiles + offline policy cache + local trace buffer + safety verifier API + robotics high-level adapter contract + operator intervention.

Splendor0.1-dev:
  Stable primitive spec:
  compatibility guarantees + conformance suite + adapter maturity model + SDK/API stability + migration/release policy.
```

---

# 5. Splendor0.01-dev — Local Kernel Baseline

## 5.1 Status

**Implemented baseline.** Treat this milestone as the current foundation and hardening target, not as a future feature milestone.

## 5.2 Goal

Prove the local Splendor kernel loop end to end.

## 5.3 Functional Requirements

| ID         | Requirement                                                                                                               |
| ---------- | ------------------------------------------------------------------------------------------------------------------------- |
| FR-0.01-01 | Run a persistent local agent loop with tenant, agent, run, tick, and action identity.                                     |
| FR-0.01-02 | Collect percepts, invoke policy, evaluate constraints, submit actions through the gateway, commit state, and emit traces. |
| FR-0.01-03 | Enforce tenant action, adapter, permission, and quota checks before side effects.                                         |
| FR-0.01-04 | Persist state graph snapshots and append-only trace events.                                                               |
| FR-0.01-05 | Replay a local run without blindly re-executing side effects.                                                             |
| FR-0.01-06 | Expose Python SDK hooks for policy, perceptor, constraints, adapters, and trace subscription.                             |
| FR-0.01-07 | Provide CLI workflows for run execution, trace export, and replay.                                                        |

## 5.4 Isolated Hardening Sprints

| Sprint  | Scope                  | Deliverables                                                                      | Exit gate                                                                   |
| ------- | ---------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| 0.01-H1 | Baseline conformance   | README/docs/examples aligned with implemented behavior                            | A new user can run the documented local example without undocumented setup. |
| 0.01-H2 | Trace/replay hardening | Trace ordering tests, integrity-chain tests, replay side-effect suppression tests | Replay reconstructs a run and does not execute unsafe side effects.         |
| 0.01-H3 | SDK ergonomics         | Python SDK examples for policy, perceptors, verifiers, adapters, traces, replay   | SDK examples pass in CI or documented smoke tests.                          |
| 0.01-H4 | Release hygiene        | Version tag, changelog, compatibility notes, minimum test matrix                  | 0.01-dev is clearly marked as baseline and ready for 0.02 work.             |

## 5.5 Non-Goals

```text
no fleet orchestration
no remote messaging
no distributed execution
no governance workflow engine
no physical orchestration
no broad adapter ecosystem
```

---

# 6. Splendor0.02-dev — Local Multi-Agent Runtime + Daemon Control

## 6.1 Goal

Multiple local agent runtime contexts can coordinate safely inside one Splendor instance, with typed messages, trace-linked causality, scoped delegation, per-agent isolation, and a daemon API for external control.

## 6.2 Functional Requirements

| ID         | Requirement                                                                                                                                                |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FR-0.02-01 | Define a canonical `Message` type with `message_id`, source agent, target agent, run, schema, payload, causal parent, response requirement, and timestamp. |
| FR-0.02-02 | Add local message inbox/outbox per agent runtime context.                                                                                                  |
| FR-0.02-03 | Route typed messages between agents in the same Splendor instance.                                                                                         |
| FR-0.02-04 | Link every message send and receive to trace events.                                                                                                       |
| FR-0.02-05 | Support parent/child run references for local delegation.                                                                                                  |
| FR-0.02-06 | Enforce per-agent permissions and quotas independently inside the same tenant.                                                                             |
| FR-0.02-07 | Prevent permission laundering by shared or local sub-agents.                                                                                               |
| FR-0.02-08 | Expose local runtime daemon API for runs, percepts, traces, state-head, replay, health, and capabilities.                                                  |
| FR-0.02-09 | Publish `@splendor/types` for canonical schemas and `@splendor/client` for daemon calls.                                                                   |
| FR-0.02-10 | Add local multi-agent replay that reconstructs message causality.                                                                                          |

## 6.3 Required Message Shape

```json
{
  "message_id": "msg_123",
  "source_agent_id": "finance.orchestrator",
  "target_agent_id": "finance.forecast",
  "run_id": "run_456",
  "schema": "splendor.message.task_request.v1",
  "payload": {
    "task": "forecast revenue for Q3",
    "input_ref": "dataset:finance.revenue_monthly_v4"
  },
  "causal_parent": "trace_evt_789",
  "requires_response": true,
  "created_at": "2026-05-25T00:00:00Z"
}
```

## 6.4 Isolated Sprints

| Sprint  | Scope                   | Deliverables                                                                                      | Exit gate                                                                 |
| ------- | ----------------------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| 0.02-S0 | Daemon Security Boundary | App/client identity, local secure defaults, endpoint scopes, signed work-order requirement, audit attribution | Daemon/client surfaces cannot stabilize with insecure defaults.           |
| 0.02-S1 | Message schema contract | `Message`, envelope, delivery status, schema version, causal parent, trace IDs                    | Messages are typed, versioned, and serializable across Rust/Python/TS.    |
| 0.02-S2 | Local message router    | In-memory router, inbox/outbox, delivery states, failure states, trace hooks                      | Two local agents exchange trace-linked typed messages.                    |
| 0.02-S3 | Agent isolation ledger  | Per-agent quota ledger, permissions, allowed message schemas, allowed recipients                  | One local agent cannot inherit another agent's broad permissions.         |
| 0.02-S4 | Local delegation model  | Parent/child run model, task request/response messages, scoped delegated permissions              | Orchestrator delegates to specialist with narrower authority.             |
| 0.02-S5 | Runtime daemon API      | `/runs`, `/percepts`, `/state-head`, `/traces`, `/replay`, `/actions`, `/health`, `/capabilities` | External client can create, start, inspect, replay, and stop a local run. |
| 0.02-S6 | TypeScript surface      | `@splendor/types`, `@splendor/client`, generated schemas, compatibility tests                     | TypeScript client can control daemon without native binding.              |
| 0.02-S7 | Replay and test harness | Multi-agent replay, message causal graph inspection, denial tests                                 | Replay reconstructs local multi-agent causality.                          |

### 0.02-S0 — Daemon Security Boundary

#### Goal

Define the security boundary for all communication between external apps and Splendor daemons before daemon APIs and SDK clients become stable.

External apps include:

- SDK clients
- CLIs
- control planes
- operator consoles
- adapters
- sidecars
- test clients
- central managers
- Harmony or other product integrations

#### Functional Requirements

| ID | Requirement |
|---|---|
| FR-0.02-S0-01 | Define `AppPrincipal` / `ClientPrincipal` as caller identities distinct from tenant, agent, run, node, and instance identities. |
| FR-0.02-S0-02 | Require authentication for every non-dev daemon API request. |
| FR-0.02-S0-03 | Define endpoint-level scopes for daemon APIs. |
| FR-0.02-S0-04 | Bind caller credentials to tenant or fleet context. |
| FR-0.02-S0-05 | Bind caller credentials to intended audience: daemon, instance, fleet, or central manager. |
| FR-0.02-S0-06 | Require expiry on caller credentials. |
| FR-0.02-S0-07 | Define a revocation path for caller credentials and work-order credentials. |
| FR-0.02-S0-08 | Require signed, scoped work orders for run creation. |
| FR-0.02-S0-09 | Require caller attribution in trace/audit metadata for mutating operations. |
| FR-0.02-S0-10 | Define local dev insecure mode as explicit, local-only, and visibly warned. |
| FR-0.02-S0-11 | Require SDKs and clients to refuse silent fallback to insecure unauthenticated communication. |

#### Required Scope

Define:

- secure local transport defaults;
- secure production transport expectations;
- daemon API auth layers;
- app/caller identity;
- endpoint scopes;
- signed work-order interaction;
- expired or revoked work-order rejection for run creation and resume;
- revocation source expectations, such as a revocation list, introspection endpoint, or signing-key invalidation;
- trace/audit attribution;
- dev-only insecure mode constraints.

#### Non-goals

Do not implement:

- full OAuth server;
- full PKI management;
- fleet mTLS rollout;
- node bootstrap protocol;
- remote fleet auth;
- governance approval workflows;
- runtime permission engine beyond documented criteria.

Those belong to later 0.03 and 0.04 work.

#### Exit Gate

The docs must clearly state that:

```text
transport security authenticates the channel;
caller identity authenticates the app;
endpoint scopes authorize API access;
signed work orders authorize runs;
tenant/agent/run policy scopes runtime authority;
the Action Gateway authorizes side effects.
```

## 6.5 Exit Gate

A single Splendor instance runs an orchestrator agent plus at least two specialist agents. Messages are typed and trace-linked. Specialist permissions are scoped. Replay reconstructs the complete causal graph. No side effects bypass the gateway.

## 6.6 Non-Goals

```text
no multi-host transport
no fleet registry
no distributed state migration
no governance workflow engine
no physical device support
```

---

# 7. Splendor0.03-dev — Resident Nodes + Fleet Execution Foundation

## 7.1 Goal

Splendor instances can register as fleet nodes, advertise capabilities, receive signed work orders, coordinate across hosts, and preserve identity, state, and trace continuity.

## 7.2 Functional Requirements

| ID         | Requirement                                                                                                                                                         |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FR-0.03-01 | Define stable `fleet_id`, `node_id`, `instance_id`, `tenant_id`, `agent_id`, `run_id`, `tick_id`, `action_id`, `state_node_id`, `trace_event_id`, and `message_id`. |
| FR-0.03-02 | Implement node registration and instance registration.                                                                                                              |
| FR-0.03-03 | Implement capability advertisement for cloud, VPC, on-prem, edge, desktop, and physical nodes.                                                                      |
| FR-0.03-04 | Implement signed work-order ingestion and rejection of unsigned, expired, revoked, or incompatible work orders.                                                     |
| FR-0.03-05 | Implement remote work-order dispatch from a minimal central manager.                                                                                                |
| FR-0.03-06 | Add fleet health heartbeat and runtime capability reporting.                                                                                                        |
| FR-0.03-07 | Add local trace buffering and central trace aggregation.                                                                                                            |
| FR-0.03-08 | Support remote typed messaging between Splendor instances.                                                                                                          |
| FR-0.03-09 | Support explicit state handoff via snapshot reference, not hidden shared mutable state.                                                                             |
| FR-0.03-10 | Record run interruption, resume, migration, and cross-instance message events in trace.                                                                             |
| FR-0.03-11 | Add fleet-level telemetry aggregation for traces, health, quotas, and failures.                                                                                     |

## 7.3 Required Work Order Shape

```json
{
  "work_order_id": "wo_123",
  "tenant_id": "tenant_acme",
  "agent_id": "finance.board_reporting",
  "objective": "Generate weekly revenue dashboard",
  "allowed_actions": ["sql.query", "artifact.create", "approval.request"],
  "allowed_adapters": ["harmony", "sql", "artifact-store"],
  "allowed_permissions": ["finance.read", "artifact.create"],
  "data_refs": ["dataset:finance.revenue_monthly_v4"],
  "quotas": {
    "max_actions_per_tick": 5,
    "max_action_duration_ms": 30000,
    "max_http_requests_per_minute": 60
  },
  "placement": {
    "target": "resident_cloud_pool",
    "data_locality": "eu-west",
    "requires_gpu": false
  },
  "issued_at": "2026-05-25T00:00:00Z",
  "expires_at": "2026-05-25T01:00:00Z",
  "signature": "..."
}
```

## 7.4 Isolated Sprints

| Sprint  | Scope                    | Deliverables                                                                      | Exit gate                                                             |
| ------- | ------------------------ | --------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| 0.03-S1 | Identity model           | Canonical distributed IDs, validation, serialization, tests                       | IDs are not overloaded across fleet/node/instance/agent/run concepts. |
| 0.03-S2 | Node/instance registry   | Register, heartbeat, capabilities, status, version, trust level                   | Resident instance appears in registry with health and capabilities.   |
| 0.03-S3 | Signed work orders       | Schema, signature validation, expiry/revocation checks, rejection paths           | Unsigned or expired work orders are rejected before run start.        |
| 0.03-S4 | Placement v0             | Capability matching, locality hints, dedicated-instance flag, rejection reasons   | Central manager chooses a valid target or explains rejection.         |
| 0.03-S5 | Remote message transport | Trace-linked remote messages, retry rules, delivery failures, idempotency markers | Two instances exchange trace-linked messages safely.                  |
| 0.03-S6 | Trace aggregation        | Local buffer, sync protocol, central trace index, hash-chain validation           | Remote traces aggregate without losing ordering or integrity.         |
| 0.03-S7 | State handoff v0         | Snapshot export/import, state-head validation, read-only reference mode           | A run can resume from validated state on another instance.            |
| 0.03-S8 | Fleet telemetry          | Health, quotas, run status, trace sync status, failure aggregation                | Fleet view reports health and run status across instances.            |

## 7.5 Exit Gate

A central manager dispatches a signed work order to a resident node. The node executes a run, sends typed remote messages, buffers and syncs traces, reports health, and preserves run/state/trace identity across interruption and resume.

## 7.6 Non-Goals

```text
no full distributed consensus engine
no arbitrary shared distributed memory
no mature governance workflow engine
no physical robotics safety certification
no marketplace
```

---

# 8. Splendor0.04-dev — Governance Workflows

## 8.1 Goal

Splendor can pause, deny, escalate, circuit-break, kill, or resume actions and runs through explicit governance workflows.

## 8.2 Functional Requirements

| ID         | Requirement                                                                                                                     |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------- |
| FR-0.04-01 | Add `needs_approval` and `needs_intervention` as first-class action and run outcomes where missing.                             |
| FR-0.04-02 | Define approval policy objects with action class, permission, tenant, adapter, risk level, and expiry.                          |
| FR-0.04-03 | Add approval request, approval grant, denial, expiry, and revocation events to trace.                                           |
| FR-0.04-04 | Pause runs while waiting for approval and resume only with a valid approval token or percept.                                   |
| FR-0.04-05 | Implement escalation policies for timeout, verifier uncertainty, repeated failure, quota pressure, and safety risk.             |
| FR-0.04-06 | Implement circuit breakers for tenant, agent, adapter, action class, node, instance, fleet, and global scopes.                  |
| FR-0.04-07 | Add kill-switch propagation from central manager to resident instances.                                                         |
| FR-0.04-08 | Add policy TTL checks and fail-closed behavior on missing or expired policy.                                                    |
| FR-0.04-09 | Add audit export for approvals, denials, circuit-breaker trips, run pauses, and resumes.                                        |
| FR-0.04-10 | Add governance adapter surface for Harmony or another external approval/control plane without making Splendor Harmony-specific. |

## 8.3 Required Governance Outcomes

```text
action.executed
action.denied
action.failed
action.needs_approval
action.needs_intervention
run.paused
run.resumed
run.cancelled
run.denied
run.expired
circuit_breaker.tripped
circuit_breaker.cleared
kill_switch.activated
policy.expired
policy.revoked
approval.requested
approval.granted
approval.denied
approval.expired
approval.revoked
```

## 8.4 Isolated Sprints

| Sprint  | Scope                       | Deliverables                                                                               | Exit gate                                                               |
| ------- | --------------------------- | ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| 0.04-S1 | Governance state model      | Approval, escalation, intervention, circuit-breaker, kill-switch schemas                   | Governance state is explicit and traceable.                             |
| 0.04-S2 | Approval verifier           | Approval-required detection, pause/resume flow, approval-token validation                  | A high-risk action pauses until valid approval arrives.                 |
| 0.04-S3 | Escalation engine           | Timeout, repeated denial/failure, verifier uncertainty, policy expiry, operator escalation | Escalations are deterministic and trace-linked.                         |
| 0.04-S4 | Circuit breakers            | Agent, tenant, adapter, action, node, fleet scopes with trace events                       | Circuit breaker prevents matching actions and records reason.           |
| 0.04-S5 | Central policy distribution | Policy bundle versioning, TTL, cache, revocation, fail-closed handling                     | Expired or revoked policy denies high-risk side effects.                |
| 0.04-S6 | Control-plane adapter       | Approval endpoint contract, scoped work-order bridge, trace-linked approval flow           | External control plane can grant/deny without owning runtime internals. |
| 0.04-S7 | Governance replay/audit     | Explain why action was allowed, denied, paused, escalated, or resumed                      | Replay produces governance explanation without side effects.            |

## 8.5 Exit Gate

A side-effectful action can be paused for approval, externally approved or denied, resumed or cancelled, fully traced, replay-explained, and circuit-broken at multiple scopes.

## 8.6 Non-Goals

```text
no full enterprise admin SaaS
no billing or org UX
no universal approval UI
no marketplace
no product-specific Harmony dependency inside the kernel
```

---

# 9. Splendor0.05-dev — Physical and Edge Orchestration

## 9.1 Goal

Support robots, drones, humanoids, edge devices, desktop sidecars, and disconnected nodes as governed Splendor runtime targets without becoming a hard real-time controller.

## 9.2 Functional Requirements

| ID         | Requirement                                                                                                                                 |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| FR-0.05-01 | Define device node profiles for robot, drone, humanoid, edge appliance, desktop sidecar, and industrial device.                             |
| FR-0.05-02 | Define physical capability model: sensors, bounded motion actions, local compute, network, battery/power, and safety status.                |
| FR-0.05-03 | Implement local policy cache with TTL and offline behavior.                                                                                 |
| FR-0.05-04 | Implement local trace buffer with reconnect sync.                                                                                           |
| FR-0.05-05 | Define robotics adapter interface for high-level bounded actions only.                                                                      |
| FR-0.05-06 | Forbid raw actuator writes, firmware safety bypass, and low-level motor control as Splendor direct actions.                                 |
| FR-0.05-07 | Add safety verifier interface for geofence, altitude, battery, collision, human proximity, force limits, emergency stop, and privacy zones. |
| FR-0.05-08 | Add operator intervention protocol for local/manual approval.                                                                               |
| FR-0.05-09 | Support cloud helper work orders that propose plans without direct actuator authority.                                                      |
| FR-0.05-10 | Trace local safety verification, operator override, offline operation, and reconnect sync.                                                  |

## 9.3 Allowed High-Level Physical Actions

```text
read_battery
read_sensor_summary
read_map
move_to_waypoint
return_to_base
dock
inspect_zone
capture_image
pause_mission
resume_mission
request_operator_override
notify_operator
upload_trace_summary
```

## 9.4 Forbidden Direct Splendor Actions

```text
set_motor_pwm_1
set_motor_pwm_2
raw actuator writes
disable_firmware_safety
bypass_collision_avoidance
modify flight-controller internals
ignore emergency stop
```

## 9.5 Isolated Sprints

| Sprint  | Scope                      | Deliverables                                                                      | Exit gate                                                            |
| ------- | -------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| 0.05-S1 | Device profile schema      | Node kind, capabilities, safety constraints, locality, runtime mode               | Device node can advertise physical capabilities and constraints.     |
| 0.05-S2 | Offline policy cache       | TTL, degraded mode, high-risk denial, local operator approval                     | Disconnected node continues only within cached policy.               |
| 0.05-S3 | Local trace buffer         | Append-only local buffer, reconnect sync, validation                              | Offline trace sync preserves order and integrity.                    |
| 0.05-S4 | Robotics adapter interface | High-level action contract and adapter skeletons                                  | Adapter rejects raw actuator or firmware-bypass actions.             |
| 0.05-S5 | Safety verifier API        | Device-local verifier plugins, fail-closed behavior, evidence capture             | Unsafe route/action is denied locally with evidence.                 |
| 0.05-S6 | Cloud-helper pattern       | Route planner/helper work order, local validation, no direct actuator authority   | Cloud helper proposes; device Splendor verifies and gates execution. |
| 0.05-S7 | Physical demo harness      | Simulated drone/robot mission, verifier denial, operator intervention, trace sync | Demo proves physical orchestration without low-level control.        |

## 9.6 Exit Gate

A resident device instance can run under cached policy, deny unsafe actions locally, buffer traces offline, sync after reconnect, and accept cloud helper proposals without granting direct actuator authority.

## 9.7 Non-Goals

```text
no motor control
no flight controller replacement
no PLC replacement
no ROS or device-driver replacement
no hard real-time safety loop
no production robotics safety certification claim
```

---

# 10. Splendor0.1-dev — Stable Primitive Spec and Compatibility Line

## 10.1 Goal

Freeze the first compatibility surface for Splendor primitives, clients, adapters, SDKs, daemon APIs, and conformance tests.

## 10.2 Functional Requirements

| ID        | Requirement                                                                                                                                                                                     |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FR-0.1-01 | Publish stable primitive specs for Tenant, Agent, Run, Tick, Action, Percept, Message, StateNode, TraceEvent, WorkOrder, Approval, Policy, Constraint, Verifier, Adapter, Feedback, and Reward. |
| FR-0.1-02 | Define schema versioning and compatibility rules.                                                                                                                                               |
| FR-0.1-03 | Define runtime compatibility between CLI, Rust crates, Python SDK, TypeScript client, adapters, and daemon API.                                                                                 |
| FR-0.1-04 | Define adapter certification levels: experimental, local-safe, network-safe, governance-aware, and device-safe.                                                                                 |
| FR-0.1-05 | Define conformance tests for gateway, trace ordering, state commits, replay, messages, work orders, governance, and adapters.                                                                   |
| FR-0.1-06 | Publish migration policy from 0.01-0.05 dev schemas to 0.1 stable schemas.                                                                                                                      |
| FR-0.1-07 | Publish operational guides for local runtime, resident nodes, multi-host fleet, governance, Harmony integration, and physical/edge.                                                             |
| FR-0.1-08 | Guarantee no silent side-effect bypass, no silent verifier failure, no untraceable action, and no replay side effects by default.                                                               |

## 10.3 Isolated Sprints

| Sprint | Scope                    | Deliverables                                                                     | Exit gate                                                       |
| ------ | ------------------------ | -------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| 0.1-S1 | Stable schema freeze     | Canonical schemas, generated Rust/Python/TS types, versioning rules              | Primitive schemas are frozen for 0.1 compatibility.             |
| 0.1-S2 | Compatibility test suite | Conformance tests for runtime, gateway, trace, state, replay, messages, adapters | Implementations can be tested against a shared suite.           |
| 0.1-S3 | Adapter maturity model   | Adapter lifecycle, certification levels, security checklist, examples            | Adapter authors know required behavior and certification level. |
| 0.1-S4 | SDK/API stabilization    | Rust traits, Python SDK, daemon API, TypeScript client compatibility contract    | Clients and adapters have stable integration surfaces.          |
| 0.1-S5 | Operational docs         | Local, daemon, fleet, governance, physical/edge, Harmony integration             | Operators can deploy each supported mode.                       |
| 0.1-S6 | Migration and release    | Migration guide, changelog, deprecation policy, 0.1 release tag                  | Users can move from dev milestones to 0.1 stable line.          |

## 10.4 Exit Gate

Third-party implementers can build an adapter, client, policy host, or integration against 0.1 primitives and expect compatibility across patch releases.

## 10.5 Non-Goals

```text
no marketplace
no enterprise SaaS UI
no full distributed consensus engine
no universal distributed memory
no production robotics certification claim
```

---

# 11. Required PR Template for Implementation Agents

Use this template for all implementation PRs.

```md
# PR Title

## Milestone and Sprint

- Milestone:
- Sprint:
- FRs:
- Primitives strengthened:

## Summary

Describe the smallest primitive-aligned change.

## Non-Goals

-
-

## Runtime Loop Impact

- Percepts:
- Policy:
- Constraints / verifiers:
- Gateway:
- Adapters:
- Outcomes:
- State:
- Trace:
- Replay:

## Security / Isolation Impact

- Tenant scope:
- Agent scope:
- Run scope:
- Quotas:
- Permissions:
- Fail-closed paths:

## Tests

- Positive path:
- Denial path:
- Failure path:
- Trace path:
- Replay path:
- Compatibility path:

## Docs / Examples

- Updated docs:
- Updated examples:
- Migration notes:

## Review Checklist

[ ] No side-effect bypass.
[ ] Trace events are complete.
[ ] State transition is explicit.
[ ] Replay behavior is safe.
[ ] Verifier failure fails closed.
[ ] Scope did not leak into later milestones.
```

---

# 12. Implementation Agent Review Checklist

Before merging, reviewers must ask:

```text
1. Which Splendor primitive does this strengthen?
2. Does it preserve the runtime loop?
3. Does it keep side effects behind the gateway?
4. Does it produce trace events?
5. Does it commit or reference state correctly?
6. Does it respect tenant/agent/run identity separation?
7. Does it work in the intended deployment mode?
8. Does it avoid replacing real-time controllers?
9. Does it avoid assuming one agent equals one pod?
10. Does it fail closed when verification is unavailable?
11. Does it support replay without accidental side effects?
12. Does it remain usable by Harmony without being Harmony-specific?
13. Does it improve distributed computing clarity rather than hiding it?
14. Does it stay inside the sprint boundary?
```

If the answer is unclear, do not merge broad abstractions. Implement the smallest primitive-aligned version.

---

# 13. Suggested Issue Labels

```text
milestone/0.01-dev
milestone/0.02-dev
milestone/0.03-dev
milestone/0.04-dev
milestone/0.05-dev
milestone/0.1-dev

sprint/0.02-S1-message-schema
sprint/0.02-S2-local-router
sprint/0.02-S3-agent-isolation
sprint/0.02-S4-local-delegation
sprint/0.02-S5-daemon-api
sprint/0.02-S6-typescript-surface
sprint/0.02-S7-replay-harness

primitive/gateway
primitive/verifier
primitive/state-graph
primitive/trace-store
primitive/replay
primitive/message
primitive/work-order
primitive/fleet
primitive/governance
primitive/adapter
primitive/sdk
primitive/physical-edge

risk/security
risk/compatibility
risk/distributed-state
risk/physical-safety
risk/side-effects
risk/replay

type/schema
type/runtime
type/sdk
type/docs
type/test
type/example
```

---

# 14. Suggested Repository Structure Additions

Use only where they fit the existing repository shape.

```text
/specs/
  primitives/
  messages/
  work-orders/
  governance/
  fleet/
  physical-edge/

/conformance/
  gateway/
  trace/
  state/
  replay/
  messages/
  work-orders/
  governance/
  adapters/

/examples/
  local-single-agent/
  local-multi-agent/
  daemon-client-ts/
  resident-node/
  signed-work-order/
  governance-approval/
  simulated-device/

/docs/
  roadmap/
  implementation-agent-guide.md
  runtime-loop.md
  action-gateway.md
  trace-and-replay.md
  multi-agent.md
  fleet.md
  governance.md
  physical-edge.md
  adapter-maturity.md
```

---

# 15. Final Sequencing Rule

Do not jump from local runtime directly to broad fleet orchestration.

Correct order:

```text
local correctness
  -> local multi-agent correctness
  -> daemon/control correctness
  -> resident/fleet identity
  -> distributed execution
  -> governance
  -> physical/edge
  -> stable spec
```

This protects the Splendor thesis: verified side effects, explicit state, ordered traces, replayable execution, stable identity, governed distributed autonomy, and clear boundaries between runtime kernel, product control plane, and physical controllers.

---

# 16. References for Implementation Agents

Use these as orientation references. The repository code and versioned specs remain the implementation source of truth.

- Splendor website: https://splendor-os.org/
- Kernel repository: https://github.com/splendor-os/kernel
- Kernel DeepWiki: https://deepwiki.com/splendor-os/kernel
- Splendor docs: https://docs.splendor-os.org/
