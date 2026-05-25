# Splendor Development Modelation

**Date:** 2026-05-25  
**Audience:** Splendor implementation agents, architecture agents, runtime agents, cloud/edge orchestration agents, Harmony integration agents.  
**Purpose:** Provide a clear, non-drifting implementation model for Splendor as a kernel-grade AI runtime for primitives, centralized management, distributed computing, cloud orchestration, physical orchestration, and stable agent execution.

---

## 0. Executive Position

Splendor should be treated as an **AI runtime kernel / autonomy management layer**, not as a generic chat-agent framework, not as a replacement for enterprise product surfaces, and not as a robot's low-level real-time controller.

Splendor's core value is to enforce and standardize AI runtime primitives:

- Percepts
- Agent identity
- Runtime contexts
- State graph
- Trace store
- Ticks
- Policies
- Constraints
- Action gateway
- Verifiers
- Quotas
- Outcomes
- Feedback
- Rewards
- Messages
- Replay
- Runtime migration
- Fleet identity
- Local and remote orchestration boundaries

The system must support:

1. **Cloud digital agents** running ephemeral or resident instances.
2. **Physical agents** running on robots, drones, humanoids, edge devices, industrial devices, or embedded systems.
3. **Hybrid deployments** where local physical Splendor instances coordinate with cloud or on-prem Splendor instances.
4. **Centralized management** for identity, policy, fleet registration, work-order dispatch, telemetry, traces, audits, approvals, and upgrades.
5. **Stable distributed computing** with explicit boundaries, state continuity, traceability, failure handling, and scoped coordination.
6. **Enterprise integration**, including Harmony, without making Harmony the center of Splendor's design.

The guiding principle:

> Splendor governs autonomous AI loops. Central managers schedule, configure, inspect, and coordinate those loops. External product platforms such as Harmony integrate through stable SDKs and protocols, but Splendor remains responsible for runtime primitives and enforced execution boundaries.

---

## 1. Non-Drift Principles

Implementation agents must not drift from these principles.

### 1.1 Splendor is a runtime kernel, not a chat app

Do not build Splendor around conversations, prompts, UI chat, or generic assistants first.

The primary unit is not a chat message. The primary unit is a **governed runtime loop**:

```text
Percepts -> Policy -> Constraints -> Gateway -> Adapter -> Outcome -> State Commit -> Trace
```

Chat can be one type of percept or one type of interface, but it is not the runtime model.

### 1.2 Splendor is not a replacement for enterprise product control planes

Splendor should expose primitives and runtime control. Platforms such as Harmony can provide:

- User interface
- Organization modeling
- Users and groups
- Business workspaces
- Approval queues
- Artifact registries
- Data-space UX
- Enterprise connector UX
- Billing
- Admin consoles

Splendor should not become a monolithic enterprise SaaS product.

### 1.3 Splendor is not a robot's real-time controller

For robotics, Splendor manages AI autonomy and enforces AI control primitives.

Splendor should not replace:

- Motor controllers
- Flight controllers
- Servo loops
- PLC safety systems
- Low-level actuator firmware
- Hard real-time stabilization
- ROS/native device drivers

Correct robotics boundary:

```text
Central Manager / Harmony / Fleet Console
  -> mission, policy, operator approvals, audit

Splendor on device
  -> AI control, autonomy supervision, action gating, state, trace, policy enforcement

Robot middleware / ROS / native stack
  -> sensors, motion planning hooks, actuator APIs

Real-time controller / firmware
  -> hard real-time control and physical safety loops
```

### 1.4 Do not map agents directly to containers

Do not assume:

```text
1 agent = 1 Docker image = 1 Kubernetes pod
```

Correct rule:

```text
1 isolation domain = 1 Splendor instance
1 agent run = 1 runtime context / slot
1 sensitive or physical boundary = likely 1 dedicated instance
```

### 1.5 Side effects must go through the action gateway

No tool, connector, filesystem write, network call, robot command, database mutation, email, ticket creation, or artifact publish should bypass Splendor's gateway once a run is delegated to Splendor.

The action gateway is not optional. It is the enforcement boundary.

### 1.6 State and trace are first-class primitives

Every meaningful runtime transition must be traceable. Every agent state transition must be explicit, versioned, replayable, and inspectable.

Implementation agents must not treat traces as logs appended after the fact. Traces are part of the runtime contract.

### 1.7 Distributed computing must be explicit, not magical

Splendor should not pretend distributed autonomy is solved by simply running many containers.

Distributed Splendor requires explicit:

- Node identity
- Instance identity
- Agent identity
- Run identity
- Message semantics
- State ownership
- State transfer
- Trace continuity
- Work-order signing
- Capability advertisement
- Policy distribution
- Failure recovery
- Offline behavior
- Replay semantics

---

## 2. Core Object Model

Splendor should standardize the following object hierarchy.

```text
Fleet
 └── Node
      └── Splendor Instance
           └── Tenant Context
                └── Agent Runtime Context
                     └── Run
                          └── Tick
                               └── Action
```

### 2.1 Fleet

A governed set of nodes and instances.

Examples:

- Cloud worker fleet
- Finance agent fleet
- Warehouse robot fleet
- Drone inspection fleet
- On-prem customer fleet
- Developer local fleet
- Edge appliance fleet

A fleet has:

- Fleet ID
- Owner authority
- Deployment type
- Policy channel
- Upgrade channel
- Node registry
- Health telemetry
- Kill-switch scope
- Trust level
- Allowed workloads

