# Kernel E2E Integration Criteria Through 0.03

**Date:** 2026-05-27  
**Status:** Required verification rule pack for the 0.03 final gate. This file defines what the end-to-end tests must prove; it does not by itself claim the current implementation already passes.  
**Milestone coverage:** Splendor0.01-dev, Splendor0.02-dev, Splendor0.03-dev  
**Sprint coverage:** 0.01-H1..H4, 0.02-S0..S7, 0.03-S1..S8  
**FR coverage:** FR-0.01-01..FR-0.01-07, FR-0.02-01..FR-0.02-10, FR-0.03-01..FR-0.03-11

This rule pack exists because unit tests and sprint-local tests are not enough to
prove Splendor's kernel behavior. A 0.03 final claim requires realistic end-to-end
evidence that the runtime loop, daemon boundary, local multi-agent model,
resident-node/fleet primitives, remote messaging, trace aggregation, state
handoff, replay, and telemetry work together without weakening enforcement.

The governed loop remains the test spine:

```text
Percepts -> Policy -> Constraints -> Gateway -> Verifiers -> Adapter -> Outcome -> State Commit -> Trace
```

---

## 1. Definition of a real kernel E2E test

A test is a **real kernel E2E test** only if it satisfies all of the following:

1. It exercises the public runtime, daemon, SDK/client, or documented kernel APIs
   that a real caller would use for the scenario.
2. It uses the real action gateway and verifier chain for every action request.
3. It persists or inspects real state graph nodes and trace events produced by the
   runtime path under test.
4. It proves at least one positive path and at least one denial or fail-closed path.
5. It proves replay behavior without re-executing unsafe side effects.
6. It preserves tenant, agent, run, tick, action, state, trace, message, work-order,
   fleet, node, and instance identity separation where those identities apply.
7. It uses deterministic fixtures so a clean checkout can reproduce the same result.

The following are **not** sufficient for this gate:

- unit-only tests for constructors or pure validation helpers;
- tests that mock the action gateway, verifier chain, trace store, or state store
  while claiming runtime correctness;
- examples that only print expected data without assertions;
- tests that prove only a success path;
- tests that use telemetry as authority for runtime permissions;
- replay tests that call adapters for side-effectful actions by default;
- daemon tests that bypass caller identity, endpoint scopes, or work-order rules;
- remote-message tests that mutate remote state directly instead of routing a typed
  message through the documented boundary.

Deterministic in-memory stores, loopback transports, fixture adapters, and local
resident-node harnesses are allowed when they keep the same kernel contracts and do
not bypass enforcement.

---

## 2. Non-goals for the 0.03 E2E gate

The 0.03 E2E suite must not pull later milestone scope forward.

- No governance approval workflow engine from 0.04.
- No circuit breaker, kill-switch, or policy distribution implementation from 0.04,
  except where a 0.03 test needs to prove a placeholder remains non-authoritative.
- No physical/edge safety verifier or robotics adapter contract from 0.05.
- No distributed consensus, global exactly-once guarantee, or arbitrary shared
  mutable distributed memory.
- No enterprise SaaS UI, dashboard, billing, marketplace, or product-specific
  Harmony dependency.
- No production PKI/OAuth rollout beyond the caller/work-order validation rules
  already required by 0.02-S0 and 0.03-S3.

---

## 3. Required suite entry points

The final 0.03 E2E suite must expose one stable aggregate command:

```bash
bash scripts/verify-0.03-kernel-e2e.sh
```

Until that script exists, reviewers must reproduce the same coverage with the
underlying commands documented in
`docs/development/kernel-e2e-integration-tests.md`.

The aggregate command must produce a machine-readable evidence report at:

```text
target/splendor-e2e/0.03-kernel-e2e-report.json
```

The report must include:

- repository revision or equivalent source identifier;
- commands executed;
- test IDs and pass/fail status;
- FR IDs covered by each test;
- trace event IDs and run IDs used as evidence;
- final state node IDs and state hashes for successful runs;
- denial/failure reason codes for negative paths;
- replay mode used and whether any adapter execution was suppressed;
- artifact paths for exported traces, state snapshots, causal graphs, and telemetry
  snapshots.

If the report cannot be produced, the 0.03 E2E gate is incomplete even when the
individual commands pass.

---

## 4. Required E2E scenarios

Each scenario below is mandatory. A scenario may be implemented as one integration
test file or as a tightly linked group of tests, but the evidence report must keep
the scenario ID stable.

### K-E2E-001 — Local kernel loop, gateway, state, trace, and replay

