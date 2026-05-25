# Splendor Sprint Verification and Per-Sprint Documentation Pack

**Date:** 2026-05-25  
**Audience:** implementation agents, review agents, architecture agents, SDK agents, runtime agents, cloud/edge orchestration agents, integration agents.  
**Purpose:** define verifiable completion criteria and required per-sprint documentation for the Splendor kernel roadmap.

This document is intentionally different from the milestone roadmap. The roadmap says **what** to build. This pack says **how a sprint proves it is complete**.

Splendor must remain an AI autonomy runtime kernel. Every sprint must strengthen one or more runtime primitives while preserving the governed loop:

```text
Percepts -> Policy -> Constraints -> Gateway -> Adapter -> Outcome -> State Commit -> Trace
```

The work should be demanding enough to produce real kernel primitives, but not so over-engineered that it creates a platform before the primitive is proven.

---

## 1. Implementation-agent operating rules

### 1.1 Bind every task to a sprint

Every issue, branch, and PR must declare:

```text
Milestone: Splendor0.xx-dev
Sprint: 0.xx-Sn or 0.01-Hn
Functional requirements touched: FR-...
Runtime primitives strengthened: ...
Non-goals: ...
```

A task that cannot name its sprint and primitive should not be implemented yet.

### 1.2 Prove behavior, not intention

A sprint is complete only when behavior can be verified through tests, examples, docs, and observable runtime outputs. A design note alone is not completion.

Required proof types:

```text
unit tests
contract/schema tests
integration tests
negative/fail-closed tests
trace/state/replay verification
example run or fixture
updated docs
```

### 1.3 Keep abstractions thin

Use abstractions only where they preserve future compatibility:

```text
schema boundary
adapter boundary
verifier boundary
policy boundary
runtime daemon boundary
transport boundary
storage boundary
```

Avoid abstraction layers that hide identity, state, trace, gateway, verifier, or quota behavior.

### 1.4 Prefer one reference path

Each sprint should normally produce one clean reference implementation, not a configurable framework with many interchangeable strategies. Extension points are allowed when they are clearly bounded and tested.

Correct:

```text
one local router implementation + a router trait that remote transport can implement later
```

Incorrect:

```text
five pluggable brokers before local message semantics are proven
```

### 1.5 Fail closed

If policy, verifier, work-order validation, approval, state commit, trace durability, identity validation, or capability validation cannot complete, the runtime must deny, pause, or request intervention. It must not silently allow side effects.

### 1.6 Preserve future integration

Design each sprint so later features can integrate without rewriting the primitive. This means:

```text
stable field names where possible
explicit version fields
clear identity scopes
trace-linked events
schema validation
no hidden mutable shared state
no ambient permissions
no side-effect bypass
```

---

## 2. Sprint completion contract

Every sprint must close with the following artifacts.

### 2.1 Implementation artifacts

```text
[ ] Code implementing only the sprint scope.
[ ] Tests for success, denial, failure, and replay/fail-closed behavior where relevant.
[ ] Trace/state verification for every runtime transition introduced.
[ ] Example or fixture showing the intended path.
[ ] Compatibility notes for changed schemas or public APIs.
```

### 2.2 Documentation artifacts

Each sprint must create or update:

```text
docs/milestones/<milestone>/<sprint-id>-<slug>.md
```

That file must contain:

```text
1. Objective
2. Functional scope
3. Non-goals
4. Public contracts changed
5. Runtime primitives touched
6. Trace events added or changed
7. State behavior added or changed
8. Verifier/gateway behavior added or changed
9. Replay behavior
10. Failure behavior
11. Test evidence
12. Example commands or fixtures
13. Future extension notes
```

### 2.3 PR checklist

Each PR must include:

```text
[ ] This PR belongs to one sprint.
[ ] It lists FR IDs or sprint criteria.
[ ] It has explicit non-goals.
[ ] It does not implement future milestone behavior accidentally.
[ ] It does not bypass the action gateway.
[ ] It emits/updates trace events for new runtime transitions.
[ ] It preserves or documents state graph behavior.
[ ] It defines replay behavior.
[ ] It includes negative/fail-closed tests.
[ ] It updates docs and examples.
```

---

## 3. Global verification standards

### 3.1 Test categories

Use these categories consistently.

| Category    | Required evidence                                                                                |
| ----------- | ------------------------------------------------------------------------------------------------ |
| Unit        | Core functions reject invalid inputs and handle expected state transitions.                      |
| Contract    | Schemas, public APIs, and SDK types match documented contracts.                                  |
| Integration | A realistic runtime flow works end-to-end.                                                       |
| Negative    | Invalid identity, policy, permission, quota, approval, trace, state, or capability fails closed. |
| Replay      | Replay reconstructs behavior without unsafe side effects.                                        |
| Trace       | Expected trace events are present, ordered, scoped, and identity-linked.                         |
| State       | State head, parents, hashes/references, and ownership are correct.                               |
| Docs        | A reviewer can follow the sprint docs without private context.                                   |

### 3.2 Required failure modes

Where applicable, every sprint must document and test:

```text
invalid identity
missing permission
quota exceeded
policy unavailable or expired
verifier unavailable
adapter failure
state commit failure
trace persistence failure
malformed schema
wrong scope
replay side-effect suppression
```

### 3.3 Maintainability threshold

A sprint should be considered over-engineered if it introduces:

```text
multiple interchangeable implementations before one reference path is proven
implicit authority inheritance
general distributed consensus
general workflow engine
product UI concerns
vendor-specific dependencies in core primitives
hidden mutable shared state
adapter-specific logic in the kernel core
```

A sprint should be considered under-built if it lacks:

```text
identity scope
trace linkage
state behavior
negative tests
replay behavior
documentation
clear non-goals
```

---

## 4. Sprint index

| Sprint  | Milestone        | Title                | Objective                                                                                                           |
| ------- | ---------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | --- | ------- | ---------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | --------------- | -------------------------------------------------------------------------------- | --- | ------- | ---------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --- | ------- | ---------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ------------------ | --------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------------- | --------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ------------------ | ----------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ------------ | ---------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ---------------- | ------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | --------------- | --------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ----------------- | --------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ----------------- | ------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ---------------- | -------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | --------------------------- | --------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | --- | ------- | ---------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------ | --- | ------- | ---------------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ------------------ | ----------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | --- | ------- | ---------------- | ------------------- | ---------------------------------------------------------------------------------------- | --- | ------- | ---------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | --- | ------- | ---------------- | --------------------- | ----------------------------------------------------------------------------------------------------------- | --- | ------ | --------------- | -------------------- | --------------------------------------------------------------------------------------------------------- | --- | ------ | --------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------- | --- | ------ | --------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------------- | --- | ------ | --------------- | ------------------------- | -------------------------------------------------------------------------------------------------------- | --- | ------ | --------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | --- | ------ | --------------- | --------------------- | ------------------------------------------------------------------------------------------------- |
| 0.01-H1 | Splendor0.01-dev | Baseline conformance | Make the implemented local kernel baseline internally consistent across README, CLI, SDK examples, tests, and docs. |     | 0.01-H2 | Splendor0.01-dev | Trace and replay hardening | Make trace and replay reliable enough to become the foundation for later messaging, governance, migration, and audit features. |     | 0.01-H3 | Splendor0.01-dev | Python SDK ergonomics | Make the Python SDK usable for real policy, perceptor, constraint, adapter, trace, and replay workflows without hiding kernel boundaries. |     | 0.01-H4 | Splendor0.01-dev | Release hygiene | Turn 0.01-dev into a clean baseline release line that future work can depend on. |     | 0.02-S1 | Splendor0.02-dev | Message schema contract | Define typed local message primitives without coupling them to a specific transport or future fleet implementation. |     | 0.02-S2 | Splendor0.02-dev | Local message router | Route typed messages between agent runtime contexts inside a single Splendor instance with traceable delivery states. |     | 0.02-S3 | Splendor0.02-dev | Agent isolation ledger | Ensure multiple agents in one instance remain separately accountable for permissions, quotas, messages, state heads, and trace streams. |     | 0.02-S4 | Splendor0.02-dev | Local delegation model | Allow an orchestrator agent to delegate scoped local work to named agents while preserving causal trace, parent/child run references, and limited authority. |     | 0.02-S5 | Splendor0.02-dev | Runtime daemon API | Expose a minimal local control API for runs, percepts, state, traces, replay, actions, health, and capabilities. |     | 0.02-S6 | Splendor0.02-dev | TypeScript surface | Provide TypeScript types and a thin daemon client without making TypeScript the core runtime. |     | 0.02-S7 | Splendor0.02-dev | Multi-agent replay and test harness | Prove local multi-agent behavior is replayable, inspectable, and testable before distributed messaging begins. |     | 0.03-S1 | Splendor0.03-dev | Distributed identity model | Define stable identities for fleet, node, instance, tenant, agent, run, tick, action, state, trace, and message without overloading concepts. |     | 0.03-S2 | Splendor0.03-dev | Node and instance registry | Allow resident Splendor nodes and instances to register, report capabilities, heartbeat, and expose runtime metadata. |     | 0.03-S3 | Splendor0.03-dev | Signed work orders | Make work orders the explicit authority object for starting distributed or resident runs. |     | 0.03-S4 | Splendor0.03-dev | Placement v0 | Select execution targets using declared capabilities and constraints without building a full scheduler platform. |     | 0.03-S5 | Splendor0.03-dev | Remote message transport | Carry typed messages between Splendor instances while preserving local message semantics, identity, causal trace, and failure visibility. |     | 0.03-S6 | Splendor0.03-dev | Trace aggregation | Sync local traces from resident or remote instances to a central index without weakening append-only semantics or trace integrity. |     | 0.03-S7 | Splendor0.03-dev | State handoff v0 | Move or share state explicitly through snapshots and references, not hidden mutable distributed memory. |     | 0.03-S8 | Splendor0.03-dev | Fleet telemetry | Aggregate fleet health, run status, quotas, trace sync state, and failure signals into a minimal operational surface. |     | 0.04-S1 | Splendor0.04-dev | Governance state model | Define approval, escalation, intervention, circuit-breaker, and kill-switch states as first-class runtime/governance objects. |     | 0.04-S2 | Splendor0.04-dev | Approval verifier | Require, pause for, validate, and apply approvals for scoped actions and runs through the verifier chain. |     | 0.04-S3 | Splendor0.04-dev | Escalation engine | Escalate uncertain, repeated, timed-out, or risky runtime situations through a small, explicit policy engine. |     | 0.04-S4 | Splendor0.04-dev | Circuit breakers | Stop unsafe or unhealthy execution at tenant, agent, adapter, action, node, instance, fleet, or global scopes. |     | 0.04-S5 | Splendor0.04-dev | Central policy distribution | Distribute, cache, validate, expire, and revoke policy bundles without requiring central connectivity for every local tick. |     | 0.04-S6 | Splendor0.04-dev | External governance adapter | Let Harmony or another control plane issue scoped work orders, approvals, and governance signals without making Splendor product-specific. |     | 0.04-S7 | Splendor0.04-dev | Governance replay and audit | Explain and export why governed actions were allowed, denied, paused, escalated, resumed, or circuit-broken. |     | 0.05-S1 | Splendor0.05-dev | Device profile schema | Represent physical and edge nodes as explicit capability-bearing runtime targets without making Splendor a device driver or real-time controller. |     | 0.05-S2 | Splendor0.05-dev | Offline policy cache | Allow resident physical/edge instances to operate within cached policy during disconnection while failing closed for high-risk actions. |     | 0.05-S3 | Splendor0.05-dev | Local trace buffer | Preserve trace continuity for disconnected physical/edge nodes and sync safely after reconnect. |     | 0.05-S4 | Splendor0.05-dev | Robotics adapter interface | Define a safe high-level adapter contract for physical actions mediated by local device middleware and real-time controllers. |     | 0.05-S5 | Splendor0.05-dev | Safety verifier API | Evaluate local physical safety constraints before and after high-level physical actions. |     | 0.05-S6 | Splendor0.05-dev | Cloud-helper pattern | Allow physical devices to use cloud/on-prem helper Splendor instances for planning or analysis without granting direct actuator authority. |     | 0.05-S7 | Splendor0.05-dev | Physical demo harness | Provide a runnable simulation that proves the physical/edge primitives work together without live hardware. |     | 0.1-S1 | Splendor0.1-dev | Stable schema freeze | Freeze the first stable primitive schema set for implementers while preserving explicit extension points. |     | 0.1-S2 | Splendor0.1-dev | Compatibility test suite | Create a conformance suite that third-party clients, adapters, and policy hosts can run against the stable primitives. |     | 0.1-S3 | Splendor0.1-dev | Adapter maturity model | Classify adapters by safety, governance, and operational maturity so future ecosystem growth remains controlled. |     | 0.1-S4 | Splendor0.1-dev | SDK and API stabilization | Stabilize Rust traits, Python SDK, daemon API, and TypeScript client around the 0.1 primitive contracts. |     | 0.1-S5 | Splendor0.1-dev | Operational documentation | Publish operational guides that show how to run Splendor in local, daemon, resident, fleet, governance, Harmony, and physical/edge modes using stable primitives. |     | 0.1-S6 | Splendor0.1-dev | Migration and release | Release 0.1 with explicit migration rules from dev milestones and a clear compatibility contract. |