### 2.2 Node

A physical, virtual, or logical machine capable of hosting one or more Splendor instances.

Examples:

- Kubernetes node
- VM
- Bare-metal server
- Robot computer
- Drone onboard computer
- Factory edge box
- Customer VPC appliance
- Developer laptop

A node advertises:

```json
{
  "node_id": "node_drone_017",
  "kind": "physical.robot.drone",
  "tenant_id": "acme",
  "location": "warehouse-lisbon-1",
  "capabilities": [
    "camera.rgb",
    "gps",
    "motion.waypoint",
    "dock",
    "local.llm.small",
    "http.egress.restricted"
  ],
  "constraints": {
    "max_altitude_m": 30,
    "requires_operator_for": [
      "external_area_flight",
      "payload_release"
    ]
  },
  "health": {
    "battery": 0.82,
    "network": "online",
    "runtime": "splendor-0.03-dev"
  }
}
```

### 2.3 Splendor Instance

A running runtime process/node that hosts one or more runtime contexts.

In cloud, an instance may be packaged as:

- OCI image
- Docker container
- Kubernetes pod
- Nomad task
- Firecracker microVM
- VM
- Systemd service

In physical deployments, an instance may run as:

- Bare-metal process
- Systemd service
- ROS-adjacent service
- Edge appliance daemon
- Container, if the device stack supports it

A Splendor instance hosts:

- Scheduler
- Runtime contexts
- Policy host
- Perceptor registry
- Constraint evaluator
- Action gateway
- Adapter registry
- Verifier chain
- State graph client/store
- Trace store client/store
- Message router
- Quota enforcement
- Local policy cache
- Local trace buffer
- Health reporter

### 2.4 Tenant Context

A scoped authority boundary for one customer, organization, department, fleet, or deployment tenant.

Tenant context contains:

- Tenant ID
- Allowed agents
- Allowed actions
- Allowed adapters
- Allowed permissions
- Quotas
- Policy bundle
- Data locality constraints
- Network constraints
- Audit requirements

### 2.5 Agent Runtime Context

An isolated execution context for an agent identity inside a Splendor instance.

It has:

- Agent ID
- Agent config
- Policy binding
- State head
- Trace stream
- Message inbox/outbox
- Allowed percept sources
- Allowed action classes
- Quota counters
- Current run ID
- Runtime-local memory references

### 2.6 Run

A single execution of an agent objective.

Examples:

- Generate weekly finance dashboard
- Inspect warehouse aisle 3
- Prepare board memo
- Monitor support queue for one hour
- Plan route for drone fleet
- Reconcile invoices

A run has:

- Run ID
- Objective
- Work order
- Agent ID
- Tenant ID
- Placement target
- Start time
- End time
- Status
- State head
- Trace range
- Approval references
- Artifact references
- Parent/child run references

### 2.7 Tick

One loop cycle inside a run.

A tick should include:

1. Tick started
2. Percepts received
3. State loaded
4. Policy invoked
5. Candidate actions proposed
6. Constraints evaluated
7. Verification started
8. Action executed, denied, failed, or deferred
9. Outcome recorded
10. Feedback/reward attached, if any
11. State committed
12. Tick completed

### 2.8 Action

A proposed side effect or read operation mediated by Splendor.

Action fields should include:

- Action ID
- Name
- Adapter
- Params
- Side-effect class
- Required permissions
- Preconditions
- Postconditions
- Cost estimate
- Quota usage estimate
- Safety class
- Approval requirements
- Created timestamp

---

## 3. Deployment Model

### 3.1 Splendor instance is a runtime deployment, not an agent identity

A Splendor instance is a runtime node. It can host one or more agents.

Do not build a separate image for every agent by default.

Default deployment:

```text
Generic Splendor runtime image
  + runtime config
  + signed work order
  + agent bundle
  + policy bundle
  + adapter permissions
```

Example:

```text
Image:
  splendor-runtime:0.1

Runtime work order:
  agent = finance.board_reporting
  tenant = acme
  data_space = finance
  allowed_actions = sql.read, artifact.create, approval.request
```

### 3.2 Per-agent images are allowed only when justified

A dedicated per-agent or per-domain image makes sense when the agent requires:

- Custom Python dependencies
- GPU libraries
- Robotics SDKs
- Local model weights
- Regulated dependency pinning
- Customer-specific runtime certification
- Hardware-specific adapters
- Strong supply-chain isolation
- Air-gapped deployment
- Different kernel/runtime version

Examples:

```text
splendor-runtime:0.1-finance-risk
splendor-runtime:0.1-robotics-drone
splendor-runtime:0.1-legal-onprem
splendor-runtime:0.1-vision-gpu
```

This is an advanced deployment mode, not the default.

### 3.3 Mode A: Ephemeral on-demand instances

Mode A is for elastic cloud or on-prem jobs.

Flow:

```text
Central manager receives task
  -> creates signed work order
  -> scheduler starts Splendor instance
  -> instance pulls agent bundle/config
  -> Splendor runs agent loop
  -> traces/state/artifacts stream back
  -> instance exits, hibernates, or returns to pool
```

Good for:

- Finance reports
- Document analysis
- SQL analysis
- One-off workflows
- Artifact generation
- Data profiling
- Heavy simulation
- Burst workloads
- Secure isolated jobs