**Primary FRs:** FR-0.01-01, FR-0.01-02, FR-0.01-03, FR-0.01-04, FR-0.01-05  
**Primitives:** percept, policy, constraint, gateway, verifier, adapter, quota,
state graph, trace store, replay

**Use case:** A single local agent receives a percept, policy proposes one allowed
fixture action and one denied action, the gateway verifies both, the allowed action
executes through a fixture adapter, state commits, traces are appended, and replay
reconstructs the run without re-executing the side effect.

**Required positive evidence:**

- tenant, agent, run, tick, and action IDs are present and distinct;
- the allowed action reaches the adapter only after verification completes;
- outcome is recorded before the state commit is accepted;
- final state head references the state node emitted in `state.committed`;
- the trace contains the minimum tick sequence in order:
  `tick.started`, `percepts.received`, `state.loaded`, `policy.invoked`,
  `policy.completed`, `actions.proposed`, `constraints.evaluated`,
  `verification.started`, `verification.completed`, action outcome,
  `outcome.recorded`, `state.committed`, `tick.completed`.
- Python SDK hooks for policy, percept submission or perceptor behavior,
  constraints/verifiers, adapter proposal/execution through the gateway, trace
  subscription, and replay exercise the same kernel path or map to equivalent
  passing SDK integration evidence.
- CLI workflows execute a run, export traces, and request replay using documented
  commands.

**Required negative/failure evidence:**

- denied action does not reach the adapter;
- missing permission or quota exhaustion fails closed;
- verifier unavailable fails closed;
- failed state commit prevents a next tick from starting;
- trace persistence failure fails closed for side-effectful actions.

**Replay evidence:**

- replay reconstructs percepts, policy output, verifier decisions, outcomes, state
  commit, and trace order;
- replay suppresses the side-effectful adapter execution by default;
- corrupted or missing trace segment fails replay with a structured error.

**Why this is complete:** It proves the local kernel loop and the non-negotiable
runtime invariants that every later distributed feature depends on.

---

### K-E2E-002 — Daemon/client boundary with caller identity and signed work order

**Primary FRs:** FR-0.02-08, FR-0.02-09; 0.02-S0 criteria  
**Primitives:** SDK/API, work order, run, percept, trace store, state graph,
replay, gateway

**Use case:** A client uses the local daemon boundary to create a run from a signed
work order, append a percept, start/pause/resume/stop or complete the run, read
state head, read ordered traces, and request replay.

**Required positive evidence:**

- non-dev daemon calls include authenticated caller identity, endpoint scope,
  tenant or fleet binding, audience binding, expiry, and audit attribution;
- run creation requires a signed work order and records the work-order ID in run
  metadata and trace/audit metadata;
- daemon percept append has the same runtime effect as CLI/SDK percept ingestion;
- trace streaming or paging preserves order;
- state-head response names a state node that exists in the state graph;
- TypeScript client types and daemon response schemas remain compatible.

**Required negative/failure evidence:**

- anonymous non-dev caller is rejected;
- missing endpoint scope is rejected;
- wrong tenant/fleet binding is rejected;
- wrong audience is rejected;
- expired or revoked caller credential is rejected;
- unsigned, expired, revoked, malformed, or incompatible work order creates no run;
- malformed percept is rejected with structured error;
- action submission without trace linkage or gateway verification is rejected;
- SDK/client cannot silently fall back to insecure unauthenticated communication.

**Replay evidence:**

- daemon replay endpoint starts inspect-only replay;
- replay does not call side-effectful adapters;
- trace/audit attribution for the replay request is visible where the API mutates
  replay job state.

**Why this is complete:** It proves external control cannot become a back door
around work-order authority, daemon scopes, gateway mediation, trace, or replay.

---

### K-E2E-003 — Local multi-agent delegation, messages, quotas, and replay

**Primary FRs:** FR-0.02-01, FR-0.02-02, FR-0.02-03, FR-0.02-04,
FR-0.02-05, FR-0.02-06, FR-0.02-07, FR-0.02-10  
**Primitives:** message, runtime context, quota, verifier, gateway, trace store,
state graph, replay

**Use case:** A local orchestrator agent delegates scoped work to at least two
specialist agents in the same Splendor instance. Messages are typed, trace-linked,
stored in per-agent inbox/outbox state, and replay reconstructs causality.

**Required positive evidence:**

- canonical `Message` includes message ID, source agent, target agent, run ID,
  schema, payload, causal parent, response requirement, and timestamp;
- parent run creates child runs only with explicit target agents, scoped objectives,
  and delegated permissions;
- each specialist has separate state head, trace stream, quota ledger, and allowed
  action/message scope;