---

## 5. Per-sprint verification and documentation requirements

### 0.01-H1 — Baseline conformance

**Milestone:** Splendor0.01-dev  
**Objective:** Make the implemented local kernel baseline internally consistent across README, CLI, SDK examples, tests, and docs.

**Bounded scope**

- Verify that documented behavior matches the implemented local runtime.
- Align runtime terms: tenant, agent, run, tick, action, state node, trace event, verifier, adapter, replay.
- Remove or clearly mark any future-looking behavior that is not implemented in 0.01-dev.

**Functional outputs**

- A 0.01 conformance matrix mapping implemented features to docs and tests.
- Updated README and docs for the local runtime baseline.
- A runnable local example that demonstrates percept -> policy -> gateway -> adapter -> outcome -> state -> trace.

**Verifiable acceptance criteria**

- [ ] A reviewer can run the documented quickstart exactly as written and produce a completed local run.
- [ ] The example emits trace events for run start, tick start, policy invocation, verification, action outcome, state commit, and tick completion.
- [ ] The state head returned by CLI or SDK matches the final state node referenced by the trace.
- [ ] Every documented 0.01 command has a smoke test or equivalent scripted verification.
- [ ] Every future milestone feature mentioned in 0.01 docs is explicitly labeled as planned, not available.
- [ ] The conformance matrix has no undocumented implemented feature and no documented missing feature.

**Required sprint documentation**

- `docs/milestones/0.01-dev/conformance.md`
- `docs/getting-started/local-runtime.md`
- `docs/reference/runtime-loop.md`
- `examples/local-basic-loop/README.md`

**Integration constraints**

- Do not change runtime behavior solely to match old docs. Fix stale docs first, then create issues for runtime gaps.
- Keep all 0.01 public names stable unless a mismatch blocks basic correctness.

**Explicit non-goals**

- No new fleet APIs.
- No local multi-agent router.
- No TypeScript package implementation.
- No governance workflow engine.

---

### 0.01-H2 — Trace and replay hardening

**Milestone:** Splendor0.01-dev  
**Objective:** Make trace and replay reliable enough to become the foundation for later messaging, governance, migration, and audit features.

**Bounded scope**

- Define required local trace event order for a tick.
- Verify append-only trace behavior and integrity chaining where implemented.
- Define replay side-effect suppression as a hard contract.

**Functional outputs**

- Trace event order contract.
- Replay contract for read-only actions, denied actions, failed actions, and side-effectful actions.
- Tests that prove replay never repeats unsafe effects by default.

**Verifiable acceptance criteria**

- [ ] A completed tick has a deterministic minimum trace sequence and the sequence is tested.
- [ ] Trace event IDs are unique within a run and refer to the correct run, agent, tick, and action where applicable.
- [ ] Replay reconstructs state transitions from persisted trace/state data without requiring live policy execution unless explicitly configured.
- [ ] Side-effectful actions are not re-executed during replay unless a named safe simulation mode is used.
- [ ] A corrupted or missing trace segment causes replay to fail with a clear error rather than silently continuing.
- [ ] A failed state commit prevents the next tick from starting in tests.

**Required sprint documentation**

- `docs/reference/trace-events.md`
- `docs/reference/replay.md`
- `docs/milestones/0.01-dev/trace-replay-hardening.md`
- `examples/replay-local-run/README.md`

**Integration constraints**

- Expose trace/replay behavior through stable interfaces that later milestones can extend with messages, approvals, migration, and remote sync.
- Prefer small event primitives over broad audit-log abstractions.

**Explicit non-goals**

- No distributed trace aggregation.
- No governance audit export.
- No cross-instance replay.

---

### 0.01-H3 — Python SDK ergonomics

**Milestone:** Splendor0.01-dev  
**Objective:** Make the Python SDK usable for real policy, perceptor, constraint, adapter, trace, and replay workflows without hiding kernel boundaries.

**Bounded scope**

- Polish the Python SDK API around existing 0.01 functionality.
- Add concise examples for extension points.
- Ensure Python code cannot bypass the gateway in official examples.

**Functional outputs**

- SDK examples for policy, perceptors, constraints/verifiers, adapters, trace subscription, and replay.
- A minimal API reference for Python users.
- Contract tests that lock expected SDK behavior.

**Verifiable acceptance criteria**

- [ ] A new developer can implement a policy and run it locally using only the SDK docs.
- [ ] Every SDK adapter example routes side effects through the action gateway.
- [ ] SDK examples include at least one allowed action, one denied action, and one failed action.
- [ ] Trace subscription examples show event classes and IDs, not opaque logs.
- [ ] SDK replay example documents and demonstrates side-effect suppression.
- [ ] SDK tests cover error propagation from verifier denial, adapter failure, and state commit failure.

**Required sprint documentation**

- `docs/sdk/python/index.md`
- `docs/sdk/python/policies.md`
- `docs/sdk/python/adapters.md`
- `docs/sdk/python/replay.md`
- `examples/python-sdk-basic/README.md`

**Integration constraints**

- Keep SDK objects close to the canonical runtime primitives. Avoid Python-only concepts that cannot map to Rust/TypeScript schemas later.
- Do not use Python convenience wrappers to obscure identity, trace, state, or gateway boundaries.

**Explicit non-goals**

- No distributed SDK client.
- No Python control-plane scheduler.
- No plugin marketplace pattern.

---

### 0.01-H4 — Release hygiene

**Milestone:** Splendor0.01-dev  
**Objective:** Turn 0.01-dev into a clean baseline release line that future work can depend on.

**Bounded scope**

- Tag release artifacts and document compatibility expectations.
- Publish a minimal changelog and known limitations.
- Define the test matrix that must pass before future milestone branches merge.

**Functional outputs**

- 0.01-dev release notes.
- Known limitations page.
- Compatibility and migration notes for 0.01 to 0.02.
- CI release checklist.

**Verifiable acceptance criteria**

- [ ] Release notes list implemented primitives and explicitly exclude future milestone behavior.
- [ ] Known limitations include local-only constraints, no fleet registry, no governance engine, and no remote messaging.
- [ ] CI has a named job or script for baseline local runtime conformance.
- [ ] A clean checkout can run the 0.01 smoke test without undocumented services.
- [ ] Version identifiers are visible from CLI and SDK.
- [ ] Breaking changes after this sprint require a migration note.