Kubernetes example:

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: splendor-run-finance-weekly
spec:
  template:
    spec:
      containers:
        - name: splendor
          image: splendor-runtime:0.1
          env:
            - name: SPLENDOR_WORK_ORDER_ID
              value: wo_123
            - name: SPLENDOR_RUNTIME_MODE
              value: ephemeral
      restartPolicy: Never
```

### 3.4 Mode B: Resident instances

Mode B is for long-running nodes, physical devices, warm pools, and data-local runtimes.

Flow:

```text
Splendor instance starts once
  -> registers with central manager
  -> reports capabilities
  -> receives signed work orders
  -> runs many runtime contexts over time
  -> streams traces and health
  -> buffers offline if needed
```

Good for:

- Robots
- Drones
- Humanoids
- Factory floor agents
- Desktop automation sidecars
- Customer VPC agents
- On-prem private data agents
- High-frequency digital agents
- Warm tenant pools

Kubernetes resident pool example:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: splendor-finance-pool
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: splendor-node
          image: splendor-runtime:0.1
          env:
            - name: SPLENDOR_RUNTIME_MODE
              value: resident
            - name: SPLENDOR_WORKSPACE
              value: finance
```

Physical systemd example:

```text
splendor-node \
  --device-id drone_017 \
  --capabilities camera,gps,waypoint,dock \
  --policy-cache /var/lib/splendor/policies \
  --trace-buffer /var/lib/splendor/traces
```

### 3.5 Hybrid deployments must be first-class

Many real deployments combine Mode A and Mode B.

Enterprise finance:

```text
Resident:
  warm finance Splendor pool for daily/weekly work

Ephemeral:
  burst pods for monthly close, scenario simulation, large analysis
```

Robotics:

```text
Resident:
  Splendor on robot for mission supervision and action gating

Ephemeral cloud:
  heavy simulation, long-horizon planning, model evaluation, fleet analytics
```

On-prem enterprise:

```text
Resident:
  on-prem Splendor node near private data

Ephemeral:
  temporary local jobs for sensitive analysis
  optional cloud jobs for non-sensitive computation
```

The scheduler must choose based on:

- Data locality
- Latency
- Hardware requirements
- Security boundary
- Tenant isolation
- Cost
- Runtime duration
- Network availability
- Physical locality
- Approval requirements
- Workload criticality

---

## 4. Sub-Agent Model

Sub-agents should not automatically receive their own Splendor instance.

The correct rule:

> Sub-agents get their own instance only when they require an independent security, locality, hardware, dependency, or lifecycle boundary.

### 4.1 Level 1: Sub-agent as module/tool/policy component

Default model.

Examples:

- CSV cleaner
- SQL explainer
- Chart generator
- Anomaly detector
- Prompt planner
- Report section writer

Use this when the sub-agent:

- Has no separate identity
- Needs no separate memory
- Needs no separate permissions
- Performs no risky side effects
- Needs no independent audit ownership

### 4.2 Level 2: Sub-agent as named agent in same instance

Use this when identity and traceability matter, but process isolation does not.

Example:

```text
Finance Orchestrator Agent
  -> Revenue Analyst Agent
  -> Expense Analyst Agent
  -> Forecast Agent
  -> Artifact Builder Agent
```

Each named sub-agent has:

- Agent ID
- State head
- Trace stream
- Permissions
- Quotas
- Message inbox/outbox

All can run in the same Splendor instance.

### 4.3 Level 3: Shared resident specialist agent

Use this when many agents reuse the same specialist.

Examples:

- Document Parsing Agent
- SQL Profiling Agent
- Artifact Rendering Agent
- Retrieval Agent
- Compliance Review Agent
- Vision Perception Agent

Important constraint:

> Shared agents must not become permission-laundering services.

They must receive scoped work orders, not inherited broad caller permissions.

Correct:

```text
Parse this document reference.
Return extracted tables.
Do not access caller's entire data space.
```

Incorrect:

```text
Shared parser inherits every permission of every caller.
```

### 4.4 Level 4: Sub-agent in separate instance

Use this when isolation is required.

Reasons:

- Different tenant
- Different trust level
- Different hardware
- Different network zone
- Different data residency boundary
- Different dependency stack
- GPU isolation
- Robot/device boundary
- Dangerous side effects
- Long-lived autonomous behavior

Examples:

```text
Drone Mission Agent on drone
  -> Cloud Route Planning Agent

Finance Agent in cloud
  -> On-Prem SQL Agent near private warehouse

Humanoid Floor Agent
  -> Vision Agent on local GPU module

Legal Agent
  -> Compliance Agent in isolated regulated runtime
```

---

## 5. Centralized Management Plane

Splendor needs a centralized management model, but the center should manage and govern runtimes, not replace them.

### 5.1 Central manager responsibilities

A Splendor central manager should provide:

- Fleet registry
- Node registry
- Instance registry
- Agent registry
- Work-order issuing
- Placement decisions
- Policy distribution
- Capability discovery
- Upgrade channels
- Runtime health
- Trace aggregation
- State-head indexing
- Approval routing
- Kill switches
- Credential brokering
- Trust and attestation metadata
- Audit export

### 5.2 Central manager must not own local autonomy loops

Resident physical and edge instances must continue operating within cached policies when disconnected.

The central manager should not be required for every local tick.

Correct offline behavior:

```text
Network lost
  -> local Splendor instance continues within cached policy
  -> high-risk actions denied or require local operator approval
  -> traces buffer locally
  -> policy TTL enforced
  -> reconnect syncs traces/state summaries
```

### 5.3 Work orders

All runs should be initiated through signed work orders.

A work order should include:

```json
{
  "work_order_id": "wo_123",
  "tenant_id": "tenant_acme",
  "agent_id": "finance.board_reporting",
  "objective": "Generate weekly revenue dashboard",
  "allowed_actions": [
    "sql.query",
    "artifact.create",
    "approval.request"
  ],
  "allowed_adapters": [
    "harmony",
    "sql",
    "artifact-store"
  ],
  "allowed_permissions": [
    "finance.read",
    "artifact.create"
  ],
  "data_refs": [
    "dataset:finance.revenue_monthly_v4"
  ],
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

Instances must reject unsigned, expired, revoked, or incompatible work orders.

---

## 6. Distributed Computing Model

Distributed Splendor is not simply multiple pods. It is a coordinated set of identity-bearing runtime instances.

### 6.1 Distributed object identities

Use stable identities:

```text
fleet_id
node_id
instance_id
tenant_id
agent_id
run_id
tick_id
action_id
state_node_id
trace_event_id
message_id
```

Do not overload one ID for multiple concepts.

### 6.2 Placement

Central placement should choose execution targets based on declared capabilities and constraints.

Type model:

```ts
type PlacementTarget =
  | "ephemeral_cloud"
  | "resident_cloud_pool"
  | "customer_vpc"
  | "on_prem"
  | "edge_device"
  | "physical_robot"
  | "desktop_sidecar";

interface PlacementDecision {
  target: PlacementTarget;
  reason: string[];
  dedicated_instance: boolean;
  required_capabilities: string[];
  max_runtime_ms?: number;
  data_locality?: "cloud" | "vpc" | "on_prem" | "device";
}
```

Examples:

```text
Finance weekly dashboard:
  resident_cloud_pool or ephemeral_cloud

Payroll-sensitive analysis:
  dedicated on_prem or customer_vpc

Drone warehouse inspection:
  physical_robot resident instance

Drone fleet route optimization:
  ephemeral_cloud helper with no direct actuator permission

Shared document parsing:
  resident_cloud_pool

External email sending:
  same run, but action requires approval gate
```

### 6.3 Messages between agents

Distributed and local multi-agent systems require typed messages.

Message fields:

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

Messages must be trace-linked.

### 6.4 State ownership

Only one runtime context should own write authority for a state head at a time.

Distributed state sharing must be explicit:

- Read-only state reference
- Snapshot handoff
- State migration
- State fork
- State merge with deterministic conflict handling
- Message-based coordination

Avoid hidden mutable shared state.

### 6.5 Runtime migration

If a run moves from one instance to another:

```text
Instance A interrupted
  -> central manager marks run interrupted
  -> state head and trace range identified
  -> Instance B receives resume work order
  -> B validates state snapshot and trace continuity
  -> B resumes run
  -> migration event recorded
```

Migration must be visible in trace.

### 6.6 Failure handling

Required failure states:

```text
pending
running
paused
waiting_for_approval
interrupted
resuming
completed
failed
cancelled
denied
expired
```

Required recovery behavior:

- Retry idempotent actions only when explicitly marked idempotent.
- Never retry dangerous side effects blindly.
- On verifier uncertainty, deny or request intervention.
- On policy expiration, pause or deny high-risk actions.
- On trace-store failure, fail closed for side-effectful actions.
- On state-commit failure, do not proceed to next tick.

---

## 7. Physical Orchestration Model

Physical devices should run resident Splendor instances when they require autonomy supervision.

### 7.1 Device stack

```text
Physical Device
 ├── Real-time controller / firmware
 ├── Robotics middleware / ROS / native stack
 ├── Splendor device instance
 │    ├── Mission Agent
 │    ├── Safety Supervisor Agent
 │    ├── Perception Summary Agent
 │    ├── Action Gateway
 │    ├── Local policy cache
 │    ├── Local trace buffer
 │    └── Device adapters
 └── Central manager connector
```

### 7.2 Action classes for robotics

Allowed high-level actions:

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

Forbidden as Splendor direct actions:

```text
set_motor_pwm_1
set_motor_pwm_2
raw actuator writes
disable_firmware_safety
bypass_collision_avoidance
modify flight-controller internals
```

### 7.3 Local verifier examples

Drone local verifier:

- Geofence check
- Altitude limit
- Battery threshold
- No-fly zone
- Collision risk
- Link quality
- Sensor health
- Payload constraints
- Operator override state

Humanoid local verifier:

- Human proximity
- Force limit
- Joint limit
- Balance state
- Workspace boundary
- Tool safety status
- Emergency stop status
- Privacy zone check

### 7.4 Cloud helper instances for physical fleets

Robots and drones may call cloud Splendor instances for:

- Long-horizon planning
- Simulation
- Fleet optimization
- Model evaluation
- Heavy perception processing
- Mission summarization
- Trace analytics
- Policy update recommendations

Cloud helpers must not receive direct actuator authority unless explicitly authorized and safety-reviewed.

Correct:

```text
Cloud route planner proposes route.
Device Splendor instance verifies route locally.
Robot middleware executes bounded waypoint actions.
Real-time controller maintains stability and safety.
```

Incorrect:

```text
Cloud model directly controls motors.
```

---

## 8. Digital Orchestration Model

Digital Splendor deployments support cloud, on-prem, customer VPC, desktop, and edge instances.

### 8.1 Digital instance types

```text
Ephemeral job instance
  -> one run, then exit