- message queued, delivered, consumed, and response events include trace IDs,
  message IDs, source/target agents, and run IDs;
- deterministic ordering is preserved within each source-target-run stream.

**Required negative/failure evidence:**

- unknown target agent causes deterministic rejection and trace emission;
- invalid message schema or payload is rejected before delivery;
- agent A cannot execute with agent B permissions;
- shared specialist receives only explicitly delegated permissions;
- one agent's quota exhaustion does not exhaust or reset another agent ledger;
- parent cancellation prevents new child delegation and emits trace events;
- failed child run propagates structured failure to parent.

**Replay evidence:**

- replay reconstructs message queued, delivered, consumed, rejected, and expired
  events;
- replay reconstructs parent/child run references and causal graph;
- permission-laundering denial appears with verifier or ledger reason;
- replay does not execute child side effects.

**Why this is complete:** It proves 0.02 local multi-agent semantics before 0.03
wraps messages for remote delivery.

---

### K-E2E-004 — Resident node registration, capability matching, and work-order dispatch

**Primary FRs:** FR-0.03-01, FR-0.03-02, FR-0.03-03, FR-0.03-04,
FR-0.03-05, FR-0.03-06  
**Primitives:** fleet identity, node registry, instance registry, capability,
work order, placement, trace/audit

**Use case:** A minimal central manager registers at least two resident nodes and
instances with different capabilities, receives a scoped signed work order, selects
a valid target using placement v0, dispatches the work order, and rejects invalid
or incompatible work orders before runtime execution.

**Required positive evidence:**

- fleet, node, instance, tenant, agent, run, tick, action, state, trace, message,
  and work-order IDs are distinct and stable in serialized evidence;
- node registration includes node ID, kind, tenant/fleet scope, capabilities,
  constraints, runtime version, and health metadata;
- instance registration includes instance ID, runtime mode, hosted tenants, and
  supported features;
- heartbeat updates health without overwriting static registration fields;
- placement decision includes target, reasons, dedicated-instance flag, required
  capabilities, and data locality when relevant;
- signed compatible work order starts or queues exactly one run on the selected
  instance;
- work-order ID appears in run metadata and trace/audit output.

**Required negative/failure evidence:**

- invalid capability documents are rejected before registration;
- stale heartbeat detection is deterministic;
- unsigned, expired, revoked, malformed, or incompatible work orders are rejected
  with no run, percept, policy, or adapter execution;
- unavailable capability, incompatible runtime, wrong data locality, or missing
  dedicated-instance requirement causes explicit placement rejection or pending
  status;
- physical target request cannot be placed on a generic cloud node unless marked
  as simulation/helper;
- placement never widens permissions.

**Replay evidence:**

- replay or audit inspection can explain registration, placement, work-order
  acceptance/rejection, and dispatch decisions without starting a new run.

**Why this is complete:** It proves resident/fleet execution begins from explicit
identity, capability, placement, and work-order authority rather than ambient
daemon or scheduler trust.

---

### K-E2E-005 — Cross-instance typed message transport and failure visibility

**Primary FRs:** FR-0.03-08, FR-0.03-10; preserves FR-0.02-01..04  
**Primitives:** message, remote transport, work order, trace store, identity,
replay

**Use case:** Two Splendor instances exchange a typed task request/response using a
remote envelope that wraps the canonical message without changing its payload. The
receiver validates identity, schema, run/work-order authority, and target agent
before delivery.

**Required positive evidence:**

- remote envelope preserves the canonical message fields and adds only transport
  metadata;
- receiver validates source/target identity, tenant/run/work-order authority,
  schema version, and target agent before delivery;
- send, accept, deliver, consume, and response events are trace-linked on both
  source and receiver sides;
- source and receiver traces can be correlated by message ID, causal parent, run ID,
  and work-order ID.

**Required negative/failure evidence:**

- duplicate remote message is detected by message ID and handled deterministically;
- invalid schema, mismatched tenant/run, unknown target agent, or missing authority
  rejects delivery before policy or adapter execution on the receiver;
- transport timeout/failure records trace/audit evidence and does not silently drop
  runtime state;
- retry occurs only when configured and safe for the message semantics.

**Replay evidence:**

- replay reconstructs send and receive sides with causal linkage;
- replay does not retry transport or execute target-side adapters.

**Why this is complete:** It proves distributed communication is explicit and
traceable without promising global exactly-once delivery or shared mutable state.

---

### K-E2E-006 — Trace aggregation, interruption, state handoff, and resume

**Primary FRs:** FR-0.03-07, FR-0.03-09, FR-0.03-10  
**Primitives:** trace store, central trace index, state graph, state handoff,
runtime context, replay

