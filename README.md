# Splendor Kernel

**An AI Kernel for Self-Managed Neuro-Symbolic Agents**

## Intent

Splendor’s intent is to provide a kernel-grade runtime for persistent,
governable agent loops. It enforces explicit state, verified execution
boundaries, and append-only traces so autonomy is reproducible, auditable, and
safe. It runs on Unix-like systems and complements existing agent frameworks
without dictating the model or planner stack.

## Project status

Splendor0.02-dev scope is implemented and integrated as the local multi-agent
runtime plus daemon-control release. The original 0.01 local kernel baseline
remains the foundation; the runnable quickstart and 0.02 sprint evidence live in
[`docs/getting-started/local-runtime.md`](docs/getting-started/local-runtime.md)
and [`docs/milestones/0.02-dev/`](docs/milestones/0.02-dev/).

Current capabilities:

- Persistent local runtime with scheduler, loop engine, and resumable state.
- State graph with snapshots backed by SQLite.
- Append-only trace store with integrity chaining and replay support.
- Verifier chain (permission, quota, invariant) and action gateway mediation.
- Filesystem and HTTP adapters with safe defaults.
- CLI tooling: `--version`, `run`, `trace export`, `state head`, and `replay`
  from YAML/JSON config.
- Python SDK runtime with policy/perceptor/constraint hooks and trace subscription.
- PyO3 bindings exposing the Python SDK runtime.
- Typed local messages with inbox/outbox delivery, trace-linked lifecycle events,
  and schema/recipient denial paths.
- Per-agent isolation ledgers for permissions, quotas, allowed message schemas,
  and allowed recipients inside the same tenant.
- Local delegation with explicit parent/child run references and scoped specialist
  authority.
- Local runtime daemon API for run lifecycle, percept append, traces, state-head,
  inspect-only replay, actions, health, and capabilities.
- TypeScript `@splendor/types` and `@splendor/client` packages for daemon control
  without duplicating Rust runtime semantics.
- Multi-agent replay causal graph reconstruction for messages, child run links,
  and isolation denials without re-executing side effects.
- Docker deployment image for the 0.02-dev local runtime surface, including
  `splendorctl`, `splendor-daemon`, the Python SDK, and local smoke-test
  examples.
- Example: `examples/single_agent_loop` with config-driven execution.
- Unit and integration test coverage with CI gates.

## Docker install

Once the GitHub Container Registry package is public, install and verify the
0.02-dev deployment image with:

```bash
docker pull ghcr.io/splendor-os/kernel:0.02-dev
docker run --rm ghcr.io/splendor-os/kernel:0.02-dev
```

Expected shape:

```text
splendorctl 0.1.0 (Splendor0.02-dev)
```

For local builds and smoke tests from a checkout:

```bash
docker build -t splendor:0.02-dev .
bash scripts/container-tests.sh
```

The Docker image is a local runtime deployment artifact. The included daemon
binary remains explicit local-only insecure development mode and must not be
published as an unauthenticated remote TCP service. See
[`docs/deployment/docker.md`](docs/deployment/docker.md) for usage and security
notes.

## Next steps

Planned beyond 0.02-dev:

1. Distributed execution, identity continuity, and fleet telemetry (0.03-dev).
2. Governance workflows such as approval gates, escalation policies, and circuit
   breakers (0.04-dev).
3. Physical/edge orchestration and device safety verifier APIs (0.05-dev).
4. Stable compatibility guarantees and adapter maturity levels (0.1-dev).

## Why

### The problem

Modern OSs standardized primitives (processes, threads, memory, IPC, permissions, scheduling), enabling reliable software at scale.

Autonomous AI systems **lack an equivalent foundation**. “Agents” are commonly assembled as ad-hoc user-space glue:

- model + prompt + tool wrappers
- planner + retry loops
- vector store + memory strategies
- bespoke safety + logging + evaluation

This results in agent systems that are:

- fragmented and inconsistent across teams and machines,
- difficult to verify, reproduce, and debug,
- risky at execution edges (tools/services/devices),
- brittle to scale into **persistent autonomy**.

### Splendor’s thesis

The next “kernel” won’t primarily schedule OS processes. It will schedule **agent loops**:

- **Perceiving** (normalized percepts from sensors/tools)
- **Deciding** (neural generalization + symbolic control)
- **Acting** (verified execution boundaries)
- **Learning** (feedback/reward channels)
- **Coordinating** (messaging, multi-tenancy, fleet scheduling)
- **Remaining constrained** by explicit rules and guarantees

Splendor provides the missing **kernel-level primitives for agents**, so autonomy becomes **stable, auditable, and governable**.

## What