**Required sprint documentation**

- `CHANGELOG.md`
- `docs/releases/0.01-dev.md`
- `docs/releases/known-limitations.md`
- `docs/development/ci-release-checklist.md`

**Integration constraints**

- Future branches must use this baseline as the reference for local runtime behavior.
- Do not freeze schemas prematurely; document what is provisional.

**Explicit non-goals**

- No stable 0.1 compatibility guarantee.
- No adapter certification program.
- No package ecosystem maturity claim.

---

### 0.02-S1 — Message schema contract

**Milestone:** Splendor0.02-dev  
**Objective:** Define typed local message primitives without coupling them to a specific transport or future fleet implementation.

**Bounded scope**

- Add canonical message schema for local agent-to-agent communication.
- Include causal linkage to trace events.
- Add schema versioning and validation rules.

**Functional outputs**

- Canonical Message and MessageEnvelope schemas.
- Validation errors for missing identity, schema, payload, run, or causal parent where required.
- Trace event definitions for message queued, delivered, rejected, expired, and consumed.

**Verifiable acceptance criteria**

- [ ] A message cannot be created without source agent, target agent, run ID, message ID, schema, payload, and timestamp.
- [ ] Invalid schema versions are rejected before routing.
- [ ] Message payload validation failure emits a rejection trace event.
- [ ] Message causal parent can reference a trace event and is preserved during replay.
- [ ] The schema is transport-neutral and does not mention HTTP, NATS, gRPC, or fleet-specific routing.
- [ ] Generated or mirrored types are consistent across Rust and documentation.

**Required sprint documentation**

- `docs/reference/messages.md`
- `docs/reference/trace-events.md#message-events`
- `docs/milestones/0.02-dev/S1-message-schema-contract.md`
- `adr/0001-message-schema-boundary.md`

**Integration constraints**

- Design message primitives so remote transport can wrap them in 0.03 without rewriting local semantics.
- Keep schema strict at the envelope layer and flexible only inside typed payloads.

**Explicit non-goals**

- No message broker.
- No remote transport.
- No distributed delivery guarantee.
- No shared mutable state channel.

---

### 0.02-S2 — Local message router

**Milestone:** Splendor0.02-dev  
**Objective:** Route typed messages between agent runtime contexts inside a single Splendor instance with traceable delivery states.

**Bounded scope**

- Implement local inbox/outbox.
- Implement routing and delivery state transitions.
- Add failure behavior for unknown agents, unauthorized messages, invalid schemas, expired messages, and full queues.

**Functional outputs**

- In-process message router.
- Per-agent inbox and outbox APIs.
- Trace events for queue, delivery, rejection, consumption, and expiration.

**Verifiable acceptance criteria**

- [ ] A message from agent A to agent B is visible only to B and trace-linked to the originating run/tick.
- [ ] Unknown target agent causes deterministic rejection and trace emission.
- [ ] Router denial does not call policy or adapter execution for the target agent.
- [ ] Message ordering is deterministic within one source-target-run stream.
- [ ] Inbox read operations do not mutate unrelated agent state.
- [ ] Router tests cover success, target missing, invalid schema, quota exceeded, and expired message.

**Required sprint documentation**

- `docs/reference/local-message-router.md`
- `docs/milestones/0.02-dev/S2-local-message-router.md`
- `examples/local-multi-agent-router/README.md`

**Integration constraints**

- Expose a router interface that future remote transport can implement without changing agent runtime context APIs.
- Avoid broker-specific semantics until 0.03 chooses remote transport boundaries.

**Explicit non-goals**

- No cross-instance messaging.
- No durable remote queue.
- No exactly-once distributed semantics.

---

### 0.02-S3 — Agent isolation ledger

**Milestone:** Splendor0.02-dev  
**Objective:** Ensure multiple agents in one instance remain separately accountable for permissions, quotas, messages, state heads, and trace streams.

**Bounded scope**

- Add per-agent quota and permission ledger.
- Enforce allowed message schemas and recipients.
- Prevent shared specialists from inheriting caller permissions by default.

**Functional outputs**

- Agent isolation ledger.
- Per-agent quota counters.
- Permission checks for actions and messages.
- Negative tests for permission laundering.

**Verifiable acceptance criteria**

- [ ] Agent A cannot execute an action using Agent B permissions.
- [ ] Agent A cannot send a disallowed message schema even if both agents share a tenant.
- [ ] Shared specialist receives only explicitly delegated permissions.
- [ ] Quota exhaustion for one agent does not exhaust or reset another agent ledger.
- [ ] Every denial emits trace data with agent ID, reason, and verifier/ledger source.
- [ ] Replay shows the same isolation decisions from persisted trace/state data.

**Required sprint documentation**

- `docs/reference/agent-isolation.md`
- `docs/reference/quotas.md`
- `docs/milestones/0.02-dev/S3-agent-isolation-ledger.md`
- `examples/local-specialist-scoped-delegation/README.md`

**Integration constraints**

- Keep isolation scoped to agent runtime context so tenant, run, and future fleet scopes can compose cleanly.
- Do not collapse tenant and agent authority into one permission object.

**Explicit non-goals**

- No operating-system sandboxing implementation.
- No microVM orchestration.
- No cross-tenant scheduler.

---

### 0.02-S4 — Local delegation model

**Milestone:** Splendor0.02-dev  
**Objective:** Allow an orchestrator agent to delegate scoped local work to named agents while preserving causal trace, parent/child run references, and limited authority.

**Bounded scope**

- Define local task request/response message schemas.
- Add parent/child run references for local delegation.
- Validate scoped delegated permissions.

**Functional outputs**

- TaskRequest and TaskResponse schemas.
- Parent/child run metadata.
- Delegated permission object.
- Example orchestrator plus specialist agents.

**Verifiable acceptance criteria**

- [ ] A parent run can create a child run only with explicit target agent and scoped objective.
- [ ] Child run cannot access parent permissions unless delegated in the request.
- [ ] Child run trace references parent causal event and parent run references child completion.
- [ ] Failed child run propagates a structured failure outcome, not an untyped exception.
- [ ] Cancellation of parent run prevents new child delegation and records trace events.
- [ ] Replay reconstructs parent/child causal relationships and message exchange.

**Required sprint documentation**

- `docs/reference/local-delegation.md`
- `docs/reference/runs.md#parent-child-runs`
- `docs/milestones/0.02-dev/S4-local-delegation-model.md`
- `examples/local-orchestrator-specialists/README.md`

**Integration constraints**

- Model local delegation as the same conceptual primitive future cross-instance work orders will use, but keep it local-only for this sprint.
- Keep delegation explicit. Do not introduce ambient inherited authority.

**Explicit non-goals**

- No remote work-order dispatch.
- No fleet placement.
- No long-lived autonomous child services.

---

### 0.02-S5 — Runtime daemon API

**Milestone:** Splendor0.02-dev  
**Objective:** Expose a minimal local control API for runs, percepts, state, traces, replay, actions, health, and capabilities.

**Bounded scope**

- Define daemon API endpoints and request/response schemas.
- Implement local-only server boundary.
- Make API behavior consistent with CLI and SDK.

**Functional outputs**

- Runtime daemon API.
- OpenAPI or schema reference.
- Health/capability endpoints.
- Integration tests for local daemon workflow.

**Verifiable acceptance criteria**

- [ ] A client can create, start, pause, resume, stop, and inspect a local run through the daemon.
- [ ] Appending a percept through the daemon produces the same runtime behavior as SDK/CLI percept ingestion.
- [ ] State-head endpoint returns a state node that exists in the state graph.
- [ ] Trace endpoint streams or pages trace events without losing event order.
- [ ] Replay endpoint starts a replay without re-executing side-effectful actions.
- [ ] Invalid run, unauthorized action, malformed percept, and unavailable runtime return structured errors.

**Required sprint documentation**

- `docs/reference/runtime-daemon-api.md`
- `docs/milestones/0.02-dev/S5-runtime-daemon-api.md`
- `examples/daemon-client-local/README.md`
- `openapi/splendor-runtime-daemon.yaml`

**Integration constraints**

- This API is the preferred future boundary for TypeScript and external control-plane clients.
- Keep daemon local/foundation-oriented. Do not bake in fleet manager assumptions.

**Explicit non-goals**

- No remote node registry.
- No production auth system beyond local/development controls.
- No fleet scheduling.

---

### 0.02-S6 — TypeScript surface

**Milestone:** Splendor0.02-dev  
**Objective:** Provide TypeScript types and a thin daemon client without making TypeScript the core runtime.

**Bounded scope**

- Publish @splendor/types for canonical schemas.
- Publish @splendor/client for the runtime daemon.
- Add compatibility tests against daemon schema.

**Functional outputs**

- @splendor/types package.
- @splendor/client package.
- Generated or validated schema parity tests.
- Minimal TypeScript example.

**Verifiable acceptance criteria**

- [ ] TypeScript Message, RunConfig, Percept, ActionRequest, ActionOutcome, TraceEvent, and StateHead types match canonical schemas.
- [ ] The client can create a run, append a percept, stream/read traces, query state head, and request replay.
- [ ] Client errors preserve daemon error codes and useful structured details.
- [ ] No TypeScript package executes kernel logic that should remain in Rust runtime.
- [ ] Package tests run without external fleet infrastructure.
- [ ] Version compatibility with daemon API is documented.

**Required sprint documentation**

- `docs/sdk/typescript/index.md`
- `docs/sdk/typescript/client.md`
- `docs/milestones/0.02-dev/S6-typescript-surface.md`
- `examples/typescript-daemon-client/README.md`

**Integration constraints**

- Keep TypeScript as a control-plane/integration surface. Do not fork runtime semantics into the client.
- Prefer schema generation or strict parity tests over hand-maintained drift.