**Use case:** A run starts on instance A, commits state, buffers traces, is marked
interrupted, exports an explicit state snapshot/reference, syncs traces to a
central index, and resumes on instance B only after snapshot hash, ownership,
authority, and trace continuity are validated.

**Required positive evidence:**

- local trace buffer sync preserves event order within a run;
- duplicate trace sync attempts do not create duplicate central events;
- central trace index can query by fleet, node, instance, tenant, agent, run, tick,
  action, and work order where available;
- exported state handoff includes state head, parent linkage, snapshot/ref,
  integrity hash, owner agent/run/tenant, work-order authority, and trace linkage;
- receiver imports only after hash verification and authority validation;
- source and receiver emit trace events for handoff boundary and resume;
- resumed run continues from the validated state head without hidden shared state.

**Required negative/failure evidence:**

- missing trace segment is detected and reported;
- corrupted trace chain or mismatched run identity causes sync rejection or
  quarantine;
- trace sync failure does not permit side-effectful actions when local policy
  requires trace durability;
- mismatched owner, stale head, corrupted snapshot, missing trace, or invalid
  work-order authority fails import;
- failed import leaves receiver state unchanged;
- read-only state reference cannot be mutated by the receiver.

**Replay evidence:**

- replay identifies interruption, handoff boundary, previous state head, import
  decision, and resumed state head;
- replay does not re-sync traces, re-import snapshots as mutation, or execute
  adapters.

**Why this is complete:** It proves 0.03 distributed state and trace continuity
without introducing hidden shared memory or an unbounded migration engine.

---

### K-E2E-007 — Fleet telemetry is complete, identity-linked, and non-authoritative

**Primary FRs:** FR-0.03-06, FR-0.03-07, FR-0.03-11  
**Primitives:** fleet telemetry, node identity, instance identity, quota,
verifier, trace sync, run status

**Use case:** After the preceding scenarios, the minimal telemetry collector reports
node health, instance runtime metadata, canonical run statuses, quotas, denial
signals, trace-sync lag/failure, and failure categories for the participating
fleet.

**Required positive evidence:**

- telemetry reports node online, stale, and offline states from heartbeat data;
- telemetry reports instance runtime version, mode, capabilities, and current run
  counts;
- run status vocabulary is exactly: `pending`, `running`, `paused`,
  `waiting_for_approval`, `interrupted`, `resuming`, `completed`, `failed`,
  `cancelled`, `denied`, `expired`;
- quota and denial signals include tenant, agent, run, verifier, reason, and
  trace/event linkage where applicable;
- trace sync lag or failure is visible per node/instance;
- failure categories are stable and documented.

**Required negative/failure evidence:**

- stale/offline thresholds are deterministic;
- malformed telemetry reports are rejected or quarantined;
- telemetry cannot be used as hidden authority for placement, runtime permission,
  gateway allow/deny, verifier result, work-order acceptance, or adapter execution.

**Replay evidence:**

- replay or audit inspection can reconstruct telemetry facts from source reports
  and trace-linked runtime facts;
- replay does not send heartbeats, retry trace sync, or mutate telemetry state.

**Why this is complete:** It proves the operational surface reflects runtime facts
without becoming a new authority path.

---

### K-E2E-008 — Final 0.03 cross-primitive journey

**Primary FRs:** all FR-0.01, FR-0.02, and FR-0.03 requirements  
**Primitives:** all primitives covered by this file

**Use case:** A single scenario ties the prior scenarios together:

1. A caller with scoped daemon credentials submits a signed work order through the
   OpenAPI-described daemon API.
2. A central manager validates the work order, chooses a resident node by placement
   v0, and dispatches the run.
3. The selected instance starts a local orchestrator agent.
4. The orchestrator delegates to a local specialist with narrower permissions.
5. The orchestrator sends a typed remote message to a second instance.
6. The remote instance validates message authority and returns a typed response.
7. One allowed action executes through the gateway and one disallowed action is
   denied before adapter execution.
8. The run commits state and emits ordered traces.
9. The run is interrupted, state handoff is exported, traces sync to the central
   index, and another compatible instance resumes from the validated state.
10. Replay reconstructs the full causal graph and suppresses side effects.
11. Fleet telemetry reports health, run status, quota/denial, trace sync, and
   failure signals without becoming authority.

**Required evidence:**

- every previous scenario has a passing evidence row;
- the final journey exports a causal graph including trace event IDs, message IDs,
  parent/child run IDs, state node IDs, and work-order ID;