Unless a capability appears in the 0.02-dev "Current capabilities" list above,
roadmap terms in the sections below (remote transport, fleet scheduling,
distributed coordination, governance workflows, kill switches, and broad adapter
ecosystem work) are planned future surfaces and are not available in the 0.02
local multi-agent release.

### What Splendor is

A systems layer that augments modern neural AI systems by enforcing primitives for autonomy, coordination, and long-term evolution.

- **Kernel-grade runtime primitives** for autonomous agents
- **Rust runtime core** for tenancy, state graphs, scheduling, local action
  verification, audit/observability, typed local messaging, per-agent isolation,
  and scoped local delegation. Multi-host execution remains planned for 0.03-dev.
- **Managed interpreters** as first-class compute (e.g., sandboxed Python instances per agent/tenant)
- **Closed-loop autonomy**: percepts → policies → (constraints) → verified actions, with feedback routed back into state/learning
- **Distributed by design**: the primitive model preserves future distributed
  identity and constraints, but multi-host execution is planned for 0.03-dev and
  is not available in 0.01-dev.
- **Boundary-aware safety**: actions are mediated at execution edges before side effects occur

### What Splendor is not

- Not a replacement for Unix / your OS
- Not a bare-metal kernel
- Not a new neural architecture (bring your models)
- Not a single agent framework that dictates how you build (bring your stack)

Splendor **complements** existing agent frameworks and tools by providing the runtime substrate beneath them.

## Core idea: neuro-symbolic by construction

Splendor treats “neuro-symbolic” as a **runtime property**, not an architecture bolt-on.

An agent loop is built from four cooperating parts, each with explicit interfaces and enforcement points:

1. **Neural policies**  
   Decide under uncertainty: map structured percepts to candidate actions using learned representations.

2. **Symbolic structure**  
   Constrain and compose behavior: planners/solvers/rules express allowed actions, invariants, and task decomposition.

3. **Verification at the boundary**  
   Mediate execution: before actions reach tools/services/devices, verification checks enforce safety, permissions, resources, and invariants.

4. **Feedback and rewards**  
   Close the loop: outcomes and evaluations are captured as first-class signals routed back into state and learning interfaces.

**Learning provides generalization. Symbolic structure provides control. Verification provides guarantees. Feedback provides adaptation.**

Splendor’s job is to make these pieces **interoperable and enforceable at runtime** without prescribing a single model, planner, or training method.

## Vision: agents as first-class compute

Operating systems separate **kernel space** (enforced invariants) from **user space** (fast-changing applications).  
Splendor applies this separation to autonomy:

### System space (stable + enforceable)

- Tenancy/isolation
- Resource limits and scheduling
- Action gating + verification and constraint enforcement
- Messaging, audit/observability, governance

### AI space (iterable + experimental)

- Models, policies, planners/solvers, tools
- Reward/evaluation logic
- Memory strategies and domain code
- Rapid iteration without breaking system invariants

**Adapters** sit at the boundary to translate environments into structured percepts, expose actuators/actions, and attach constraints and verification.

## Architecture

### Runs on top of Unix-based systems

Splendor runs in user space and relies on the host OS for:

- drivers and hardware access
- filesystems and process isolation primitives
- networking

### Kernel runtime (Rust core)

Responsibilities:

- **Tenancy** and isolation contexts per agent/tenant
- **State graphs** (explicit state; versioned snapshots; replay)
- **Scheduling** (agent-loop execution policies; fairness; quotas)
- **Messaging** (typed, traceable local message passing; remote transport is
  planned for 0.03-dev)
- **Governance & audit** (0.01 provides append-only traces and reproducibility;
  approval/circuit-breaker workflows are planned for 0.04-dev)
- **Action verification** (pre/post gates; invariants; budgets; permissions)

### Managed compute (Python interfaces)

- Sandboxed Python interpreter instances as managed compute per agent/tenant
- Hosts: model calls, tools, planners, domain code
- Kernel enforces limits and records traces

### Distributed by default *(planned beyond 0.02-dev)*

- Multi-device identity and trust boundaries are planned for 0.03-dev.
- Structured messaging across machines is planned for 0.03-dev.
- Fleet telemetry aggregation is planned for 0.03-dev.
- Constraints and action gates are local-only in 0.02-dev and are designed to
  remain enforceable across future fleet boundaries.

### Core runtime flow:

```
Percepts -> Policy -> Constraints -> Gateway -> Adapter -> Outcomes -> State commit -> Trace
```

### Major components:

- `crates/splendor-kernel`: scheduler, loop engine, tenancy, state graph, runtime tracing.
- `crates/splendor-gateway`: verifier chain and adapter execution pipeline.
- `crates/splendor-store`: Backed state and trace stores.
- `crates/splendor-types`: core schemas and IDs.
- `adapters/filesystem`, `adapters/http`: gated side-effect adapters.
- `crates/splendor-daemon`: local runtime daemon API for run lifecycle, percepts,
  traces, state-head, replay, actions, health, and capabilities.
- `crates/splendorctl`: operational CLI for version, run, trace, state-head,
  and replay workflows.
- `python/splendor`: Python SDK runtime.
- `python/bindings`: PyO3 bindings wrapper.
- `typescript/packages/types`, `typescript/packages/client`: schema-aligned
  TypeScript daemon types and thin client.

## Core Domain Model (Kernel Objects)

### Entities (nouns Splendor standardizes)

- **Tenant**: administrative boundary (quotas, policies, permissions)
- **Agent**: persistent identity + configuration + ownership (tenant)
- **RuntimeContext**: isolated execution container for an agent (limits, interpreter handles)
- **StateGraph**: explicit, versioned state nodes/edges + snapshots
- **Percept**: structured observation/event (schema + payload + provenance)
- **Policy**: maps (state, percept) → candidate actions
- **Constraint**: hard/soft rules/invariants; obligations; allowable sets
- **Plan**: optional decomposition artifact (steps + constraint justification)
- **Action**: proposed side-effectful operation (tool/device/service)
- **Verifier**: gatekeeper enforcing pre/postconditions, permissions, budgets, invariants
- **Feedback**: evaluation outcome (human/automated/env), routed into state/learning
- **Reward**: numeric/structured learning signal (often derived from feedback)
- **Trace**: append-only record of loop decisions, constraint checks, verifications, I/O
- **Message**: typed inter-agent communication artifact (trace-linked)

### Core invariants (non-negotiables)

1. **No side effects without passing a verifier.**
2. **Every loop step emits trace artifacts** (inputs, decisions, constraints, actions, outcomes).
3. **State is explicit and versioned** (snapshot/replay support within allowed nondeterminism).
4. **Tenant quotas and policies apply everywhere** (local + distributed).
5. **Adapters are the only execution boundary** (side effects go through gateways, not bypassed).

## Primitives to Standardize

Splendor’s goal is to make agent-building look less like glue code and more like building on an OS.

### 01 — Perception

- **Perceptors** (sensor + tool observation interfaces)
- **Environment schemas** (what the agent can see)
- Representation/embedding stores (optional module hooks)
- Multi-modal encoder hooks (optional)

### 02 — Policy & Learning

- Pluggable **policy networks** / decision modules
- **Reward functions** + evaluators
- Value estimators / critics (optional)
- **Feedback channels** (human, automated, environment-derived)

### 03 — Reasoning & Constraints

- Constraint solvers (hard/soft constraints)
- Planners (symbolic / hybrid)
- Rules and invariants (“never do X”, “always require Y”)
- Proof/trace artifacts where feasible

### 04 — Execution

- Actuators / tool interfaces (structured)
- State machines (structured control)
- Action verifiers (pre/post-conditions)
- Rollback / compensation patterns

### 05 — Safety & Governance

- Guardrails as enforceable runtime objects (not just prompts)
- Alignment signals (telemetry + reward shaping hooks)
- Kill switches / circuit breakers
- Audit logs and reproducibility primitives

### 06 — Coordination & Distributed Systems

- Typed, traceable message passing
- Shared-state and consensus mechanisms (optional modules)
- Resource allocation / scheduling (agent-aware)
- Multi-device identity, permissions, and trust boundaries

## Interfaces

### Design rule

**Python can propose; Rust enforces.**

### Rust kernel API (internal stability surface)

Stable traits/interfaces for:

- `Perceptor`
- `PolicyHost` / `DecisionProvider`
- `ConstraintEngine`
- `ActionGateway`
- `Verifier`
- `StateStore` (state graph + snapshots)
- `TraceStore` (append-only)
- `LocalMessageRouter` / `MessageBus` (local in-process routing implemented for
  0.02-dev; remote transport planned for 0.03-dev)
- `Scheduler`
- `GovernancePolicy` (0.01 covers tenancy, quotas, and permissions; kill switch
  is planned for governance/fleet milestones)

### Python SDK (public ergonomics surface)

Expose:

- Define agent loops and policies (callbacks/plugins)
- Register perceptors, actions, constraints, verifiers
- Launch/stop/restart persistent agents
- Subscribe to trace/feedback streams
- Provide adapter authoring kits (safe defaults)

## Repository Blueprint (Monorepo)