**Explicit non-goals**

- No native Node runtime binding.
- No browser runtime.
- No Harmony adapter implementation unless required as a thin example.

---

### 0.02-S7 — Multi-agent replay and test harness

**Milestone:** Splendor0.02-dev  
**Objective:** Prove local multi-agent behavior is replayable, inspectable, and testable before distributed messaging begins.

**Bounded scope**

- Extend replay to include messages, child runs, and per-agent isolation decisions.
- Provide a deterministic local multi-agent test harness.
- Add inspectable causal graph output.

**Functional outputs**

- Multi-agent replay support.
- Causal graph inspection tool or output format.
- Reusable test harness for orchestrator/specialist patterns.

**Verifiable acceptance criteria**

- [ ] Replay reconstructs message queued, delivered, consumed, rejected, and expired events.
- [ ] Replay shows parent/child run relationships without executing child side effects.
- [ ] Permission-laundering denial appears in replay with verifier/ledger reason.
- [ ] Causal graph includes trace event IDs, message IDs, source/target agents, and run IDs.
- [ ] The harness can run one positive multi-agent scenario and at least three denial/failure scenarios.
- [ ] Test output is deterministic across repeated runs with the same inputs.

**Required sprint documentation**

- `docs/reference/multi-agent-replay.md`
- `docs/milestones/0.02-dev/S7-multi-agent-replay-test-harness.md`
- `examples/local-multi-agent-replay/README.md`

**Integration constraints**

- Use this harness as a baseline for 0.03 remote messaging conformance.
- Do not add remote transport mocks that hide missing 0.03 work.

**Explicit non-goals**

- No cross-instance replay.
- No remote transport.
- No distributed trace sync.

---

### 0.03-S1 — Distributed identity model

**Milestone:** Splendor0.03-dev  
**Objective:** Define stable identities for fleet, node, instance, tenant, agent, run, tick, action, state, trace, and message without overloading concepts.

**Bounded scope**

- Formalize ID types and validation.
- Define identity relationships.
- Add serialization and trace embedding rules.

**Functional outputs**

- Distributed identity reference.
- Validation library or module.
- Schema updates for work orders, node registration, messages, state, and trace.

**Verifiable acceptance criteria**

- [ ] Each ID type is distinct in schema and documentation.
- [ ] No runtime path uses one ID to represent multiple concepts.
- [ ] Trace events include enough identity fields to locate fleet, node, instance, run, agent, tick, and action when applicable.
- [ ] Invalid, missing, or mismatched identities fail before execution.
- [ ] Identity serialization is stable across Rust, Python, and TypeScript surfaces that need it.
- [ ] Migration notes explain any changes from 0.02 IDs.

**Required sprint documentation**

- `docs/reference/identity.md`
- `docs/milestones/0.03-dev/S1-distributed-identity-model.md`
- `adr/0002-distributed-identity-boundaries.md`

**Integration constraints**

- Make identity types portable across fleet, governance, and physical/edge milestones.
- Do not introduce a distributed consensus or global registry dependency in this sprint.

**Explicit non-goals**

- No node registry implementation.
- No remote messaging.
- No placement engine.

---

### 0.03-S2 — Node and instance registry

**Milestone:** Splendor0.03-dev  
**Objective:** Allow resident Splendor nodes and instances to register, report capabilities, heartbeat, and expose runtime metadata.

**Bounded scope**

- Implement registry data model.
- Implement registration and heartbeat APIs.
- Add capability and version reporting.

**Functional outputs**

- Node registration API.
- Instance registration API.
- Heartbeat endpoint.
- Capability document format.
- Registry tests.

**Verifiable acceptance criteria**

- [ ] A node can register with node ID, kind, tenant/fleet scope, capabilities, constraints, runtime version, and health metadata.
- [ ] An instance can register under a node with instance ID, runtime mode, hosted tenants, and supported features.
- [ ] Heartbeat updates health without overwriting static registration fields accidentally.
- [ ] Stale heartbeat detection is deterministic and documented.
- [ ] Invalid capability documents are rejected before registration.
- [ ] Registry changes emit trace or management audit events suitable for later aggregation.

**Required sprint documentation**

- `docs/reference/node-registry.md`
- `docs/reference/capabilities.md`
- `docs/milestones/0.03-dev/S2-node-instance-registry.md`
- `examples/resident-node-registration/README.md`

**Integration constraints**

- Keep registry minimal: discover and describe nodes, do not schedule complex workloads yet.
- Capability model must remain extensible for physical devices in 0.05.

**Explicit non-goals**

- No advanced placement policy.
- No upgrade orchestration.
- No device safety verifier implementation.

---

### 0.03-S3 — Signed work orders

**Milestone:** Splendor0.03-dev  
**Objective:** Make work orders the explicit authority object for starting distributed or resident runs.

**Bounded scope**

- Define work-order schema and signature envelope.
- Validate signatures, expiry, revocation marker, target compatibility, allowed actions/adapters/permissions, quotas, and placement hints.
- Reject invalid work orders before runtime execution.

**Functional outputs**

- WorkOrder schema.
- Signature validation path.
- Work-order ingestion API.
- Negative tests for unsigned, expired, revoked, malformed, and incompatible work orders.

**Verifiable acceptance criteria**

- [ ] Unsigned work orders are rejected with no run created.
- [ ] Expired work orders cannot start new runs or authorize side effects.
- [ ] Revoked work orders are rejected before percept, policy, or adapter execution.
- [ ] Allowed actions/adapters/permissions become runtime constraints for the run.
- [ ] Work-order ID appears in run metadata and trace events.
- [ ] Signature validation failure emits a clear management/audit trace without leaking secrets.

**Required sprint documentation**

- `docs/reference/work-orders.md`
- `docs/security/signed-work-orders.md`
- `docs/milestones/0.03-dev/S3-signed-work-orders.md`
- `examples/signed-work-order-local-resident/README.md`

**Integration constraints**

- Work orders are the bridge from external managers to Splendor runtime; keep them scoped and portable.
- Do not add broad user credentials or ambient authority.

**Explicit non-goals**

- No full PKI product.
- No enterprise identity provider integration.
- No approval workflow engine.

---

### 0.03-S4 — Placement v0

**Milestone:** Splendor0.03-dev  
**Objective:** Select execution targets using declared capabilities and constraints without building a full scheduler platform.

**Bounded scope**

- Define placement target types and decision model.
- Implement simple capability/locality matching.
- Return clear rejection reasons.

**Functional outputs**

- PlacementDecision type.
- Capability matcher.
- Placement explain output.
- Tests for cloud, VPC, on-prem, edge, physical, and desktop target classes.

**Verifiable acceptance criteria**

- [ ] Placement decision includes target, reasons, dedicated-instance flag, required capabilities, and data locality when relevant.
- [ ] A work order requiring unavailable capabilities is rejected or left pending with explicit reason.
- [ ] A physical target request cannot be placed on a generic cloud node unless explicitly marked as simulation/helper.
- [ ] Data-locality hints are preserved in the decision and trace/audit output.
- [ ] Placement tests cover success, no matching node, incompatible runtime, missing capability, and dedicated-instance requirement.
- [ ] No placement decision silently widens permissions.

**Required sprint documentation**

- `docs/reference/placement.md`
- `docs/milestones/0.03-dev/S4-placement-v0.md`
- `examples/placement-basic/README.md`

**Integration constraints**

- Placement v0 should be deterministic and explainable, not clever. Later schedulers can replace the strategy behind the same decision contract.
- Keep placement separate from work-order authority validation.

**Explicit non-goals**

- No autoscaling.
- No multi-region optimizer.
- No cost optimizer.
- No Kubernetes operator.

---

### 0.03-S5 — Remote message transport

**Milestone:** Splendor0.03-dev  
**Objective:** Carry typed messages between Splendor instances while preserving local message semantics, identity, causal trace, and failure visibility.

**Bounded scope**

- Define remote transport envelope.
- Bridge local router to remote instance boundary.
- Add delivery failure, retry metadata, and idempotency marker rules.

**Functional outputs**

- RemoteMessageEnvelope schema.
- Transport adapter interface.
- Cross-instance message tests.
- Trace events for remote send, accept, reject, deliver, timeout, and duplicate.

**Verifiable acceptance criteria**

- [ ] A local message can be wrapped for remote delivery without changing the canonical Message payload.
- [ ] Remote receiver validates identity, schema, run/work-order authority, and target agent before delivery.
- [ ] Duplicate remote messages are detected by message ID and handled deterministically.
- [ ] Transport failure records a trace/audit event and does not silently drop runtime state.
- [ ] Retry occurs only when configured and safe for the message semantics.
- [ ] Remote message replay shows send and receive sides with causal linkage.

**Required sprint documentation**

- `docs/reference/remote-messaging.md`
- `docs/milestones/0.03-dev/S5-remote-message-transport.md`
- `examples/two-instance-message/README.md`

**Integration constraints**

- Transport should be pluggable, but only one reference implementation is needed. Avoid abstracting for every broker up front.
- Maintain same local message contract from 0.02.

**Explicit non-goals**

- No distributed consensus.
- No exactly-once global guarantee.
- No arbitrary remote state mutation.

---

### 0.03-S6 — Trace aggregation

**Milestone:** Splendor0.03-dev  
**Objective:** Sync local traces from resident or remote instances to a central index without weakening append-only semantics or trace integrity.

**Bounded scope**

- Implement local trace buffer sync.
- Implement central trace aggregation/indexing.
- Validate ordering and hash/integrity information where available.

**Functional outputs**

- Trace sync protocol.
- Central trace index.
- Conflict/corruption handling.
- Tests for partial sync, reconnect, duplicate sync, and corrupted trace segment.

**Verifiable acceptance criteria**