- no side effect bypasses the gateway;
- no verifier uncertainty silently allows execution;
- no permission laundering occurs through local or remote specialists;
- no replay side effect occurs;
- no telemetry snapshot is consulted as an allow/deny authority;
- OpenAPI request/response validation passes for daemon API calls in the journey.

**Why this is complete:** It proves the integrated 0.03 kernel surface rather than
only isolated primitives.

---

### K-E2E-009 — Data-local finance report on a resident VPC node

**Primary FRs:** FR-0.01-01..07, FR-0.02-08, FR-0.02-09,
FR-0.03-01..07, FR-0.03-09, FR-0.03-11  
**Primitives:** work order, placement, capability, data reference, gateway,
adapter, trace store, state graph, replay, telemetry

**Use case:** A finance reporting caller submits a signed work order for a weekly
dashboard using a scoped `dataset:finance.revenue_monthly_v4` data reference. A
central manager fixture must place the run on a resident customer-VPC or on-prem
node with the required data-local capability, execute a read-only query fixture and
artifact-create fixture through the gateway, commit report state, sync traces, and
expose telemetry.

**Required positive evidence:**

- work order lists data refs, allowed actions, allowed adapters, allowed
  permissions, quota limits, placement target, locality, expiry, and signature;
- placement selects the only node advertising the required data-local capability;
- read-only data action and artifact creation both pass gateway verification before
  adapter execution;
- final state references report inputs, artifact reference, trace range, and
  work-order ID;
- trace export and replay explain the query/artifact actions without re-executing
  side effects;
- telemetry reports the run, quota usage, trace-sync status, and node health.

**Required negative/failure evidence:**

- generic cloud node placement is rejected for the data-local work order;
- broader dataset access outside the work-order data refs is denied before adapter
  execution;
- missing finance permission, quota exhaustion, expired work order, or revoked work
  order creates no unauthorized side effect;
- artifact publish or external mutation is denied because it is not in the 0.03
  work-order scope.

**Why this is complete:** It is a realistic digital kernel use case that exercises
signed work-order authority, data locality, placement, gateway checks, state,
trace, replay, and telemetry without adding enterprise product features.

---

### K-E2E-010 — Shared document specialist without cross-tenant leakage

**Primary FRs:** FR-0.02-01..07, FR-0.02-10, FR-0.03-01,
FR-0.03-04, FR-0.03-08, FR-0.03-10, FR-0.03-11  
**Primitives:** message, local delegation, remote message, work order, quota,
permission ledger, trace store, replay, telemetry

**Use case:** Tenant A and tenant B both delegate document parsing to the same
shared specialist agent. Each caller provides a scoped work order with one document
reference. The specialist may parse only the referenced document and must not see,
read, trace, or return data belonging to the other tenant.

**Required positive evidence:**

- two tenant-scoped work orders create separate runs, quotas, state heads, trace
  streams, and message graphs;
- the shared specialist receives explicit delegated data refs and permissions per
  work order;
- local or remote task messages preserve source/target agent, run, schema, payload,
  causal parent, and timestamp;
- replay reconstructs both tenant causal graphs independently;
- telemetry can report both runs without exposing cross-tenant trace/state data.

**Required negative/failure evidence:**

- tenant A cannot read tenant B document ref, state head, trace stream, messages, or
  telemetry details beyond authorized aggregate status;
- the shared specialist cannot retain or reuse tenant A delegated permissions when
  servicing tenant B;
- disallowed message schema, wrong tenant binding, or quota exhaustion is denied and
  trace-linked.

**Why this is complete:** It proves shared specialists remain useful without
permission laundering, cross-tenant leakage, hidden state, or trace confusion.

---

### K-E2E-011 — Remote analysis helper returns a proposal, not authority

**Primary FRs:** FR-0.02-01..07, FR-0.02-10, FR-0.03-01,
FR-0.03-04, FR-0.03-08, FR-0.03-10  
**Primitives:** remote message, work order, gateway, verifier, state graph, trace
store, replay

**Use case:** An origin orchestrator asks a remote analysis helper to compute a
route, query plan, report outline, or analysis proposal. The helper returns a typed
proposal artifact/reference. Only the origin run may decide whether to submit a
gateway-mediated action using that proposal.

**Required positive evidence:**

- origin sends a typed remote task request with scoped input refs and no origin
  adapter authority;
- remote helper validates message/work-order authority and commits only helper-owned
  state;
- helper response is a typed proposal message or artifact reference, not an action
  execution claim;
- origin verifies the proposal and executes any follow-up action locally through its
  own gateway/verifier chain;
- traces correlate request, helper state commit, proposal response, origin
  verification, and origin outcome.

**Required negative/failure evidence:**