- `crates/`
  - `splendor-kernel/` — scheduler, tenancy, state graph, tracing, governance hooks
  - `splendor-gateway/` — action mediation, verifier pipeline, compensation hooks
  - `splendor-store/` — state/trace stores (traits + implementations)
  - `splendor-net/` — planned distributed messaging, identity, transport backends
  - `splendor-policy/` — constraints model + evaluation integration points
- `python/`
  - `splendor/` — Python SDK
  - `bindings/` — Rust↔Python bridge
- `adapters/`
  - `filesystem/`, `http/` — 0.01 gated side-effect adapters
  - `shell/`, `db`, `llm/` — planned adapters (not “in-kernel logic”)
- `examples/`
  - `single_agent_loop/`
  - `local-basic-loop/`
  - `replay-local-run/`
  - `python-sdk-basic/`
  - `local-specialist-scoped-delegation/`, `local-orchestrator-specialists/`,
    `local-multi-agent-replay/`, `daemon-client-local/`,
    `typescript-daemon-client/` — 0.02 local multi-agent and daemon examples
  - `verified_tools/` — planned for later milestones
- `docs/`
  - `concepts/` — system space vs AI space, primitives, neuro-symbolic runtime property
  - `reference/` — schemas, APIs, versioning
  - `guides/` — adapters, verifiers, constraints, operations
  - `rfc/` — design proposals and primitive evolution
- `.github/` — CI, issue templates, PR templates
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `GOVERNANCE.md`

## MVP Definition

**Goal:** prove the thesis with the smallest coherent system: persistent local agent loops with verified execution and reproducible traces.

### MVP scope

1. **Single-machine kernel runtime**
   - Agent loop scheduler
   - Tenant isolation (logical contexts + quotas)
   - Explicit state graph + snapshots
   - Trace/audit log (append-only)

2. **Action gateway with verification**
   - Minimal verifier chain:
     - permission checks
     - budget/quota checks
     - invariant checks
   - Example safe adapters:
     - filesystem (restricted sandboxed ops)
     - HTTP client (allowlist + rate limits)

3. **Python SDK**
   - Define agent loop (policy callback)
   - Register perceptors/actions/verifiers/constraints
   - Run agent persistently (restartable)

4. **Reproducibility**
   - Replay mode from traces + state snapshots (best-effort determinism)
   - Deterministic serialization of percept/action/constraint objects

### MVP non-goals

- Full fleet orchestration across hosts
- Complex consensus/shared-state systems
- End-to-end RL training pipelines inside the kernel
- A single mandated agent framework

## Roadmap

- **Splendor0.01-dev:** local runtime + gateway + Python SDK + trace + state graph + replay
- **Splendor0.02-dev:** local multi-agent runtime + daemon control, including typed messages, per-agent isolation, local delegation, runtime daemon API, TypeScript types/client, and multi-agent replay
- **Splendor0.03-dev:** multi-host distributed execution; identity continuity; fleet telemetry aggregation
- **Splendor0.04-dev:** governance workflows (approval gates, escalation policies, circuit breakers)
- **Splendor0.1-dev:** stable primitives spec + compatibility guarantees + adapter ecosystem maturity

## Docs Model

Docs should mirror the mental model and keep the primitive surface stable.

1. **Concepts**
   - What Splendor is (runtime kernel for agent loops)
   - System space vs AI space
   - Neuro-symbolic “runtime property”
2. **Primitives Reference**
   - Percept schema
   - Action schema
   - Constraint schema
   - Trace schema
   - Message schema
3. **Guides**
   - Build a perceptor
   - Build a verifier
   - Build an adapter
   - Run persistent agents
   - Replay, debugging, and audit
4. **Operations**
   - Tenancy, quotas, governance, kill switches
   - Deploying on one machine vs fleet
5. **RFC process**
   - Any primitive change requires RFC + migration plan + versioning rules

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

SPDX-License-Identifier: Apache-2.0 OR MIT

## Contribution, Community & Governance

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be licensed under the terms of both the
Apache License, Version 2.0 and the MIT license, without any additional terms
or conditions.

### Contribution shape

- RFCs for primitives: keep the “kernel contract” stable
- Working groups:
  - Runtime + scheduling
  - Verification + policy
  - Distributed coordination
  - Python SDK + developer experience
  - Adapters ecosystem

### Governance baseline

- Maintainer model (core maintainers + WG leads)
- Security response policy (`SECURITY.md`)
- Compatibility promise: primitive versioning + deprecation windows

### Desired community outcomes

- Shared adapter ecosystem: perceptors/actuators/verifiers that interoperate
- Reusable constraint packs (e.g., safe filesystem ops, PII handling, prod DB gates)
- Trace-based reproducibility as the default debugging and review workflow