Resident pool instance
  -> many runs over time

Dedicated sensitive instance
  -> one tenant/workspace/security boundary

Customer VPC instance
  -> near private customer systems

On-prem instance
  -> local data residency and low-latency private connectors

Desktop sidecar
  -> local app/file interaction

Shared specialist instance
  -> scoped service for parsing, retrieval, rendering, etc.
```

### 8.2 Digital actions

Typical actions:

```text
sql.query
file.read
file.write
artifact.create
artifact.publish
approval.request
email.draft
email.send
ticket.create
crm.update
calendar.create
http.fetch
webhook.call
code.run
notebook.execute
```

Side effects require gateway enforcement.

### 8.3 Data and artifacts

Splendor should not own the full enterprise data workspace UX, but it must understand runtime-level data references.

Use references:

```text
dataset:finance.revenue_monthly_v4
query:approved.finance.weekly_revenue
file:drive.abc123
artifact:weekly-revenue-dashboard.v2
memory:agent.finance.summary
```

Do not pass broad credentials or raw data access by default.

Artifacts produced by agents should be trace-linked:

```text
Artifact
 ├── artifact_id
 ├── version
 ├── source_refs
 ├── run_id
 ├── state_node_id
 ├── trace_range
 ├── approval_state
 └── export_targets
```

---

## 9. Action Gateway and Verifier Contract

The action gateway is the central enforcement primitive.

### 9.1 Action request

```ts
interface ActionRequest {
  action_id: string;
  tenant_id: string;
  agent_id: string;
  run_id: string;
  action: {
    name: string;
    adapter?: string;
    params: Record<string, unknown>;
    side_effect_class?: string;
    required_permissions?: string[];
    preconditions?: string[];
    postconditions?: string[];
  };
  quota_usage?: Partial<QuotaUsage>;
  satisfied_preconditions?: string[];
  requested_at: string;
}
```

### 9.2 Action outcome

```ts
interface ActionOutcome {
  action_id: string;
  status: "executed" | "denied" | "failed" | "needs_approval" | "needs_intervention";
  verification: VerificationResult;
  post_verification?: VerificationResult;
  output?: unknown;
  error?: string;
  completed_at: string;
}
```

### 9.3 Verification result

```ts
interface VerificationResult {
  allowed: boolean;
  reasons: string[];
  verifier_results: Array<{
    verifier: string;
    allowed: boolean;
    reason: string;
    evidence?: unknown;
  }>;
}
```

### 9.4 Required verifier categories

At minimum:

- Tenant verifier
- Agent permission verifier
- Adapter verifier
- Quota verifier
- Preconditions verifier
- Data-scope verifier
- Network egress verifier
- Filesystem verifier
- Approval verifier
- Safety verifier
- Policy TTL verifier
- Capability verifier
- Postcondition verifier

### 9.5 Fail-closed behavior

If verification cannot complete, default to deny or intervention.

Do not let unavailable verifiers silently allow side effects.

---

## 10. State Graph and Trace Store

### 10.1 State graph

Every agent runtime context has a state graph.

State commit fields:

```json
{
  "state_node_id": "state_123",
  "agent_id": "finance.board_reporting",
  "run_id": "run_456",
  "parents": ["state_122"],
  "state_hash": "...",
  "snapshot_ref": "object://snapshots/state_123",
  "metadata": {
    "tick": 17,
    "reason": "artifact draft created"
  },
  "created_at": "2026-05-25T00:00:00Z"
}
```

### 10.2 Trace store

Trace events must be append-only and ordered within a run.

Required trace event classes:

```text
tick.started
percepts.received
state.loaded
policy.invoked
policy.completed
actions.proposed
constraints.evaluated
verification.started
verification.completed
action.executed
action.denied
action.failed
action.needs_approval
outcome.recorded
feedback.received
reward.recorded
state.committed
tick.completed
run.paused
run.resumed
run.migrated
run.completed
run.failed
```

Trace events should support hash chaining or an equivalent tamper-evident mechanism.

### 10.3 Replay

Replay must be a core feature.

Replay should support:

- Reconstructing state transitions
- Inspecting percepts
- Inspecting policy outputs
- Inspecting verifier decisions
- Replaying read-only actions
- Simulating denied/dangerous actions without executing side effects
- Comparing policy versions
- Explaining why an action was allowed or denied

Replay must not blindly re-execute side effects.

---

## 11. Language and SDK Strategy

### 11.1 Rust core

Rust should own:

- Kernel runtime
- Scheduler
- Action gateway
- Verifier chain
- State graph primitives
- Trace store primitives
- Quotas
- Runtime identity
- Local message router
- Adapter boundary
- Policy/runtime safety boundary

### 11.2 Python managed compute

Python should own:

- Model calls
- Domain policies
- Data science
- Document processing
- Planning logic
- Simulation logic
- Robotics high-level adapters where suitable
- ML evaluation
- Forecasting
- Enterprise analytics

### 11.3 TypeScript API

Splendor should expose TypeScript APIs, but in the right order:

```text
1. @splendor/types
   Pure schemas and generated types

2. @splendor/client
   Remote client for Splendor daemon/runtime

3. @splendor/harmony
   Harmony integration adapter

4. @splendor/native
   Optional Node native binding through Rust/N-API