- remote helper cannot execute origin adapters, mutate origin state, or consume
  origin quotas directly;
- wrong work-order scope, duplicate message, missing data ref, or target-agent
  mismatch rejects the helper request before policy/adapter execution;
- replay reconstructs the helper proposal but does not resend remote messages or
  execute follow-up adapters.

**Why this is complete:** It proves distributed helper compute can collaborate
without receiving direct action authority.

---

### K-E2E-012 — Placement fallback under stale node and capability mismatch

**Primary FRs:** FR-0.03-01, FR-0.03-02, FR-0.03-03, FR-0.03-05,
FR-0.03-06, FR-0.03-11  
**Primitives:** node registry, heartbeat, capability, placement, fleet telemetry,
work order

**Use case:** A work order can run on any resident node with `artifact.render` and
`eu-west` locality. Node A has the capability but becomes stale, node B is healthy
but lacks the capability, and node C is healthy and compatible. Placement must
choose C or reject with explicit reasons if C is unavailable.

**Required positive evidence:**

- heartbeat age deterministically marks node A stale/offline;
- capability matching explains why node B is incompatible;
- placement chooses node C with reasons, required capabilities, dedicated-instance
  flag, and data locality preserved;
- telemetry reports stale, incompatible, and selected nodes without becoming an
  authorization source.

**Required negative/failure evidence:**

- invalid heartbeat or capability document is rejected/quarantined;
- if all compatible nodes are stale or missing capabilities, placement returns
  pending/rejected with explicit reasons and starts no run;
- telemetry status alone cannot grant permissions, override work-order scope, or
  force placement.

**Why this is complete:** It proves placement and telemetry compose in a real fleet
condition while keeping telemetry observational.

---

### K-E2E-013 — Read-only state reference collaboration

**Primary FRs:** FR-0.01-04, FR-0.01-05, FR-0.02-01..04,
FR-0.03-01, FR-0.03-08, FR-0.03-09, FR-0.03-10  
**Primitives:** state graph, state handoff, read-only reference, remote message,
trace store, replay

**Use case:** Instance A owns a run state head and sends a read-only state reference
to a remote specialist so the specialist can inspect context and return a typed
recommendation. The specialist must not mutate the origin state or advance the
origin state head.

**Required positive evidence:**

- exported read-only reference includes owner tenant/agent/run, state node ID,
  state hash, snapshot/ref, trace linkage, and expiry or scope;
- receiver validates authority and can inspect only allowed fields;
- receiver response is a typed message with recommendation/proposal output;
- origin performs any state mutation through a new explicit state commit;
- replay identifies the read-only reference boundary and both state heads.

**Required negative/failure evidence:**

- receiver mutation attempt is denied and leaves origin state unchanged;
- stale head, corrupted hash, missing trace, wrong tenant/run, or expired reference
  rejects inspection;
- read-only reference cannot be converted into state ownership or write authority.

**Why this is complete:** It proves distributed collaboration can use explicit
state references without shared mutable memory.

---

### K-E2E-014 — Adapter failure and safe retry boundaries

**Primary FRs:** FR-0.01-02, FR-0.01-03, FR-0.01-04, FR-0.01-05,
FR-0.02-08, FR-0.03-10, FR-0.03-11  
**Primitives:** gateway, verifier, adapter, outcome, state graph, trace store,
replay, telemetry

**Use case:** A work order allows an idempotent fixture action and a non-idempotent
fixture action. The idempotent action fails once and is retried only when marked
safe; the non-idempotent action fails and must not be retried blindly.

**Required positive evidence:**

- both actions reach adapters only after gateway verification;
- adapter failure creates `action.failed` and `outcome.recorded` trace evidence;
- state behavior is explicit for failure and retry outcomes;
- idempotent retry consumes quota predictably and is trace-linked;
- telemetry reports failure category, run status, quota/denial signals, and trace
  sync status.

**Required negative/failure evidence:**

- non-idempotent retry is denied unless a new scoped work-order/action request marks
  it safe;
- verifier uncertainty during retry fails closed;
- replay explains failure/retry decisions without executing adapters.

**Why this is complete:** It proves realistic runtime failure recovery without
unsafe side-effect repetition.

---

### K-E2E-015 — OpenAPI daemon contract and API-client E2E coverage

**Primary FRs:** FR-0.02-08, FR-0.02-09; supports FR-0.03-04,
FR-0.03-05, FR-0.03-07, FR-0.03-10  
**Primitives:** daemon API, SDK/API, work order, gateway, trace store, state graph,
replay, OpenAPI contract