- [ ] A local instance can buffer trace events and sync them later without reordering events inside a run.
- [ ] Duplicate sync attempts do not create duplicate central trace events.
- [ ] Missing segments are detected and reported clearly.
- [ ] Corrupted trace chain or mismatched run identity causes sync rejection or quarantine.
- [ ] Central index can query by fleet, node, instance, tenant, agent, run, tick, action, and work order where available.
- [ ] Trace sync failure does not permit side-effectful actions if local policy requires trace durability.

**Required sprint documentation**

- `docs/reference/trace-sync.md`
- `docs/reference/central-trace-index.md`
- `docs/milestones/0.03-dev/S6-trace-aggregation.md`
- `examples/resident-trace-sync/README.md`

**Integration constraints**

- Use the same trace event contract from 0.01 and 0.02; aggregation must not invent incompatible audit records.
- Prepare for governance audit export without implementing governance workflows here.

**Explicit non-goals**

- No analytics dashboard.
- No long-term data warehouse design.
- No governance audit product.

---

### 0.03-S7 — State handoff v0

**Milestone:** Splendor0.03-dev  
**Objective:** Move or share state explicitly through snapshots and references, not hidden mutable distributed memory.

**Bounded scope**

- Define state snapshot export/import.
- Define read-only state reference.
- Validate state head, parent linkage, hash, run/agent ownership, and trace continuity.

**Functional outputs**

- StateHandoff schema.
- Snapshot export/import path.
- Read-only state reference mode.
- Tests for valid handoff, mismatched owner, stale head, corrupted snapshot, and missing trace.

**Verifiable acceptance criteria**

- [ ] A receiving instance cannot resume from a snapshot unless agent/run/work-order authority is valid.
- [ ] State snapshot hash is verified before import.
- [ ] Read-only state references cannot be mutated by the receiver.
- [ ] State handoff creates trace events on source and receiver sides.
- [ ] A failed import leaves the receiver state unchanged.
- [ ] Replay can identify the handoff boundary and previous state head.

**Required sprint documentation**

- `docs/reference/state-handoff.md`
- `docs/reference/state-graph.md`
- `docs/milestones/0.03-dev/S7-state-handoff-v0.md`
- `examples/state-handoff-basic/README.md`

**Integration constraints**

- This is not shared memory. Keep ownership explicit and narrow so future migration and fork/merge can be added safely.
- Do not implement conflict merge unless it is deterministic and scoped; otherwise defer.

**Explicit non-goals**

- No distributed mutable state.
- No CRDT system.
- No automatic conflict resolution.
- No full runtime migration engine.

---

### 0.03-S8 — Fleet telemetry

**Milestone:** Splendor0.03-dev  
**Objective:** Aggregate fleet health, run status, quotas, trace sync state, and failure signals into a minimal operational surface.

**Bounded scope**

- Define telemetry event/metric model.
- Collect node, instance, run, queue, quota, and sync status.
- Add failure category taxonomy.

**Functional outputs**

- Fleet telemetry model.
- Telemetry ingestion endpoint or collector.
- Status query examples.
- Failure category reference.

**Verifiable acceptance criteria**

- [ ] Telemetry reports node online/stale/offline state from heartbeat data.
- [ ] Telemetry reports instance runtime version, mode, capabilities, and current run counts.
- [ ] Run status uses the canonical states: pending, running, paused, waiting_for_approval, interrupted, resuming, completed, failed, cancelled, denied, expired.
- [ ] Quota and denial signals identify tenant, agent, run, and verifier where applicable.
- [ ] Trace sync lag or failure is visible per node/instance.
- [ ] Telemetry cannot be used as a hidden authority source for runtime permissions.

**Required sprint documentation**

- `docs/reference/fleet-telemetry.md`
- `docs/milestones/0.03-dev/S8-fleet-telemetry.md`
- `examples/fleet-telemetry-basic/README.md`

**Integration constraints**

- Keep telemetry operational and minimal. Later dashboards can consume it without changing runtime semantics.
- Do not couple telemetry to a specific observability vendor.

**Explicit non-goals**

- No UI dashboard.
- No anomaly detection engine.
- No billing metrics.
- No fleet autoscaler.

---

### 0.04-S1 — Governance state model

**Milestone:** Splendor0.04-dev  
**Objective:** Define approval, escalation, intervention, circuit-breaker, and kill-switch states as first-class runtime/governance objects.

**Bounded scope**

- Define governance schemas.
- Define state transitions.
- Define trace events for governance state changes.

**Functional outputs**

- ApprovalRequest, ApprovalGrant, ApprovalDenial, Escalation, Intervention, CircuitBreaker, KillSwitch schemas.
- Governance state transition table.
- Trace event reference updates.

**Verifiable acceptance criteria**

- [ ] Every governance object has stable identity, scope, created time, expiry where applicable, reason, issuer/source, and trace linkage.
- [ ] Invalid governance transitions are rejected and traced.
- [ ] Governance state can be tied to tenant, agent, run, action, adapter, node, instance, fleet, or global scope as appropriate.
- [ ] Governance state does not require an enterprise UI to exist.
- [ ] Expiry and revocation are represented explicitly.
- [ ] Schema tests validate forward-compatible extension fields without allowing arbitrary authority escalation.

**Required sprint documentation**

- `docs/reference/governance-state.md`
- `docs/reference/trace-events.md#governance-events`
- `docs/milestones/0.04-dev/S1-governance-state-model.md`
- `adr/0003-governance-state-scopes.md`

**Integration constraints**

- Governance must compose with gateway/verifier behavior rather than bypass it.
- Keep scope model clear so future external control planes can integrate without owning kernel logic.

**Explicit non-goals**

- No approval UI.
- No enterprise org model.
- No external IAM integration.

---

### 0.04-S2 — Approval verifier

**Milestone:** Splendor0.04-dev  
**Objective:** Require, pause for, validate, and apply approvals for scoped actions and runs through the verifier chain.

**Bounded scope**

- Implement approval-required detection.
- Pause run/action when approval is missing.
- Resume or deny based on valid approval grant/denial/expiry.

**Functional outputs**

- Approval verifier.
- Approval token/percept validation.
- Run pause/resume integration.
- Tests for grant, denial, expiry, revocation, and wrong scope.

**Verifiable acceptance criteria**

- [ ] An action requiring approval returns needs_approval and does not execute the adapter.
- [ ] The run enters waiting_for_approval or equivalent paused state with trace linkage.
- [ ] A valid approval grant scoped to the action/run permits re-evaluation and execution.
- [ ] Approval for a different tenant, agent, action, adapter, or expired window is rejected.
- [ ] Approval denial or expiry results in denied/cancelled state, not silent retry.
- [ ] Replay explains why approval was required and what grant/denial changed the outcome.

**Required sprint documentation**

- `docs/reference/approval-verifier.md`
- `docs/reference/governance-workflows.md`
- `docs/milestones/0.04-dev/S2-approval-verifier.md`
- `examples/action-approval-flow/README.md`

**Integration constraints**

- Approval is a verifier/gateway concern. External systems may issue approval objects but cannot bypass gateway execution.
- Do not implement broad workflow orchestration in the verifier.

**Explicit non-goals**

- No approval queue UI.
- No human notification system.
- No workflow DSL.

---

### 0.04-S3 — Escalation engine

**Milestone:** Splendor0.04-dev  
**Objective:** Escalate uncertain, repeated, timed-out, or risky runtime situations through a small, explicit policy engine.

**Bounded scope**

- Define escalation triggers.
- Implement timeout, repeated failure/denial, verifier uncertainty, quota pressure, and policy expiry escalation.
- Emit trace events and structured intervention outcomes.

**Functional outputs**

- Escalation policy schema.
- Escalation evaluator.
- Intervention outcome handling.
- Tests for each trigger category.

**Verifiable acceptance criteria**

- [ ] Verifier uncertainty can produce needs_intervention rather than allow.
- [ ] Repeated adapter failure reaches configured escalation threshold and pauses or denies according to policy.
- [ ] Approval timeout escalates or denies according to explicit policy.
- [ ] Quota pressure can pause or deny without corrupting quota ledger.
- [ ] Policy expiry causes high-risk actions to deny or request intervention.
- [ ] Escalation trace includes trigger, threshold, scope, action/run reference, and decision.

**Required sprint documentation**

- `docs/reference/escalation-policies.md`
- `docs/milestones/0.04-dev/S3-escalation-engine.md`
- `examples/escalation-basic/README.md`

**Integration constraints**

- Keep escalation deterministic and explainable. Avoid inventing a general business workflow engine.
- Escalation outcomes should use existing run/action statuses where possible.

**Explicit non-goals**

- No BPMN/workflow language.
- No ticketing integration.
- No notification platform.

---

### 0.04-S4 — Circuit breakers

**Milestone:** Splendor0.04-dev  
**Objective:** Stop unsafe or unhealthy execution at tenant, agent, adapter, action, node, instance, fleet, or global scopes.

**Bounded scope**

- Define circuit-breaker scopes and states.
- Evaluate breakers before side effects.
- Trace trips, resets, and denied actions.

**Functional outputs**

- CircuitBreaker schema.
- Breaker evaluator/verifier.
- Breaker management API or local config path.
- Tests for all supported scopes.

**Verifiable acceptance criteria**

- [ ] A tripped adapter breaker prevents matching adapter execution across affected scope.
- [ ] A tenant breaker prevents tenant runs/actions without blocking unrelated tenants.
- [ ] A node/instance breaker prevents new work and optionally pauses existing runs according to policy.
- [ ] Breaker reset requires explicit authorized event and is traced.
- [ ] Breaker evaluation happens before adapter execution.
- [ ] Replay shows which breaker denied an action and at what scope.

**Required sprint documentation**

