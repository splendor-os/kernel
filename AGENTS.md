# AGENTS.md — Splendor Implementation Agent Contract

This file is the first instruction document for implementation agents working on Splendor.
It exists to prevent architectural drift, keep work sprint-scoped, and make every contribution verifiable.

Splendor is a **kernel-grade AI runtime for governed agent loops**.
It runs **on top of Unix-like systems**. It is **not** a bare-metal OS, not a chat-agent framework, not an enterprise SaaS product, and not a robot real-time controller.

---

## 1. Mission

Build Splendor as a runtime substrate for persistent autonomous agents.

The runtime must standardize and enforce:

- tenant, agent, run, tick, action, state, trace, and message identity;
- explicit state graph commits;
- append-only trace events;
- verified action boundaries;
- quotas, permissions, policies, constraints, and verifier chains;
- replay without accidental side effects;
- local first, then distributed, then governed fleet execution.

The core loop is:

```text
Percepts
  -> Policy
  -> Constraints
  -> Action Gateway
  -> Verifiers
  -> Adapter
  -> Outcome
  -> State Commit
  -> Trace
```

All work must strengthen this loop.

---

## 2. Required reading before implementation

Before changing code, implementation agents must read the files relevant to their sprint.
Do not skip large files; read them in chunks when needed.

Required for all implementation work:

```text
AGENTS.md
/docs/rules/splendor_dev_model.md
/docs/rules/sprints_frs_milestones.md
/docs/rules/verifiable_criteria/main.md
/docs/rules/verifiable_criteria/sprints/<sprint-id>-<slug>.md
```

Required for primitive or API changes:

```text
/docs/rfc/
/docs/reference/
/docs/concepts/
```

Required for adapter, verifier, gateway, state, trace, replay, or distributed work:

```text
/docs/guides/
/docs/reference/
/examples/
```

If a required document conflicts with another document, follow this priority order:

```text
1. AGENTS.md
2. /docs/rules/splendor_dev_model.md
3. /docs/rules/verifiable_criteria/main.md and the applicable sprint criteria file
4. /docs/rules/sprints_frs_milestones.md
5. /docs/reference/*
6. /docs/guides/*
7. examples and comments
```

Open a docs issue or RFC if the conflict affects a primitive, schema, runtime invariant, or public API.

---

## 3. Non-negotiable invariants

These are blocking requirements. A pull request that violates any of them must not merge.

### 3.1 Gateway enforcement

No side-effectful action may bypass the action gateway.

Side effects include:

- filesystem writes;
- network calls;
- database mutations;
- shell commands;
- ticket creation;
- email sending;
- webhook calls;
- artifact publishing;
- robot/device commands;
- credential use;
- external service mutation.

Read-only actions may still require gateway mediation when they touch protected data, credentials, network, filesystem, tenant resources, or external systems.

### 3.2 Verification before execution

Before an adapter executes an action, required verifiers must run.

Minimum verifier categories:

- tenant verifier;
- agent permission verifier;
- adapter verifier;
- quota verifier;
- precondition verifier;
- data-scope verifier;
- network/filesystem verifier where applicable;
- approval verifier where applicable;
- safety verifier where applicable;
- policy TTL verifier where applicable;
- postcondition verifier after execution where applicable.

If a required verifier cannot run, fail closed: deny, pause, or request intervention.

### 3.3 Trace is runtime contract, not logging

Every meaningful transition must emit trace events.