5. @splendor/wasm
   Optional future browser/edge-safe partial runtime
```

TypeScript should be the control-plane and integration language, not necessarily the primary policy/runtime language.

### 11.4 Remote daemon API before native Node binding

Preferred production boundary:

```text
TypeScript control plane
  -> HTTP/gRPC/NATS
  -> Splendor runtime daemon / sidecar
  -> Rust core + Python managed compute
```

Native Node binding is useful for:

- Local development
- Testing
- Replay tools
- Trace parsing
- State inspection
- Embedded developer runtimes

Native Node binding should not be the default production boundary for critical physical systems.

### 11.5 N-API / napi-rs path

When building `@splendor/native`, use the Node-API/N-API style boundary through Rust tooling such as napi-rs.

Native binding should expose:

```ts
class KernelRuntime {
  constructor(config: KernelRuntimeConfig);
  createTenant(config: TenantConfig): string;
  createAgent(config: AgentConfig): string;
  appendPercept(agentId: string, percept: Percept): Promise<void>;
  runOnce(agentId: string): Promise<TickResult>;
  runUntil(config: RunUntilConfig): Promise<RunResult>;
  pause(runId: string): Promise<void>;
  resume(runId: string): Promise<void>;
  getStateHead(agentId: string): Promise<StateHead>;
  streamTraces(runId: string): AsyncIterable<TraceEvent>;
}
```

---

## 12. TypeScript Contract Draft

### 12.1 `@splendor/types`

```ts
export type TenantId = string;
export type AgentId = string;
export type RunId = string;
export type ActionId = string;
export type StateNodeId = string;
export type ISODateTime = string;

export interface RunConfig {
  trace_db?: string;
  state_db?: string;
  run_id?: RunId;
  tick_budget_ms?: number;
  tick_interval_ms?: number;
  cycles?: number;
  tenants: TenantConfig[];
  agents: AgentConfig[];
  adapters?: Record<string, unknown>;
}

export interface TenantConfig {
  id: TenantId;
  allowed_actions: string[];
  allowed_adapters: string[];
  allowed_permissions?: string[];
  quotas?: QuotaPolicy;
}

export interface AgentConfig {
  id: AgentId;
  tenant_id: TenantId;
  label?: string;
  metadata?: Record<string, string>;
  initial_state?: unknown;
  resume?: boolean;
}

export interface QuotaPolicy {
  max_actions_per_tick?: number;
  max_action_duration_ms?: number;
  max_filesystem_read_bytes?: number;
  max_filesystem_write_bytes?: number;
  max_network_read_bytes?: number;
  max_network_write_bytes?: number;
  max_http_requests_per_minute?: number;
}

export interface Percept {
  schema: string;
  payload: unknown;
  provenance: {
    source: string;
    detail?: string;
  };
  timestamp: ISODateTime;
}

export interface Action {
  name: string;
  params: Record<string, unknown>;
  side_effect_class?: "read_only" | "filesystem" | "network" | "external" | string;
  required_permissions?: string[];
  preconditions?: string[];
  postconditions?: string[];
}

export interface QuotaUsage {
  actions: number;
  action_duration_ms: number;
  filesystem_read_bytes: number;
  filesystem_write_bytes: number;
  network_read_bytes: number;
  network_write_bytes: number;
  http_requests: number;
}

export interface ActionRequest {
  action_id: ActionId;
  tenant_id: TenantId;
  agent_id: AgentId;
  run_id: RunId;
  action: Action;
  adapter?: string;
  quota_usage?: Partial<QuotaUsage>;
  satisfied_preconditions?: string[];
  requested_at: ISODateTime;
}

export interface VerificationResult {
  allowed: boolean;
  reasons: string[];
  verifier_results?: Array<{
    verifier: string;
    allowed: boolean;
    reason: string;
    evidence?: unknown;
  }>;
}

export interface ActionOutcome {
  action_id: ActionId;
  status: "executed" | "denied" | "failed" | "needs_approval" | "needs_intervention";
  verification: VerificationResult;
  post_verification?: VerificationResult;
  output?: unknown;
  error?: string;
  completed_at: ISODateTime;
}
```

### 12.2 `@splendor/client`

```ts
export class SplendorClient {
  constructor(private readonly opts: { baseUrl: string; token: string }) {}

  createRun(config: RunConfig): Promise<{ run_id: string }>;

  startRun(runId: string): Promise<void>;

  pauseRun(runId: string): Promise<void>;

  resumeRun(runId: string): Promise<void>;

  stopRun(runId: string): Promise<void>;

  appendPercept(runId: string, agentId: string, percept: Percept): Promise<void>;

  submitAction(request: ActionRequest): Promise<ActionOutcome>;

  streamTraces(runId: string): AsyncIterable<TraceEvent>;

  getStateHead(runId: string, agentId: string): Promise<{ state_node_id: string }>;