- `docs/reference/circuit-breakers.md`
- `docs/milestones/0.04-dev/S4-circuit-breakers.md`
- `examples/circuit-breaker-basic/README.md`

**Integration constraints**

- Circuit breakers should be simple control objects, not a monitoring platform. Telemetry can trigger future automation, but this sprint only enforces breaker state.
- Breaker scopes must align with distributed identity model from 0.03.

**Explicit non-goals**

- No automated incident response system.
- No UI dashboard.
- No predictive safety model.

---

### 0.04-S5 — Central policy distribution

**Milestone:** Splendor0.04-dev  
**Objective:** Distribute, cache, validate, expire, and revoke policy bundles without requiring central connectivity for every local tick.

**Bounded scope**

- Define policy bundle metadata and TTL.
- Implement cache and sync semantics.
- Fail closed on missing/expired/invalid policies according to action risk.

**Functional outputs**

- Policy bundle schema.
- Policy sync API.
- Local policy cache.
- Policy TTL and revocation tests.

**Verifiable acceptance criteria**

- [ ] Runtime records policy bundle ID/version in run and trace metadata.
- [ ] Expired policy prevents high-risk side effects and follows configured degraded/offline behavior.
- [ ] Invalid policy signature or schema is rejected before policy invocation.
- [ ] Policy revocation prevents new affected runs and side effects.
- [ ] Resident node can continue low-risk cached-policy operation when disconnected if policy allows it.
- [ ] Policy sync failure is visible in telemetry/trace without silently broadening permissions.

**Required sprint documentation**

- `docs/reference/policy-distribution.md`
- `docs/reference/offline-policy-cache.md`
- `docs/milestones/0.04-dev/S5-central-policy-distribution.md`
- `examples/policy-cache-degraded-mode/README.md`

**Integration constraints**

- Policy distribution should support both cloud and physical/edge future use.
- Do not require a central manager call for every tick.

**Explicit non-goals**

- No policy authoring UI.
- No enterprise policy language product.
- No global consensus on policy state.

---

### 0.04-S6 — External governance adapter

**Milestone:** Splendor0.04-dev  
**Objective:** Let Harmony or another control plane issue scoped work orders, approvals, and governance signals without making Splendor product-specific.

**Bounded scope**

- Define minimal adapter endpoints/contracts.
- Map external approvals to Splendor governance objects.
- Map Splendor traces/state/artifacts back to external references.

**Functional outputs**

- External governance adapter contract.
- Reference Harmony-compatible adapter skeleton.
- Scoped work-order bridge tests.
- Trace-linked artifact/approval example.

**Verifiable acceptance criteria**

- [ ] Adapter accepts scoped work orders, not broad user credentials.
- [ ] External approval maps to ApprovalGrant/Denial with explicit scope, expiry, issuer, and trace reference.
- [ ] Artifact references include run ID, state node ID, trace range, approval state, and source refs.
- [ ] Adapter failure cannot approve or execute an action by default.
- [ ] The same contract can be used by a non-Harmony control plane with renamed endpoints or thin mapping.
- [ ] Docs clearly separate Splendor-owned runtime concerns from external product concerns.

**Required sprint documentation**

- `docs/integrations/governance-adapter.md`
- `docs/integrations/harmony.md`
- `docs/milestones/0.04-dev/S6-external-governance-adapter.md`
- `examples/harmony-governance-bridge/README.md`

**Integration constraints**

- Keep external integration at the boundary. Splendor remains source of truth for runtime enforcement.
- Do not add enterprise SaaS concepts to core schemas unless they map to runtime primitives.

**Explicit non-goals**

- No Harmony admin UI.
- No billing/org/workspace product implementation.
- No connector marketplace.

---

### 0.04-S7 — Governance replay and audit

**Milestone:** Splendor0.04-dev  
**Objective:** Explain and export why governed actions were allowed, denied, paused, escalated, resumed, or circuit-broken.

**Bounded scope**

- Extend replay explanation for governance events.
- Add audit export format.
- Support governance event filtering by scope and run/action.

**Functional outputs**

- Governance replay explanation output.
- Audit export schema.
- Queries for approval, denial, escalation, breaker, kill-switch, pause, and resume events.

**Verifiable acceptance criteria**

- [ ] Replay explains approval requirement, grant/denial, expiry, and final action outcome.
- [ ] Replay explains circuit-breaker denial with scope and breaker ID.
- [ ] Audit export includes identity, work order, policy version, verifier results, action outcome, state node, and trace range.
- [ ] Audit export does not include raw secrets or broad credentials.
- [ ] Filtering by tenant, agent, run, action, adapter, node, instance, and fleet works where data exists.
- [ ] A governance audit can be produced without re-running the original policy or side effects.

**Required sprint documentation**

- `docs/reference/governance-replay.md`
- `docs/reference/audit-export.md`
- `docs/milestones/0.04-dev/S7-governance-replay-audit.md`
- `examples/governance-audit-export/README.md`

**Integration constraints**

- Audit is derived from trace/state/governance primitives. Do not create a separate parallel truth source.
- Keep export format stable enough for external tools but mark 0.04 schema as dev unless 0.1 freezes it.

**Explicit non-goals**

- No compliance certification claim.
- No dashboard.
- No long-term archival product.

---

### 0.05-S1 — Device profile schema

**Milestone:** Splendor0.05-dev  
**Objective:** Represent physical and edge nodes as explicit capability-bearing runtime targets without making Splendor a device driver or real-time controller.

**Bounded scope**

- Define device node kinds and profiles.
- Represent sensors, bounded actions, power, safety status, locality, and runtime mode.
- Validate physical capability documents.

**Functional outputs**

- DeviceProfile schema.
- Capability categories for sensors, bounded actions, local compute, network, battery/power, safety status.
- Examples for robot, drone, humanoid, edge appliance, desktop sidecar, and industrial device.

**Verifiable acceptance criteria**

- [ ] A drone/robot profile can advertise high-level actions such as move_to_waypoint, dock, inspect_zone, capture_image, and return_to_base.
- [ ] Profiles cannot advertise raw motor/actuator writes as Splendor direct actions.
- [ ] Capability validation rejects ambiguous or unsafe action classes.
- [ ] Profile includes safety constraints and local policy/offline capability indicators.
- [ ] Placement v0 can distinguish physical device, cloud helper, simulation, and desktop sidecar targets.
- [ ] Docs explicitly state Splendor is not the hard real-time control layer.

**Required sprint documentation**

- `docs/reference/device-profiles.md`
- `docs/reference/physical-capabilities.md`
- `docs/milestones/0.05-dev/S1-device-profile-schema.md`
- `examples/device-profiles/README.md`

**Integration constraints**

- Keep device profiles compatible with 0.03 capability model.
- Use high-level capability names that can map to ROS/native/device middleware later.

**Explicit non-goals**

- No ROS integration implementation.
- No flight controller integration.
- No motor control.
- No safety certification.

---

### 0.05-S2 — Offline policy cache

**Milestone:** Splendor0.05-dev  
**Objective:** Allow resident physical/edge instances to operate within cached policy during disconnection while failing closed for high-risk actions.

**Bounded scope**

- Extend policy cache for offline operation.
- Define degraded mode.
- Enforce TTL and local-risk rules.

**Functional outputs**

- Offline policy cache behavior.
- Degraded mode state.
- High-risk action denial rules.
- Tests for connected, disconnected, expired, revoked, and missing-policy cases.

**Verifiable acceptance criteria**

- [ ] Disconnected node can continue explicitly allowed low-risk actions within policy TTL.
- [ ] High-risk actions are denied or require local operator approval when central connectivity is unavailable.
- [ ] Expired policy transitions runtime to degraded, paused, or deny mode according to policy.
- [ ] Reconnect sync updates policy state without losing local trace continuity.
- [ ] Policy cache stores version, signature/validation metadata, TTL, scope, and last-sync time.
- [ ] Offline behavior is visible in trace and telemetry.

**Required sprint documentation**

- `docs/reference/offline-operation.md`
- `docs/reference/offline-policy-cache.md`
- `docs/milestones/0.05-dev/S2-offline-policy-cache.md`
- `examples/offline-device-policy-cache/README.md`

**Integration constraints**

- Build on 0.04 policy distribution. Do not create a separate physical-only policy system.
- Keep local autonomy bounded and inspectable.

**Explicit non-goals**

- No mesh policy consensus.
- No autonomous policy authoring on device.
- No bypass of central policy authority after TTL expiry.

---

### 0.05-S3 — Local trace buffer

**Milestone:** Splendor0.05-dev  
**Objective:** Preserve trace continuity for disconnected physical/edge nodes and sync safely after reconnect.

**Bounded scope**

- Harden local trace buffering for offline operation.
- Add reconnect sync semantics.
- Handle conflicts, missing segments, and storage pressure.

**Functional outputs**

- Offline trace buffer.
- Reconnect sync path.
- Storage pressure behavior.
- Tests for offline run, reconnect, duplicate sync, corrupted segment, and buffer full.

**Verifiable acceptance criteria**

- [ ] A disconnected node records local trace events for ticks, actions, denials, safety checks, operator interventions, and policy status.
- [ ] Reconnect sync preserves local ordering and identifies offline intervals.
- [ ] Duplicate sync does not create duplicate central trace events.
- [ ] Buffer-full behavior fails closed for side-effectful actions if trace durability cannot be preserved.
- [ ] Corrupted local segment is detected and quarantined or rejected.
- [ ] Replay can identify offline execution period and sync boundary.

**Required sprint documentation**

- `docs/reference/local-trace-buffer.md`
- `docs/reference/trace-sync.md#offline-nodes`
- `docs/milestones/0.05-dev/S3-local-trace-buffer.md`
- `examples/offline-trace-sync/README.md`

**Integration constraints**

