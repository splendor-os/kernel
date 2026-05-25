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

This directory is the source of truth for sprint verification criteria. The former
monolithic `docs/rules/verifiable_criteria.md` was split structurally so agents and
reviewers can read the global criteria first and then load only the relevant sprint
criteria file. The split does not change the meaning of any acceptance criteria.

| Sprint | Milestone | Title | Criteria file |
| ------ | --------- | ----- | ------------- |
| 0.01-H1 | Splendor0.01-dev | Baseline conformance | [sprints/0.01-H1-baseline-conformance.md](sprints/0.01-H1-baseline-conformance.md) |
| 0.01-H2 | Splendor0.01-dev | Trace and replay hardening | [sprints/0.01-H2-trace-and-replay-hardening.md](sprints/0.01-H2-trace-and-replay-hardening.md) |
| 0.01-H3 | Splendor0.01-dev | Python SDK ergonomics | [sprints/0.01-H3-python-sdk-ergonomics.md](sprints/0.01-H3-python-sdk-ergonomics.md) |
| 0.01-H4 | Splendor0.01-dev | Release hygiene | [sprints/0.01-H4-release-hygiene.md](sprints/0.01-H4-release-hygiene.md) |
| 0.02-S0 | Splendor0.02-dev | Daemon Security Boundary | [sprints/0.02-S0-daemon-security-boundary.md](sprints/0.02-S0-daemon-security-boundary.md) |
| 0.02-S1 | Splendor0.02-dev | Message schema contract | [sprints/0.02-S1-message-schema-contract.md](sprints/0.02-S1-message-schema-contract.md) |
| 0.02-S2 | Splendor0.02-dev | Local message router | [sprints/0.02-S2-local-message-router.md](sprints/0.02-S2-local-message-router.md) |
| 0.02-S3 | Splendor0.02-dev | Agent isolation ledger | [sprints/0.02-S3-agent-isolation-ledger.md](sprints/0.02-S3-agent-isolation-ledger.md) |
| 0.02-S4 | Splendor0.02-dev | Local delegation model | [sprints/0.02-S4-local-delegation-model.md](sprints/0.02-S4-local-delegation-model.md) |
| 0.02-S5 | Splendor0.02-dev | Runtime daemon API | [sprints/0.02-S5-runtime-daemon-api.md](sprints/0.02-S5-runtime-daemon-api.md) |
| 0.02-S6 | Splendor0.02-dev | TypeScript surface | [sprints/0.02-S6-typescript-surface.md](sprints/0.02-S6-typescript-surface.md) |
| 0.02-S7 | Splendor0.02-dev | Multi-agent replay and test harness | [sprints/0.02-S7-multi-agent-replay-and-test-harness.md](sprints/0.02-S7-multi-agent-replay-and-test-harness.md) |
| 0.03-S1 | Splendor0.03-dev | Distributed identity model | [sprints/0.03-S1-distributed-identity-model.md](sprints/0.03-S1-distributed-identity-model.md) |
| 0.03-S2 | Splendor0.03-dev | Node and instance registry | [sprints/0.03-S2-node-and-instance-registry.md](sprints/0.03-S2-node-and-instance-registry.md) |
| 0.03-S3 | Splendor0.03-dev | Signed work orders | [sprints/0.03-S3-signed-work-orders.md](sprints/0.03-S3-signed-work-orders.md) |
| 0.03-S4 | Splendor0.03-dev | Placement v0 | [sprints/0.03-S4-placement-v0.md](sprints/0.03-S4-placement-v0.md) |
| 0.03-S5 | Splendor0.03-dev | Remote message transport | [sprints/0.03-S5-remote-message-transport.md](sprints/0.03-S5-remote-message-transport.md) |
| 0.03-S6 | Splendor0.03-dev | Trace aggregation | [sprints/0.03-S6-trace-aggregation.md](sprints/0.03-S6-trace-aggregation.md) |
| 0.03-S7 | Splendor0.03-dev | State handoff v0 | [sprints/0.03-S7-state-handoff-v0.md](sprints/0.03-S7-state-handoff-v0.md) |
| 0.03-S8 | Splendor0.03-dev | Fleet telemetry | [sprints/0.03-S8-fleet-telemetry.md](sprints/0.03-S8-fleet-telemetry.md) |
| 0.04-S1 | Splendor0.04-dev | Governance state model | [sprints/0.04-S1-governance-state-model.md](sprints/0.04-S1-governance-state-model.md) |
| 0.04-S2 | Splendor0.04-dev | Approval verifier | [sprints/0.04-S2-approval-verifier.md](sprints/0.04-S2-approval-verifier.md) |
| 0.04-S3 | Splendor0.04-dev | Escalation engine | [sprints/0.04-S3-escalation-engine.md](sprints/0.04-S3-escalation-engine.md) |
| 0.04-S4 | Splendor0.04-dev | Circuit breakers | [sprints/0.04-S4-circuit-breakers.md](sprints/0.04-S4-circuit-breakers.md) |
| 0.04-S5 | Splendor0.04-dev | Central policy distribution | [sprints/0.04-S5-central-policy-distribution.md](sprints/0.04-S5-central-policy-distribution.md) |
| 0.04-S6 | Splendor0.04-dev | External governance adapter | [sprints/0.04-S6-external-governance-adapter.md](sprints/0.04-S6-external-governance-adapter.md) |
| 0.04-S7 | Splendor0.04-dev | Governance replay and audit | [sprints/0.04-S7-governance-replay-and-audit.md](sprints/0.04-S7-governance-replay-and-audit.md) |
| 0.05-S1 | Splendor0.05-dev | Device profile schema | [sprints/0.05-S1-device-profile-schema.md](sprints/0.05-S1-device-profile-schema.md) |
| 0.05-S2 | Splendor0.05-dev | Offline policy cache | [sprints/0.05-S2-offline-policy-cache.md](sprints/0.05-S2-offline-policy-cache.md) |
| 0.05-S3 | Splendor0.05-dev | Local trace buffer | [sprints/0.05-S3-local-trace-buffer.md](sprints/0.05-S3-local-trace-buffer.md) |
| 0.05-S4 | Splendor0.05-dev | Robotics adapter interface | [sprints/0.05-S4-robotics-adapter-interface.md](sprints/0.05-S4-robotics-adapter-interface.md) |
| 0.05-S5 | Splendor0.05-dev | Safety verifier API | [sprints/0.05-S5-safety-verifier-api.md](sprints/0.05-S5-safety-verifier-api.md) |
| 0.05-S6 | Splendor0.05-dev | Cloud-helper pattern | [sprints/0.05-S6-cloud-helper-pattern.md](sprints/0.05-S6-cloud-helper-pattern.md) |
| 0.05-S7 | Splendor0.05-dev | Physical demo harness | [sprints/0.05-S7-physical-demo-harness.md](sprints/0.05-S7-physical-demo-harness.md) |
| 0.1-S1 | Splendor0.1-dev | Stable schema freeze | [sprints/0.1-S1-stable-schema-freeze.md](sprints/0.1-S1-stable-schema-freeze.md) |
| 0.1-S2 | Splendor0.1-dev | Compatibility test suite | [sprints/0.1-S2-compatibility-test-suite.md](sprints/0.1-S2-compatibility-test-suite.md) |
| 0.1-S3 | Splendor0.1-dev | Adapter maturity model | [sprints/0.1-S3-adapter-maturity-model.md](sprints/0.1-S3-adapter-maturity-model.md) |
| 0.1-S4 | Splendor0.1-dev | SDK and API stabilization | [sprints/0.1-S4-sdk-and-api-stabilization.md](sprints/0.1-S4-sdk-and-api-stabilization.md) |
| 0.1-S5 | Splendor0.1-dev | Operational documentation | [sprints/0.1-S5-operational-documentation.md](sprints/0.1-S5-operational-documentation.md) |
| 0.1-S6 | Splendor0.1-dev | Migration and release | [sprints/0.1-S6-migration-and-release.md](sprints/0.1-S6-migration-and-release.md) |

---

## 5. Per-sprint verification and documentation requirements

Per-sprint verification requirements now live in the linked files under
[`sprints/`](sprints/). Each file preserves the corresponding sprint's objective,
bounded scope, functional outputs, verifiable acceptance criteria, required sprint
documentation, integration constraints, and explicit non-goals.

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