At minimum, a tick must trace:

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
action.executed | action.denied | action.failed | action.needs_approval
outcome.recorded
state.committed
tick.completed
```

Distributed, migration, approval, denial, circuit-breaker, and replay events must also be trace-linked.

### 3.4 State is explicit and versioned

Agent state must be represented through state graph nodes.
Do not introduce hidden mutable state that affects runtime behavior without a state commit or state reference.

Every state commit must identify:

- state node ID;
- tenant ID;
- agent ID;
- run ID;
- parent state node(s);
- snapshot or patch reference;
- state hash or equivalent integrity field;
- trace linkage;
- timestamp.

### 3.5 Replay must not cause real side effects

Replay is for reconstruction, debugging, audit, comparison, and simulation.
Replay must not blindly re-execute side effects.

Allowed replay modes:

- inspect only;
- read-only re-evaluation;
- safe simulation;
- policy comparison;
- verifier explanation.

Any replay mode that can touch an external system must be explicitly marked, separately gated, and off by default.

### 3.6 Identity separation is mandatory

Do not overload IDs.

Keep these concepts distinct:

```text
fleet_id
node_id
instance_id
tenant_id
agent_id
runtime_context_id
run_id
tick_id
action_id
state_node_id
trace_event_id
message_id
work_order_id
approval_id
artifact_id
```

### 3.7 No permission laundering

Shared agents, specialist agents, tools, adapters, or services must not inherit broad caller permissions by default.

Delegation must be scoped through:

- work order;
- allowed action list;
- allowed adapter list;
- allowed permission list;
- data references;
- quotas;
- expiry;
- trace linkage.

### 3.8 Physical systems boundary

Splendor may govern physical autonomy, but it must not replace real-time controllers.

Allowed physical actions are high-level and bounded, such as:

```text
read_battery
read_sensor_summary
inspect_zone
move_to_waypoint
return_to_base
dock
pause_mission
request_operator_override
upload_trace_summary
```

Forbidden as direct Splendor actions:

```text
raw motor writes
set_motor_pwm
bypass collision avoidance
disable firmware safety
modify flight-controller internals
hard real-time stabilization
```

### 3.9 Python can propose; Rust enforces

Python is for policies, model calls, domain logic, data science, planning, simulation, and adapter ergonomics.
Rust owns runtime enforcement:

- scheduler;
- gateway;
- verifier chain;
- state graph;
- trace store;
- quotas;
- runtime identity;
- message routing;
- adapter boundary;
- fail-closed behavior.

### 3.10 Secure App Communication

Apps, SDKs, CLIs, control planes, adapters, and external services must not communicate with Splendor as anonymous callers.

Every non-dev request to a Splendor daemon, sidecar, node, or central manager must have:

1. authenticated caller identity;
2. tenant or fleet binding;
3. endpoint-level scope authorization;
4. expiry;
5. audience binding;
6. revocation path;
7. trace/audit attribution for mutating calls.

A caller token authenticates the app.
A signed work order authorizes a run.
The Action Gateway authorizes side effects.

Never treat a daemon API token as permission to execute arbitrary agent actions.
Never allow expired or revoked work orders to create, resume, or authorize runs.
Never allow SDKs to bypass daemon/gateway enforcement.
Never expose unauthenticated TCP listeners except in explicit local dev mode.

Local development may use an insecure mode only when all of the following are true:

- the daemon is bound to a Unix domain socket or loopback-only address;
- insecure mode is enabled by an explicit flag;
- startup logs clearly warn that insecure mode is active;
- the mode cannot be used for fleet, remote, resident-node, or production operation.

---

## 4. What to build now, and what not to build yet

### Build primitives first

Prefer small, enforceable runtime primitives over broad product features.

Prioritize:

- local runtime correctness;
- state/trace/replay correctness;
- gateway and verifier correctness;
- typed messages;
- scoped delegation;
- daemon/API contracts;
- fleet identity and work orders;
- governance gates;
- physical/edge safety boundaries;
- compatibility tests.

### Do not build first

Do not build these before the primitive layer is stable:

- full enterprise admin SaaS;
- general marketplace;
- broad low-code app builder;
- unbounded browser computer;
- arbitrary shared mutable distributed memory;
- full consensus system;
- per-agent Docker image factory as the default;
- real-time robotics controller;
- hidden tool execution outside the gateway;
- chat-first architecture;
- broad integration surface without stable schemas.

---

## 5. Roadmap discipline

Implementation must follow the active sprint and milestone documents.

Current milestone shape:

```text
0.01-dev: local kernel baseline
0.02-dev: local multi-agent runtime + daemon control
0.03-dev: resident nodes + fleet execution foundation
0.04-dev: governance workflows
0.05-dev: physical/edge orchestration
0.1-dev: stable primitive spec + compatibility line
```

Do not pull later-milestone features into earlier sprints unless they are required to complete the sprint acceptance criteria.

A sprint is complete only when:

- all functional requirements for the sprint are implemented;
- verifiable criteria pass;
- documentation is updated;
- examples or fixtures demonstrate the behavior;
- regressions are covered;
- non-goals were not accidentally implemented;
- review checklist passes.

---

## 6. Implementation workflow

Use this workflow for every issue or pull request.

### Step 1 — Identify the primitive

Every change must name the primitive it strengthens:

```text
percept
policy
constraint
action gateway
verifier
adapter
quota
state graph
trace store
message
replay
work order
approval
fleet identity
node registry
runtime context
SDK/API
docs/tests
```

If the change does not strengthen a primitive, reconsider it.

### Step 2 — Identify the boundary

State whether the change affects:

```text
local runtime
Python SDK
Rust core
daemon API
TypeScript client
adapter boundary
state store
trace store
message router
fleet manager
governance layer
physical/edge layer
docs only
```

Boundary changes require tests and documentation.

### Step 3 — Write the smallest maintainable implementation

Prefer:

- explicit structs and typed schemas;
- deterministic state transitions;
- narrow traits;
- simple storage contracts;
- clear error types;
- integration tests over clever abstractions;
- feature flags for experimental surfaces;
- stable defaults with extension points.

Avoid:

- global mutable state;
- implicit permissions;
- hidden background work;
- magic transport behavior;
- broad trait objects without real use cases;
- premature plugin systems;
- accidental public APIs;
- complex distributed semantics before local semantics are stable.

### Step 4 — Prove behavior

Every non-trivial change must include one or more of:

- unit tests;
- integration tests;
- replay tests;
- trace assertion tests;
- state graph integrity tests;
- gateway denial tests;
- quota tests;
- permission tests;
- compatibility tests;
- example scenario.

### Step 5 — Update docs

A code change that changes behavior must update docs.

At minimum:

```text
/docs/reference/*     for schemas, APIs, event names, and contracts
/docs/guides/*        for implementation usage
/docs/rules/*         for sprint or invariant changes
/examples/*           for runnable behavior
CHANGELOG.md          for user-visible changes
```

---

## 7. Pull request contract

Every pull request must include this checklist in the description.

```md
## Primitive strengthened

- [ ] percept
- [ ] policy
- [ ] constraint
- [ ] action gateway
- [ ] verifier
- [ ] adapter
- [ ] quota
- [ ] state graph
- [ ] trace store
- [ ] message
- [ ] replay
- [ ] work order
- [ ] governance
- [ ] fleet/node identity
- [ ] SDK/API
- [ ] docs/tests

## Runtime invariants

- [ ] No side-effect bypass was introduced.
- [ ] Required verifiers fail closed.
- [ ] Trace events are emitted for meaningful transitions.
- [ ] State changes are explicit and versioned.
- [ ] Replay does not re-execute side effects by default.
- [ ] Tenant, agent, run, action, state, trace, and message IDs remain distinct.
- [ ] Shared agents/tools cannot launder permissions.
- [ ] Physical/device boundaries remain high-level and bounded, if relevant.

## Sprint discipline

- [ ] This belongs to the active milestone/sprint.
- [ ] Acceptance criteria are linked.
- [ ] Non-goals were respected.
- [ ] Tests prove the behavior.
- [ ] Docs/examples were updated.
- [ ] Public API/schema changes include versioning or RFC notes.
```

A PR that cannot complete this checklist should be split, redesigned, or moved behind an experimental feature flag.

Block a PR if it adds or documents daemon/client communication without authenticated caller identity, scoped authorization, and signed work-order rules.

---

## 8. Code quality rules

### Rust

Rust code should favor correctness, explicitness, and composable boundaries.

Required:

- typed IDs or newtypes for core identities when practical;
- structured errors;
- deterministic serialization for trace/state objects;
- tests for verifier/gateway/state/trace behavior;
- no panics in runtime paths except unrecoverable programmer errors;
- no unbounded retries for side-effectful actions;
- no silent fallback from deny to allow.

Preferred:

- small traits with one responsibility;
- clear module ownership;
- feature flags for experimental integrations;
- property tests for state/trace integrity where useful;
- integration tests for loop behavior.

### Python

Python SDK code should be ergonomic but never bypass enforcement.

Required:

- Python policy code proposes actions; it does not execute privileged side effects directly;
- SDK actions route through the gateway/daemon/runtime;
- examples show safe defaults;
- exceptions preserve enough context for trace/debugging;
- tests cover policy callbacks, percept submission, action proposals, and trace subscriptions.

### TypeScript

TypeScript is primarily for schemas, clients, and control-plane integration.

Required:

- generated or schema-aligned types;
- compatibility with daemon API contracts;
- no independent runtime semantics that conflict with Rust core;
- clear versioning for public packages.

---

## 9. Test expectations by subsystem

### Gateway and adapters

Must test:

- allowed action executes only after verification;
- denied action does not reach adapter execution;
- verifier failure fails closed;
- adapter failure creates traceable failure outcome;
- postcondition failure is recorded;
- quotas are consumed or rejected predictably;
- replay does not call adapter by default.

### State graph

Must test:

- state node creation;
- parent linkage;
- state hash/integrity field;
- state-head update;
- failed commit prevents next tick;
- replay can reconstruct state transitions.

### Trace store

Must test:

- append-only event behavior;
- event ordering within a run;
- required tick event coverage;
- trace linkage for actions, messages, approvals, and state commits;
- integrity chain if enabled;
- export/import behavior if implemented.

### Messaging

Must test:

- message schema validation;
- local delivery;
- target/source identity;
- causal parent linkage;
- delivery failure state;
- per-agent permissions;
- replay reconstruction of message causality.

### Work orders and fleet

Must test:

- unsigned work orders rejected;
- expired work orders rejected;
- incompatible work orders rejected;
- capabilities matched explicitly;
- node/instance identity preserved;
- trace continuity across dispatch/resume/migration.

### Governance

Must test:

- approval-required action pauses instead of executing;
- approval grant resumes correctly;
- denial cancels or blocks correctly;
- approval expiry is enforced;
- circuit breaker denies within scope;
- kill switch fails closed;
- audit/replay explains decisions.

### Physical/edge

Must test:

- low-level actuator actions are not accepted;
- safety verifier denial prevents adapter execution;
- offline policy cache honors TTL;
- high-risk action is denied or requires local intervention while offline;
- trace buffer syncs after reconnect;
- cloud helper cannot gain direct actuator authority by default.

---

## 10. Documentation expectations

Docs must keep the primitive surface stable and implementation-friendly.

When adding or changing a primitive, update:

```text
/docs/concepts/<primitive>.md
/docs/reference/<primitive>.md
/docs/guides/<how-to>.md
/examples/<scenario>/README.md
```

Reference docs must include:

- purpose;
- schema or trait/interface;
- lifecycle;
- trace events;
- failure modes;
- security notes;
- compatibility/versioning notes;
- minimal example.

Guide docs must include:

- a small runnable path;
- expected trace/state behavior;
- what is intentionally not allowed;
- debugging/replay instructions.

Docs must not describe aspirational behavior as implemented behavior.
Clearly mark future, planned, experimental, and stable surfaces.

---

## 11. Design review questions

Before implementing, answer these in the issue or PR:

1. Which Splendor primitive does this strengthen?
2. Which runtime invariant does this depend on?
3. Does this preserve the loop model?
4. Can any side effect bypass the gateway?
5. What trace events prove this happened?
6. What state is committed or referenced?
7. What happens when verification fails?
8. What happens when storage fails?
9. What happens during replay?
10. What is the tenant/agent/run boundary?
11. Is this local-only, daemon-facing, or distributed?
12. Does this require an RFC or schema version change?
13. What is the smallest test that proves the behavior?
14. What is explicitly out of scope?

If the answer is unclear, implement the smallest primitive-aligned version or write an RFC before coding.

---

## 12. Error and failure policy

Splendor should prefer safe, traceable failure over hidden continuation.

Required behavior:

- verifier unavailable -> deny, pause, or intervention;
- policy unavailable -> fail closed for side-effectful work;
- state commit fails -> do not advance to next tick;
- trace write fails -> fail closed for side-effectful actions;
- work order invalid -> reject before run starts;
- quota exceeded -> deny or pause;
- adapter error -> record outcome and trace failure;
- replay unsafe -> refuse or simulate without side effect;
- policy expired -> deny high-risk actions and request refresh/intervention.

Never convert an enforcement error into an implicit allow.

---

## 13. Compatibility and RFC rules

An RFC is required for:

- new primitive;
- renamed primitive;
- public schema change;
- trace event rename or semantic change;
- state graph format change;
- action gateway contract change;
- verifier pipeline change;
- daemon API breaking change;
- SDK breaking change;
- distributed identity change;
- governance semantics change;
- physical/device action model change.

An RFC must include:

- motivation;
- primitive affected;
- schema/API proposal;
- migration plan;
- compatibility impact;
- security impact;
- trace/replay impact;
- tests required;
- docs required.

---

## 14. Repository ownership guide

Expected ownership by area:

```text
crates/splendor-kernel/     runtime loop, scheduler, tenancy, runtime context
crates/splendor-gateway/    action gateway, verifier pipeline, action outcomes
crates/splendor-store/      state graph, trace store, persistence traits
crates/splendor-policy/     constraints, policy host contracts, rule evaluation hooks
crates/splendor-net/        messaging, node identity, transport, fleet protocol
python/splendor/            Python SDK, callbacks, local ergonomics
python/bindings/            Rust/Python boundary
typescript/                 generated types, client, control-plane packages
adapters/                   gated adapter implementations
examples/                   runnable scenarios that prove contracts
docs/                       concepts, references, guides, rules, RFCs
.github/                    CI, templates, checks
```

Keep runtime-critical enforcement in Rust core unless an RFC explicitly justifies otherwise.

---

## 15. Naming rules

Use precise names.

Preferred terms:

```text
tenant
agent
runtime context
run
tick
percept
policy
constraint
action request
action outcome
verifier
adapter
state node
trace event
message
work order
approval
fleet
node
instance
```

Avoid vague names for runtime primitives:

```text
job        unless it is explicitly a scheduled host/container job
task       unless scoped as a message payload or work-order objective
log        when the object is actually a trace event
memory     when the object is actually state graph data
plugin     when the object is an adapter, verifier, perceptor, or policy host
tool       when the object performs side effects through an adapter
agent pod  because one agent is not necessarily one pod/container
```

---

## 16. Security posture

Assume autonomous systems fail at boundaries.

Default stance:

- least privilege;
- scoped work orders;
- explicit data references;
- no broad inherited credentials;
- no silent network or filesystem access;
- no hidden shared mutable state;
- no execution without trace;
- no unsafe replay;
- no direct physical low-level control;
- deny on verifier uncertainty.

Security-sensitive changes must include tests for denial paths, not just successful paths.

---

## 17. Final implementation rule

When in doubt, choose the design that is:

```text
explicit over magical
traceable over convenient
fail-closed over permissive
local-correct before distributed
primitive-aligned over product-shaped
schema-stable over fast-changing
small and composable over broad and clever
```

Splendor must make autonomy auditable, governable, and safely executable without dictating the AI stack.