**Use case:** The runtime daemon's exposed API contract in
`openapi/splendor-runtime-daemon.yaml` is treated as an executable contract. An
OpenAPI-derived or OpenAPI-validated client drives the daemon workflow instead of
calling internal Rust helpers directly.

**Required positive evidence:**

- OpenAPI document parses as OpenAPI 3.1 and exposes the documented operations:
  `createRun`, `inspectRun`, `startRun`, `pauseRun`, `resumeRun`, `stopRun`,
  `appendPercept`, `getStateHead`, `getRunTraces`, `replayRun`, `submitAction`,
  `getHealth`, and `getCapabilities`;
- every request and response used by K-E2E-002 validates against the OpenAPI schema;
- the client path uses HTTP-shaped daemon requests and the documented paths, not
  private runtime calls or Rust-only constructors;
- TypeScript client request/response types are generated from, or parity-checked
  against, the OpenAPI schemas;
- OpenAPI schemas are parity-checked against canonical Splendor contracts for run
  status vocabulary, endpoint scopes, caller credential fields, work-order
  authorization fields, action outcome statuses, trace record identity/linkage, and
  required trace redaction parameters;
- trace read requires the OpenAPI `redaction_policy` parameter;
- OpenAPI version, operation IDs used, schema validation result, client package
  version, and daemon version appear in the evidence report.

**Required negative/failure evidence:**

- undocumented daemon path, wrong HTTP method, missing required parameter, malformed
  body, wrong response status, schema mismatch, or mismatch with canonical Splendor
  primitive vocabulary fails the contract test;
- `POST /actions` cannot be used to self-attest gateway completion or bypass runtime
  gateway verification;
- OpenAPI `health` and `capabilities` responses cannot authorize runs, actions,
  work orders, placement, or telemetry decisions;
- local-dev server URL remains explicit and cannot imply unauthenticated production
  TCP exposure.

**Replay evidence:**

- OpenAPI-driven `replayRun` requests create inspect-only replay evidence and never
  execute side-effectful adapters.

**Why this is complete:** It proves the public daemon API contract is the tested
integration boundary for clients and control planes, not an unverified document that
can drift from runtime behavior.

---

## 5. FR-to-scenario coverage matrix

| FR | Required E2E scenario evidence |
| --- | --- |
| FR-0.01-01 | K-E2E-001, K-E2E-008, K-E2E-009 |
| FR-0.01-02 | K-E2E-001, K-E2E-008, K-E2E-009, K-E2E-014 |
| FR-0.01-03 | K-E2E-001, K-E2E-003, K-E2E-008, K-E2E-009, K-E2E-014 |
| FR-0.01-04 | K-E2E-001, K-E2E-006, K-E2E-008, K-E2E-009, K-E2E-013, K-E2E-014 |
| FR-0.01-05 | K-E2E-001, K-E2E-006, K-E2E-008, K-E2E-009, K-E2E-013, K-E2E-014 |
| FR-0.01-06 | K-E2E-001, K-E2E-002 |
| FR-0.01-07 | K-E2E-001 and reproduction commands in the development guide |
| FR-0.02-01 | K-E2E-003, K-E2E-005, K-E2E-008, K-E2E-010, K-E2E-011, K-E2E-013 |
| FR-0.02-02 | K-E2E-003, K-E2E-008, K-E2E-010 |
| FR-0.02-03 | K-E2E-003, K-E2E-008, K-E2E-010 |
| FR-0.02-04 | K-E2E-003, K-E2E-005, K-E2E-008, K-E2E-010, K-E2E-011, K-E2E-013 |
| FR-0.02-05 | K-E2E-003, K-E2E-008, K-E2E-010 |
| FR-0.02-06 | K-E2E-003, K-E2E-008, K-E2E-010 |
| FR-0.02-07 | K-E2E-003, K-E2E-008, K-E2E-010, K-E2E-011 |
| FR-0.02-08 | K-E2E-002, K-E2E-008, K-E2E-009, K-E2E-014, K-E2E-015 |
| FR-0.02-09 | K-E2E-002, K-E2E-015, and TypeScript commands in the development guide |
| FR-0.02-10 | K-E2E-003, K-E2E-005, K-E2E-008, K-E2E-010, K-E2E-011, K-E2E-013 |
| FR-0.03-01 | K-E2E-004, K-E2E-005, K-E2E-006, K-E2E-008, K-E2E-009, K-E2E-010, K-E2E-011, K-E2E-012, K-E2E-013 |
| FR-0.03-02 | K-E2E-004, K-E2E-007, K-E2E-008, K-E2E-012 |
| FR-0.03-03 | K-E2E-004, K-E2E-007, K-E2E-008, K-E2E-009, K-E2E-012 |
| FR-0.03-04 | K-E2E-004, K-E2E-008, K-E2E-009, K-E2E-010, K-E2E-011, K-E2E-015 |
| FR-0.03-05 | K-E2E-004, K-E2E-008, K-E2E-009, K-E2E-012, K-E2E-015 |
| FR-0.03-06 | K-E2E-004, K-E2E-007, K-E2E-008, K-E2E-009, K-E2E-012 |
| FR-0.03-07 | K-E2E-006, K-E2E-007, K-E2E-008, K-E2E-009, K-E2E-014, K-E2E-015 |
| FR-0.03-08 | K-E2E-005, K-E2E-008, K-E2E-010, K-E2E-011, K-E2E-013 |
| FR-0.03-09 | K-E2E-006, K-E2E-008, K-E2E-009, K-E2E-013 |
| FR-0.03-10 | K-E2E-005, K-E2E-006, K-E2E-008, K-E2E-010, K-E2E-011, K-E2E-013, K-E2E-014, K-E2E-015 |
| FR-0.03-11 | K-E2E-007, K-E2E-008, K-E2E-009, K-E2E-010, K-E2E-012, K-E2E-014 |

