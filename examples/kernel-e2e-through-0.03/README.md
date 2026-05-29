# Kernel E2E Through 0.03 Fixture

This example directory documents the required end-to-end fixture for the
Splendor0.03 final kernel integration gate.

It is intentionally a full kernel journey, not a quickstart. The fixture must prove
that 0.01 local runtime primitives, 0.02 local multi-agent/daemon primitives, and
0.03 resident/fleet primitives work together without weakening gateway,
verification, state, trace, replay, identity, work-order, or telemetry invariants.

Authoritative rule pack:

```text
docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md
```

Developer guide:

```text
docs/development/kernel-e2e-integration-tests.md
```

---

## Intended command

From the repository root:

```bash
bash scripts/verify-0.03-kernel-e2e.sh
```

Until that aggregate script exists, follow the manual commands in
`docs/development/kernel-e2e-integration-tests.md`.

Expected evidence report:

```text
target/splendor-e2e/0.03-kernel-e2e-report.json
```

---

## Fixture topology

The fixture should model these local deterministic components:

```text
test client / CLI / TypeScript client
  -> OpenAPI-described runtime daemon API
  -> local daemon boundary
  -> minimal central manager fixture
  -> resident node A / instance A
       -> orchestrator agent
       -> local specialist agent
  -> resident node B / instance B
       -> remote specialist agent
  -> resident node C / instance C
       -> compatible fallback worker
  -> shared document specialist fixture
  -> central trace index fixture
  -> fleet telemetry collector fixture
```

All components may run in one process or one test harness as long as the documented
runtime boundaries are preserved. The fixture must not rely on external SaaS,
production credentials, a real fleet manager, Kubernetes, or physical devices.

---

## Scenario data

The final journey should use deterministic data with stable IDs or stable ID
prefixes:

```text
fleet_id: fleet_e2e_0_03
node_a: node_e2e_resident_a
node_b: node_e2e_resident_b
instance_a: instance_e2e_a
instance_b: instance_e2e_b
tenant_id: tenant_e2e_acme
orchestrator_agent: agent_e2e_orchestrator
local_specialist_agent: agent_e2e_local_specialist
remote_specialist_agent: agent_e2e_remote_specialist
shared_document_specialist: agent_e2e_shared_parser
work_order_id: wo_e2e_signed_0_03
```

The exact generated IDs may differ when the runtime requires generated IDs, but the
evidence report must preserve identity separation and map generated IDs back to the
fixture roles.

---

## Required journey

1. Register node A and node B with different capabilities.
2. Register instance A and instance B under their nodes.
3. Submit a signed scoped work order through an authenticated caller boundary.
4. Place the work order on instance A using placement v0.
5. Start an orchestrator run from the work order.
6. Append a percept that causes the orchestrator policy to:
   - delegate a scoped local task to the local specialist;
   - send a typed remote task request to the remote specialist;
   - propose one allowed gateway-mediated action;
   - propose one disallowed action that must be denied before adapter execution.
7. Commit explicit state and append the required ordered trace events.
8. Buffer and sync local traces to the central trace index.
9. Interrupt the run, export an explicit state handoff, validate it on instance B,
   and resume from the validated state head.
10. Request replay and export a causal graph without executing side effects.
11. Query fleet telemetry and prove it is observational only.

---

## Required complete use-case variants

In addition to the final cross-primitive journey, the fixture must include these
realistic kernel use cases through 0.03-S8:

- `K-E2E-009`: data-local finance report placed on a resident VPC/on-prem node,
  with scoped data refs, gateway-mediated read/artifact actions, broader dataset
  denial, state/trace/replay evidence, and telemetry.
- `K-E2E-010`: two tenants share a document specialist without cross-tenant
  document, state, message, quota, trace, or permission leakage.
- `K-E2E-011`: remote helper returns a proposal artifact/reference, while the
  origin run keeps all gateway/action authority.
- `K-E2E-012`: placement fallback handles stale node, missing capability, and
  compatible node selection or explicit rejection without telemetry as authority.
- `K-E2E-013`: remote specialist receives a read-only state reference, can inspect
  scoped context, and cannot mutate origin state.
- `K-E2E-014`: adapter failure records trace/outcome/state explicitly, retries only
  when idempotent and authorized, and replay does not execute adapters.
- `K-E2E-015`: OpenAPI daemon contract covers every exposed daemon operation used by
  the E2E suite, with request/response schema validation, canonical primitive
  parity, and API-client evidence.

---

## Required negative cases

The fixture must include deterministic negative cases, not separate optional smoke
tests:

- anonymous daemon request rejected;
- missing endpoint scope rejected;
- unsigned work order rejected with no run created;
- expired or revoked work order rejected before percept, policy, or adapter
  execution;
- invalid capability document rejected before registration;
- placement missing capability rejected with explicit reason;
- disallowed action denied before adapter execution;
- local specialist cannot inherit orchestrator permissions;
- remote duplicate message handled deterministically;
- remote message with wrong tenant/run/work-order scope rejected before delivery;
- corrupted trace sync segment rejected or quarantined;
- corrupted state snapshot import rejected and receiver state unchanged;
- replay suppresses side-effectful adapter execution;
- telemetry cannot authorize placement, permissions, gateway decisions, verifier
  results, work-order acceptance, or adapter execution.
- data-locality mismatch rejected without broad data access;
- cross-tenant specialist access denied;
- remote helper authority escalation denied;
- read-only state mutation denied;
- non-idempotent retry denied unless a new scoped authority explicitly allows it.
- undocumented API path or OpenAPI schema mismatch rejected without state mutation;
- `/actions` cannot self-attest gateway completion or bypass gateway verification.

---

## Required outputs

The fixture must write artifacts under:

```text
target/splendor-e2e/
```

Minimum artifacts:

```text
0.03-kernel-e2e-report.json
K-E2E-008-trace-source.json
K-E2E-008-trace-central-index.json
K-E2E-008-state-handoff.json
K-E2E-008-replay-causal-graph.json
K-E2E-008-telemetry-snapshot.json
K-E2E-008-denials.json
K-E2E-009-data-local-report.json
K-E2E-010-shared-specialist-isolation.json
K-E2E-011-remote-helper-proposal.json
K-E2E-012-placement-fallback.json
K-E2E-013-read-only-state-reference.json
K-E2E-014-adapter-failure-retry.json
K-E2E-015-openapi-contract.json
```

The report must identify which artifacts prove each FR and each K-E2E scenario.

---

## What this fixture deliberately does not prove

- It does not prove 0.04 approvals, circuit breakers, kill switches, or policy TTL
  distribution.
- It does not prove 0.05 physical/edge safety behavior.
- It does not prove production transport security, full PKI, OAuth, or fleet mTLS.
- It does not prove Kubernetes scheduling, autoscaling, UI dashboards, or analytics.
- It does not prove global exactly-once remote delivery or distributed consensus.

Those are future milestone concerns and must not be claimed from this fixture.