- Build on 0.03 trace aggregation. Do not fork trace formats for devices.
- Use storage abstraction only at buffer boundary; avoid building a general embedded database framework.

**Explicit non-goals**

- No full observability stack.
- No device log collector.
- No cloud analytics UI.

---

### 0.05-S4 — Robotics adapter interface

**Milestone:** Splendor0.05-dev  
**Objective:** Define a safe high-level adapter contract for physical actions mediated by local device middleware and real-time controllers.

**Bounded scope**

- Define allowed high-level mission/action classes.
- Define forbidden low-level action classes.
- Implement reference mock/simulated robotics adapter.

**Functional outputs**

- RoboticsAdapter interface.
- Mission action class reference.
- Forbidden action policy.
- Simulated robot/drone adapter example.

**Verifiable acceptance criteria**

- [ ] Allowed actions are high-level and bounded: read_battery, read_sensor_summary, read_map, move_to_waypoint, return_to_base, dock, inspect_zone, capture_image, pause_mission, resume_mission, request_operator_override, notify_operator, upload_trace_summary.
- [ ] Raw actuator writes, firmware safety bypass, motor PWM, and flight-controller internals are rejected before adapter execution.
- [ ] Every physical action goes through gateway and safety verifier chain.
- [ ] Adapter outputs include action status, evidence/reference data, and postcondition data where applicable.
- [ ] Simulated adapter demonstrates success, verifier denial, adapter failure, and operator override request.
- [ ] Docs clearly place real-time control below Splendor.

**Required sprint documentation**

- `docs/reference/robotics-adapter.md`
- `docs/reference/physical-actions.md`
- `docs/milestones/0.05-dev/S4-robotics-adapter-interface.md`
- `examples/simulated-drone-adapter/README.md`

**Integration constraints**

- Adapter must be middleware-agnostic. ROS/native/industrial stacks can implement it later.
- Keep physical action names abstract and bounded, not hardware-driver specific.

**Explicit non-goals**

- No production ROS package.
- No motor controller.
- No live robot certification.
- No direct cloud-to-actuator path.

---

### 0.05-S5 — Safety verifier API

**Milestone:** Splendor0.05-dev  
**Objective:** Evaluate local physical safety constraints before and after high-level physical actions.

**Bounded scope**

- Define safety verifier interface.
- Add verifier evidence model.
- Implement simulated verifiers for geofence, battery, collision, emergency stop, altitude, privacy, or proximity checks.

**Functional outputs**

- SafetyVerifier interface.
- Safety evidence schema.
- Reference simulated safety verifiers.
- Fail-closed safety tests.

**Verifiable acceptance criteria**

- [ ] Safety verifier can deny a physical action before adapter execution.
- [ ] Verifier uncertainty returns deny or needs_intervention, not allow.
- [ ] Postcondition verifier can flag unsafe or failed physical outcome.
- [ ] Evidence includes relevant sensor/status references without requiring raw sensor data in trace.
- [ ] Simulated tests cover geofence violation, low battery, emergency stop, collision risk, and missing verifier.
- [ ] Missing required safety verifier fails closed.

**Required sprint documentation**

- `docs/reference/safety-verifiers.md`
- `docs/reference/physical-safety.md`
- `docs/milestones/0.05-dev/S5-safety-verifier-api.md`
- `examples/simulated-safety-verifiers/README.md`

**Integration constraints**

- Safety verifiers are part of the existing verifier chain. Do not create a parallel physical execution path.
- Evidence model should support future certified/verifiable device data without requiring it now.

**Explicit non-goals**

- No certified safety system.
- No low-level sensor fusion.
- No real-time collision avoidance.

---

### 0.05-S6 — Cloud-helper pattern

**Milestone:** Splendor0.05-dev  
**Objective:** Allow physical devices to use cloud/on-prem helper Splendor instances for planning or analysis without granting direct actuator authority.

**Bounded scope**

- Define helper work-order pattern.
- Define route/plan proposal artifact.
- Require local validation before execution.

**Functional outputs**

- CloudHelperWorkOrder example/schema.
- RoutePlan or MissionPlan proposal reference.
- Local validation flow.
- End-to-end simulated device plus cloud helper example.

**Verifiable acceptance criteria**

- [ ] Cloud helper receives objective and scoped data refs, not raw actuator authority.
- [ ] Helper returns plan/proposal artifact or typed message, not direct physical commands.
- [ ] Local device Splendor instance validates proposal against policy and safety verifiers before bounded actions.
- [ ] Rejected plan creates trace events on helper and device sides where applicable.
- [ ] Network failure during helper call does not leave device in unsafe action state.
- [ ] Replay shows helper proposal, local validation, and final local action decisions.

**Required sprint documentation**

- `docs/reference/cloud-helper-physical.md`
- `docs/milestones/0.05-dev/S6-cloud-helper-pattern.md`
- `examples/robot-cloud-route-planner/README.md`

**Integration constraints**

- This pattern should reuse 0.03 remote messaging and work orders plus 0.05 device safety.
- Keep cloud helper advisory by default. Direct physical authority requires future explicit safety review.

**Explicit non-goals**

- No cloud teleoperation.
- No direct actuator control from cloud.
- No fleet route optimizer product.

---

### 0.05-S7 — Physical demo harness

**Milestone:** Splendor0.05-dev  
**Objective:** Provide a runnable simulation that proves the physical/edge primitives work together without live hardware.

**Bounded scope**

- Build simulated drone/robot mission.
- Use device profile, offline policy cache, local trace buffer, robotics adapter, safety verifiers, operator intervention, and optional cloud helper.
- Generate replayable trace.

**Functional outputs**

- Physical simulation harness.
- Scenario tests.
- Trace/replay sample output.
- Operator intervention example.

**Verifiable acceptance criteria**

- [ ] The harness runs a successful mission with high-level physical actions only.
- [ ] The harness runs at least one safety denial scenario.
- [ ] The harness runs one offline interval and reconnect trace sync.
- [ ] The harness runs one operator intervention/override request flow.
- [ ] The harness runs one cloud-helper proposal validated locally.
- [ ] All scenarios produce replayable trace and final state head.

**Required sprint documentation**

- `docs/milestones/0.05-dev/S7-physical-demo-harness.md`
- `examples/physical-simulation-harness/README.md`
- `examples/physical-simulation-harness/scenarios.md`

**Integration constraints**

- Use simulation to validate contracts, not to imply hardware readiness.
- Keep harness modular so real adapters can replace simulated components later.

**Explicit non-goals**

- No production robot deployment.
- No live flight testing.
- No certification claim.

---

### 0.1-S1 — Stable schema freeze

**Milestone:** Splendor0.1-dev  
**Objective:** Freeze the first stable primitive schema set for implementers while preserving explicit extension points.

**Bounded scope**

- Finalize canonical schemas.
- Generate Rust/Python/TypeScript types where appropriate.
- Define extension and versioning policy.

**Functional outputs**

- Stable schemas for Tenant, Agent, Run, Tick, Action, Percept, Message, StateNode, TraceEvent, WorkOrder, Approval, Policy, Constraint, Verifier, Adapter, Feedback, and Reward.
- Schema versioning guide.
- Generated type packages or parity tests.

**Verifiable acceptance criteria**

- [ ] Every stable primitive has a schema, identity rules, required fields, optional fields, and extension rules.
- [ ] Breaking vs non-breaking schema changes are clearly defined.
- [ ] Generated or parity-tested Rust/Python/TypeScript types agree on required fields and enum values.
- [ ] Deprecated dev fields are either removed or marked with migration guidance.
- [ ] Unknown extension fields cannot widen authority or bypass verification.
- [ ] A schema conformance suite validates all stable examples.

**Required sprint documentation**

- `docs/spec/0.1/primitives.md`
- `docs/spec/0.1/schema-versioning.md`
- `docs/milestones/0.1-dev/S1-stable-schema-freeze.md`

**Integration constraints**

- Freeze contracts, not internals. Runtime implementation can evolve behind stable schemas.
- Use explicit extension points for future features; do not leave ambiguous catch-all authority fields.

**Explicit non-goals**

- No new runtime feature beyond spec stabilization.
- No marketplace.
- No compatibility guarantee for undocumented internals.

---

### 0.1-S2 — Compatibility test suite

**Milestone:** Splendor0.1-dev  
**Objective:** Create a conformance suite that third-party clients, adapters, and policy hosts can run against the stable primitives.

**Bounded scope**

- Define required conformance categories.
- Build tests for runtime loop, gateway, trace, state, replay, messages, work orders, governance, and adapters.
- Publish expected fixtures.

**Functional outputs**

- Conformance test suite.
- Fixture library.
- Pass/fail report format.
- Documentation for running tests locally and in CI.

**Verifiable acceptance criteria**

- [ ] Suite covers positive, denial, failure, replay, and fail-closed paths for core primitives.
- [ ] Adapters can be tested without requiring production secrets or external systems.
- [ ] Trace and state fixtures verify ordering and identity consistency.
- [ ] Work-order tests reject unsigned, expired, revoked, and overbroad authority.
- [ ] Governance tests verify approval, denial, escalation, and circuit breaker behavior.
- [ ] Conformance report identifies exact failed primitive and requirement.

**Required sprint documentation**

- `docs/spec/0.1/conformance.md`
- `docs/development/conformance-suite.md`
- `docs/milestones/0.1-dev/S2-compatibility-test-suite.md`

**Integration constraints**

- Make suite usable by internal and external implementers.
- Keep tests primitive-focused instead of environment-specific.

**Explicit non-goals**

- No full certification business process.
- No vendor-specific test runners.
- No UI test dashboard.

---

### 0.1-S3 — Adapter maturity model

**Milestone:** Splendor0.1-dev  
**Objective:** Classify adapters by safety, governance, and operational maturity so future ecosystem growth remains controlled.

**Bounded scope**