---

## 6. Required failure-mode matrix

The aggregate suite must include these fail-closed cases. A fail-closed case may be
proved in any scenario, but the evidence report must name the test ID.

| Failure mode | Required pass condition |
| --- | --- |
| Invalid identity | Rejected before execution or delivery; trace/audit reason includes mismatched identity. |
| Missing permission | Gateway/verifier denies; adapter is not called. |
| Quota exceeded | Denied or paused according to policy; no silent allow; quota ledger identity is preserved. |
| Policy unavailable | Side-effectful work fails closed; no adapter call. |
| Policy expired | Side-effectful work fails closed or pauses according to documented policy behavior; no silent allow. |
| Verifier unavailable | Deny, pause, or intervention result; no implicit allow. |
| Adapter failure | Outcome is `failed`; trace and state behavior are explicit. |
| State commit failure | Next tick does not start; failure is traceable. |
| Trace persistence/sync failure | Side-effectful actions fail closed when durability is required; sync corruption is rejected/quarantined. |
| Malformed schema | Message, work-order, capability, percept, or telemetry schema rejected with structured error. |
| Wrong scope | Tenant/fleet/agent/run/work-order mismatch rejected before runtime side effects. |
| Replay side effect | Replay suppresses unsafe side effects by default and reports suppression. |
| Permission laundering | Shared/local/remote specialist cannot inherit caller permissions without explicit delegation. |
| Duplicate remote message | Duplicate detected by message ID and handled deterministically. |
| Corrupted state snapshot | Import rejected and receiver state remains unchanged. |
| Telemetry misuse | Telemetry is rejected as an authorization source. |
| Data-locality mismatch | Work order is rejected or placed pending with explicit reason; no broad data access. |
| Cross-tenant specialist access | Other-tenant document/state/trace access is denied and trace-linked. |
| Remote helper authority escalation | Helper proposal cannot execute origin actions or mutate origin state. |
| Read-only state mutation | Mutation attempt is denied and state head remains unchanged. |
| Unsafe retry | Non-idempotent retry is denied unless a new scoped authority explicitly allows it. |
| OpenAPI drift | Operation, schema, status, canonical primitive vocabulary, endpoint scope, auth/work-order field, trace redaction, or required-parameter mismatch fails contract validation. |
| Undocumented API path | Request is rejected and cannot mutate runtime state. |

---

## 7. Evidence quality bar

For each scenario, the test output must include or link to:

- the command used to run it;
- fixture paths and deterministic seed/time controls when applicable;
- exported trace events or trace query output;
- final state head and state graph proof;
- replay output or causal graph proof;
- denial/failure evidence;
- assertion that adapters were or were not called as expected;
- non-goals not exercised by the scenario.

A reviewer must be able to reproduce the suite from a clean checkout using only the
commands and fixture paths in the development guide.

---

## 8. Blocking conditions

Block a 0.03 final integration claim if any of these are true:

- any K-E2E scenario is absent or marked manual-only without deterministic evidence;
- a side-effectful action can execute without the action gateway;
- a required verifier can fail unavailable and still allow execution;
- trace events are missing, unordered within a run, or not identity-linked;
- state handoff uses hidden shared mutable state;
- work-order validation can be skipped by daemon/client authority;
- local or remote specialists inherit broad caller permissions by default;
- replay re-executes side effects by default;
- telemetry authorizes actions, placement, permissions, or work-order acceptance;
- docs claim completion without a report and command output proving it.