  replayRun(runId: string): Promise<{ replay_id: string }>;
}
```

---

## 13. Harmony Integration Contract

Harmony should be supported as an integration and management/control-plane consumer, but Splendor should not be optimized only for Harmony.

### 13.1 Harmony should use Splendor for runtime governance

Harmony can delegate autonomous runs to Splendor when it needs:

- Persistent loop execution
- Traceable action decisions
- Verified side effects
- State continuity
- Replay
- Agent quotas
- Approval boundaries
- Runtime migration
- Fleet execution

### 13.2 Harmony owns enterprise product concerns

Harmony may own:

- Org model
- Users/groups
- Business workspaces
- Data spaces
- Connector UX
- Artifact registry
- Approval queue
- Admin console
- Billing
- Enterprise policy UI

### 13.3 Splendor owns runtime concerns

Splendor owns:

- Percepts
- Ticks
- Policies
- Constraints
- Gateway
- Adapters
- Outcomes
- State graph
- Trace store
- Replay
- Messages
- Feedback
- Rewards
- Quotas
- Runtime identity

### 13.4 Harmony adapter endpoints

Minimum Harmony adapter API for Splendor:

```text
GET  /splendor/work-orders/:id
POST /splendor/action-gateway
POST /splendor/traces
POST /splendor/percepts
POST /splendor/feedback
POST /splendor/state-commits
POST /splendor/approvals
```

### 13.5 Work-order rule

Harmony should never give a Splendor run broad user credentials.

Harmony should issue scoped, signed work orders with:

- Allowed data refs
- Allowed actions
- Allowed adapters
- Allowed permissions
- Quotas
- Approval requirements
- Expiration
- Placement target

### 13.6 Artifact rule

If Harmony uses Splendor to generate enterprise artifacts, artifacts should be trace-linked:

```text
Harmony Artifact
 ├── artifact_id
 ├── version
 ├── source_refs
 ├── splendor_run_id
 ├── splendor_state_node_id
 ├── splendor_trace_range
 ├── approval_state
 └── publication_targets
```

---

## 14. Security and Governance Invariants

Implementation agents must enforce these invariants.

### 14.1 No unsigned work orders

A Splendor instance must reject unsigned work orders.

### 14.2 No expired work orders

Expired work orders must not start new runs or authorize new side effects.

### 14.3 No side-effect bypass

Side-effectful actions must go through the action gateway.

### 14.4 No silent policy failure

If policy cannot be loaded or validated, fail closed.

### 14.5 No silent verifier failure

If a required verifier cannot run, deny or request intervention.

### 14.6 No hidden shared mutable state

Agent state must be explicit, versioned, and scoped.

### 14.7 No broad inherited permissions for shared sub-agents

Shared agents must receive scoped delegated permissions.

### 14.8 No direct low-level robot control

Splendor actions for robotics must remain high-level, bounded, and mediated by local safety systems.

### 14.9 No untraceable actions

Every action request, denial, execution, failure, and outcome must be traceable.

### 14.10 No replay side effects by default

Replay must not re-execute side effects unless explicitly in a safe simulation mode.

---

## 15. Implementation Phases

### 15.1 Phase 1: Local/runtime foundation

Deliver:

- Rust runtime core
- Agent runtime context
- Tick loop
- Percepts
- Policy invocation
- Action gateway
- Verifier chain
- State graph
- Trace store
- Quotas
- Filesystem adapter
- HTTP adapter
- Python SDK
- CLI
- Basic replay

Do not build full fleet orchestration yet.

### 15.2 Phase 2: TypeScript and daemon control

Deliver:

- `@splendor/types`
- `@splendor/client`
- Runtime daemon API
- Trace streaming
- Work-order ingestion
- State-head query
- Replay API
- Basic Harmony adapter
- Basic management-plane protocol

### 15.3 Phase 3: Resident nodes and fleet registry

Deliver:

- Node registration
- Instance registration
- Capability advertisement
- Health reporting
- Policy distribution
- Work-order dispatch
- Local trace buffering
- Central trace aggregation
- Kill switch
- Upgrade channel metadata

### 15.4 Phase 4: Distributed multi-agent execution

Deliver:

- Typed messages
- Local multi-agent routing
- Remote agent messaging
- Parent/child runs
- Cross-instance work-order delegation
- State handoff
- Migration trace events
- Failure recovery semantics

### 15.5 Phase 5: Physical orchestration support

Deliver:

- Device node profile
- Device capability model
- Local policy cache
- Offline operation mode
- Safety verifier interface
- Robotics adapter interface
- Mission action classes
- Operator intervention protocol
- Trace sync after reconnect

### 15.6 Phase 6: Stable distributed computing

Deliver:

- Stronger identity continuity
- State migration
- Fleet scheduling policies
- Regional placement
- Customer VPC/on-prem routing
- Distributed replay
- Trace integrity across instances
- Cross-fleet telemetry
- Failure domain isolation

---

## 16. Minimum Implementation Interfaces

### 16.1 Runtime daemon API

```text
POST /runs
POST /runs/:run_id/start
POST /runs/:run_id/pause
POST /runs/:run_id/resume
POST /runs/:run_id/stop
POST /runs/:run_id/percepts
GET  /runs/:run_id/state-head
GET  /runs/:run_id/traces
POST /runs/:run_id/replay
POST /actions
GET  /health
GET  /capabilities
```

### 16.2 Node management API

```text
POST /nodes/register
POST /nodes/:node_id/heartbeat
POST /nodes/:node_id/capabilities
POST /nodes/:node_id/policies/sync
POST /nodes/:node_id/work-orders/poll
POST /nodes/:node_id/traces/sync
POST /nodes/:node_id/state/sync
POST /nodes/:node_id/kill
```

### 16.3 Adapter interface

```ts
interface Adapter {
  name: string;
  capabilities(): AdapterCapability[];
  verify?(request: ActionRequest): Promise<VerificationResult>;
  execute(request: ActionRequest): Promise<ActionOutcome>;
  compensate?(outcome: ActionOutcome): Promise<void>;
}
```

### 16.4 Verifier interface

```ts
interface Verifier {
  name: string;
  appliesTo(request: ActionRequest): boolean;
  verify(context: VerificationContext, request: ActionRequest): Promise<VerifierResult>;
}
```

### 16.5 Policy interface

```ts
interface PolicyHost {
  invoke(input: PolicyInput): Promise<PolicyOutput>;
}