- Define adapter lifecycle levels.
- Define required verifier/trace/replay behavior per level.
- Add adapter checklist and example metadata.

**Functional outputs**

- Adapter maturity levels: experimental, local-safe, network-safe, governance-aware, device-safe.
- Adapter metadata schema.
- Adapter review checklist.
- Example adapter manifests.

**Verifiable acceptance criteria**

- [ ] Each maturity level has explicit requirements and prohibited claims.
- [ ] Side-effectful adapters require gateway enforcement, verifier coverage, trace events, and replay-safe behavior.
- [ ] Device-safe adapters require high-level bounded actions and safety verifier integration.
- [ ] Network-safe adapters define egress/data-scope/quota behavior.
- [ ] Governance-aware adapters support approval/circuit-breaker semantics where required.
- [ ] Experimental adapters are clearly blocked from stable/production claims.

**Required sprint documentation**

- `docs/spec/0.1/adapter-maturity.md`
- `docs/development/adapter-review-checklist.md`
- `docs/milestones/0.1-dev/S3-adapter-maturity-model.md`

**Integration constraints**

- This model should guide future ecosystem growth without adding marketplace mechanics.
- Keep maturity levels evidence-based and testable.

**Explicit non-goals**

- No public marketplace.
- No legal certification.
- No vendor approval workflow.

---

### 0.1-S4 — SDK and API stabilization

**Milestone:** Splendor0.1-dev  
**Objective:** Stabilize Rust traits, Python SDK, daemon API, and TypeScript client around the 0.1 primitive contracts.

**Bounded scope**

- Mark stable public APIs.
- Document compatibility guarantees and deprecation policy.
- Align SDK behavior with conformance suite.

**Functional outputs**

- Stable Rust trait/API reference.
- Python SDK stable API reference.
- Daemon API stable reference.
- TypeScript client stable reference.
- Deprecation policy.

**Verifiable acceptance criteria**

- [ ] Public stable APIs are explicitly named; internal APIs are not accidentally promised.
- [ ] SDKs pass relevant conformance tests against stable schemas.
- [ ] Daemon API version negotiation or compatibility header behavior is documented.
- [ ] Error shapes are stable enough for clients to handle programmatically.
- [ ] Deprecation policy defines notice, replacement, and removal expectations.
- [ ] Examples use stable APIs only.

**Required sprint documentation**

- `docs/spec/0.1/api-stability.md`
- `docs/sdk/python/stable-0.1.md`
- `docs/sdk/typescript/stable-0.1.md`
- `docs/reference/runtime-daemon-api.md`
- `docs/milestones/0.1-dev/S4-sdk-api-stabilization.md`

**Integration constraints**

- Stabilize the boundary that future systems integrate with, not every implementation detail.
- Avoid over-promising physical/fleet behavior beyond documented stable support.

**Explicit non-goals**

- No stable native Node binding unless already mature.
- No browser runtime guarantee.
- No undocumented API compatibility.

---

### 0.1-S5 — Operational documentation

**Milestone:** Splendor0.1-dev  
**Objective:** Publish operational guides that show how to run Splendor in local, daemon, resident, fleet, governance, Harmony, and physical/edge modes using stable primitives.

**Bounded scope**

- Write task-oriented operator docs.
- Include minimal diagrams and failure-mode guidance.
- Document non-goals and safety boundaries.

**Functional outputs**

- Local runtime guide.
- Daemon operation guide.
- Resident node/fleet guide.
- Governance operation guide.
- Harmony/external control-plane integration guide.
- Physical/edge guide.

**Verifiable acceptance criteria**

- [ ] Each guide includes setup, run path, trace/state inspection, failure handling, and teardown or cleanup.
- [ ] Guides state required maturity level and limitations.
- [ ] Physical/edge guide says Splendor is not a real-time controller and lists forbidden low-level actions.
- [ ] Governance guide shows approval, denial, escalation, and circuit breaker flows.
- [ ] Fleet guide shows signed work orders, node registration, trace sync, and telemetry.
- [ ] Docs have runnable examples or fixtures wherever practical.

**Required sprint documentation**

- `docs/operations/local-runtime.md`
- `docs/operations/runtime-daemon.md`
- `docs/operations/resident-fleet.md`
- `docs/operations/governance.md`
- `docs/operations/harmony-integration.md`
- `docs/operations/physical-edge.md`
- `docs/milestones/0.1-dev/S5-operational-documentation.md`

**Integration constraints**

- Write docs for stable primitives and operational patterns, not product UI flows.
- Keep docs concise enough to maintain; move deep references to spec pages.

**Explicit non-goals**

- No admin SaaS manual.
- No vendor-specific deployment cookbook for every platform.
- No production safety certification guide.

---

### 0.1-S6 — Migration and release

**Milestone:** Splendor0.1-dev  
**Objective:** Release 0.1 with explicit migration rules from dev milestones and a clear compatibility contract.

**Bounded scope**

- Write migration guide from 0.01-0.05 dev schemas/APIs.
- Finalize changelog.
- Tag release and archive dev incompatibilities.
- Publish release validation checklist.

**Functional outputs**

- 0.1 migration guide.
- 0.1 changelog.
- Compatibility guarantee.
- Release validation report.

**Verifiable acceptance criteria**

- [ ] Migration guide maps each renamed/changed/removed dev field or API to stable replacement or removal reason.
- [ ] Release checklist includes conformance suite result, docs review, examples review, and known limitations.
- [ ] Compatibility guarantee defines patch/minor expectations and what remains experimental.
- [ ] Release notes list stable primitives and explicitly mark experimental future work.
- [ ] A fresh user can run local and daemon stable examples from the release docs.
- [ ] Known gaps are documented honestly without implying unsupported production guarantees.

**Required sprint documentation**

- `docs/releases/0.1-dev.md`
- `docs/releases/0.1-migration.md`
- `docs/releases/compatibility-policy.md`
- `CHANGELOG.md`
- `docs/milestones/0.1-dev/S6-migration-release.md`

**Integration constraints**

- 0.1 should be a stable integration line. Avoid sneaking in new major primitives during the release sprint.
- Prefer deprecation with migration over silent breakage.

**Explicit non-goals**

- No 1.0 production claim.
- No certification claim.
- No enterprise support policy.

---

## 6. Required format for each sprint documentation file

Each sprint documentation file should use this template.

```md
# <Sprint ID> — <Sprint title>

## Objective

One paragraph. State the runtime primitive being strengthened.

## Functional scope

- What is implemented.
- What is validated.
- What is exposed publicly, if anything.

## Non-goals

- What is deliberately not implemented.
- Which future milestones are not being pulled forward.

## Public contracts changed

List schemas, APIs, SDK types, CLI commands, daemon endpoints, trace events, and examples changed.

## Runtime primitive impact

| Primitive   | Impact                 |
| ----------- | ---------------------- |
| Percept     | none / changed / added |
| Policy      | none / changed / added |
| Gateway     | none / changed / added |
| Verifier    | none / changed / added |
| State graph | none / changed / added |
| Trace store | none / changed / added |
| Replay      | none / changed / added |
| Message     | none / changed / added |
| Work order  | none / changed / added |
| Governance  | none / changed / added |

## Trace behavior

- New event classes.
- Changed event fields.
- Required ordering.
- Failure/denial events.

## State behavior

- State nodes created.
- State head updates.
- Snapshot/reference behavior.
- Commit failure behavior.

## Gateway and verifier behavior

- New checks.
- New denial reasons.
- Fail-closed behavior.
- Adapter execution conditions.

## Replay behavior

- What can be reconstructed.
- What is simulated.
- What is never re-executed.
- Known limitations.

## Tests and evidence

| Test        | Purpose | Evidence |
| ----------- | ------- | -------- |
| unit        | ...     | ...      |
| contract    | ...     | ...      |
| integration | ...     | ...      |
| negative    | ...     | ...      |
| replay      | ...     | ...      |

## Example or fixture

Include command, script, or fixture path.

## Future extension notes

State exactly how later milestones can extend this work without rewriting it.
```

---

## 7. Review-agent rubric

Review agents should block a sprint if any of these are true.

```text
[ ] The sprint implements behavior outside its explicit scope.
[ ] The sprint adds a side-effect path outside the gateway.
[ ] The sprint hides state outside the state graph or explicit snapshot/reference model.
[ ] The sprint emits logs instead of trace events for runtime-contract data.
[ ] The sprint lacks denial/failure tests.
[ ] The sprint lacks replay behavior or says replay is future work for a primitive introduced now.
[ ] The sprint broadens permissions through shared agents, adapters, work orders, or external integrations.
[ ] The sprint couples core kernel primitives to Harmony, Kubernetes, ROS, a specific cloud, or a specific broker unnecessarily.
[ ] The sprint treats telemetry or docs as proof of runtime correctness.
[ ] The sprint changes public contracts without migration notes.
```

Approve only when the sprint is clear, functional, testable, traceable, replay-aware, and limited to the smallest primitive-aligned implementation.

---

## 8. Final implementation guidance

The project should remain ambitious, but the ambition should sit in the primitives, not in accidental platform complexity.

Build in this order:

```text
prove local correctness
prove local multi-agent correctness
prove daemon and SDK contracts
prove resident/fleet identity
prove distributed transport/state/trace boundaries
prove governance enforcement
prove physical/edge safety boundaries
freeze stable specs
```

Do not make the project easy by skipping denial paths, replay, trace integrity, scoped authority, or state ownership.

Do not over-engineer by building a product platform, a broker ecosystem, a workflow engine, a robot controller, or a distributed consensus layer before the kernel primitive requires it.

The correct implementation shape is:

```text
small primitive
strict contract
clear identity
fail-closed verifier
gateway-mediated side effect
state commit
trace event
replay explanation
focused docs
future extension hook
```