interface PolicyInput {
  tenant_id: string;
  agent_id: string;
  run_id: string;
  state_ref: string;
  percepts: Percept[];
  messages: Message[];
  tick_id: string;
}

interface PolicyOutput {
  proposed_actions: Action[];
  state_patch?: unknown;
  messages?: Message[];
  confidence?: number;
  rationale_ref?: string;
}
```

---

## 17. Concrete Examples

### 17.1 Cloud finance run

```text
User requests weekly dashboard
  -> central manager creates work order
  -> scheduler selects resident finance pool
  -> Splendor instance starts run
  -> percept: dataset revenue_monthly_v4 available
  -> policy proposes sql.query
  -> gateway verifies finance.read permission
  -> SQL adapter executes read-only query
  -> policy proposes artifact.create
  -> gateway verifies artifact.create permission
  -> artifact created
  -> policy proposes artifact.publish
  -> approval verifier denies without CFO approval
  -> action outcome = needs_approval
  -> run pauses
  -> approval percept received
  -> publish action re-proposed
  -> gateway verifies approval token
  -> artifact published
  -> state committed
  -> trace complete
```

### 17.2 Drone inspection run

```text
Central manager sends mission work order to drone_017
  -> drone Splendor instance validates signature
  -> percepts: battery, GPS, map, mission zone
  -> policy proposes inspect_zone(zone=A3)
  -> local verifier checks geofence, battery, altitude, collision status
  -> gateway calls robotics adapter move_to_waypoint
  -> robot middleware handles path command
  -> real-time controller maintains flight stability
  -> camera percept returns inspection image summary
  -> policy proposes upload_summary
  -> network verifier checks connection policy
  -> upload occurs
  -> trace buffered locally and synced to central manager
```

### 17.3 Hybrid robot + cloud planner

```text
Robot local Splendor instance receives mission
  -> local policy asks for route optimization
  -> sends typed message/work order to cloud route planner
  -> cloud Splendor instance computes route proposal
  -> cloud returns route artifact/reference
  -> local device receives route percept
  -> local verifier checks route against geofence/battery/safety
  -> local device executes bounded waypoint actions
```

The cloud planner never receives raw motor authority.

---

## 18. What Not To Build First

Avoid these in v1:

- General enterprise admin SaaS
- Full low-code app builder
- Marketplace
- Universal plugin ecosystem
- Unbounded browser computer
- Full distributed consensus engine
- Arbitrary shared mutable distributed memory
- Real-time robotics controller
- Per-agent Docker image factory as the default
- Complex multi-region fleet scheduler before local runtime is stable
- Hidden side-effect execution outside gateway
- Chat-first architecture

Build primitives first.

---

## 19. Implementation Agent Checklist

Before implementing any feature, answer:

1. Which Splendor primitive does this feature strengthen?
2. Does it preserve the runtime loop?
3. Does it keep side effects behind the gateway?
4. Does it produce trace events?
5. Does it commit or reference state correctly?
6. Does it respect tenant/agent/run identity separation?
7. Does it work in both ephemeral and resident modes?
8. Does it handle disconnected physical nodes if relevant?
9. Does it avoid replacing real-time controllers?
10. Does it avoid assuming one agent equals one pod?
11. Does it fail closed when verification is unavailable?
12. Does it support replay without accidental side effects?
13. Does it remain usable by Harmony without being Harmony-specific?
14. Does it improve stable distributed computing, not obscure it?

If the answer is unclear, do not implement broad abstractions. Implement the smallest primitive-aligned version.

---

## 20. Final Architecture Summary

Splendor should develop toward this shape:

```text
Central Management Plane
 ├── Fleet registry
 ├── Node registry
 ├── Work-order issuer
 ├── Placement engine
 ├── Policy distributor
 ├── Trace aggregator
 ├── State index
 ├── Approval bridge
 ├── Kill switch
 └── Upgrade channel

Splendor Runtime Instance
 ├── Scheduler
 ├── Tenant contexts
 ├── Agent runtime contexts
 ├── Tick loop
 ├── Percepts
 ├── Policy host
 ├── Constraint evaluator
 ├── Action gateway
 ├── Verifier chain
 ├── Adapters
 ├── State graph
 ├── Trace store
 ├── Message router
 ├── Quota enforcement
 └── Health reporter

Deployment Targets
 ├── Ephemeral cloud jobs
 ├── Resident cloud pools
 ├── Customer VPC nodes
 ├── On-prem appliances
 ├── Desktop sidecars
 ├── Edge devices
 ├── Robots
 ├── Drones
 └── Humanoids
```

Core principle:

> Splendor is the AI autonomy management kernel. It enforces primitives, state, traces, verification, quotas, and side-effect boundaries across cloud, edge, and physical deployments. Central managers orchestrate fleets and policies. Platforms like Harmony integrate through stable contracts. Robotics controllers remain responsible for hard real-time control.

This is the implementation direction. Do not drift.
