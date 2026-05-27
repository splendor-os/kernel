# 0.03 Final — Kernel E2E Integration Gate

## 1. Objective

Define the final Splendor0.03-dev kernel-level end-to-end integration gate. This
gate proves that the 0.01 local runtime foundation, 0.02 local multi-agent and
daemon control surface, and 0.03 resident/fleet execution foundation compose into a
single enforceable, traceable, replay-safe runtime surface.

This is a documentation and verification contract. It does not claim completion
unless the required commands and evidence report pass.

## 2. Functional scope

- Requires the scenarios in
  `docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md`.
- Requires a stable aggregate command:
  `bash scripts/verify-0.03-kernel-e2e.sh`.
- Requires evidence report:
  `target/splendor-e2e/0.03-kernel-e2e-report.json`.
- Requires cross-reference to existing sprint criteria for 0.01-H1..H4,
  0.02-S0..S7, and 0.03-S1..S8.
- Requires reproducible local fixtures and examples, not external SaaS or physical
  infrastructure.

## 3. Non-goals

- No 0.04 governance approval workflow, circuit breaker, kill switch, or policy TTL
  implementation.
- No 0.05 physical/edge safety verifier, robotics adapter, offline policy cache, or
  operator intervention implementation.
- No distributed consensus, exactly-once global messaging guarantee, or shared
  mutable distributed memory.
- No production PKI/OAuth rollout, Kubernetes operator, dashboard, billing, or
  marketplace.
- No claim that a docs-only change proves runtime success.

## 4. Public contracts changed

New documentation contracts:

- `docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md`
- `docs/development/kernel-e2e-integration-tests.md`
- `docs/milestones/0.03-dev/final-kernel-e2e-integration.md`
- `examples/kernel-e2e-through-0.03/README.md`

Updated documentation:

- `docs/rules/verifiable_criteria/main.md`
- `docs/development/ci-release-checklist.md`

No runtime schemas, daemon endpoints, trace event names, state formats, or SDK APIs
are changed by this gate document.

## 5. Runtime primitives touched

| Primitive | Required integration proof |
| --- | --- |
| Percept | Percepts submitted through local/daemon paths drive identical runtime behavior. |
| Policy | Policy invocation is trace-linked and cannot silently bypass constraints/gateway. |
| Constraint/verifier | Required verifiers run before adapter execution and fail closed. |
| Action gateway | Every side effect is gateway-mediated; denied actions do not reach adapters. |
| Adapter | Fixture adapters execute only after verification; replay suppresses side effects. |
| Quota | Per-agent and work-order quotas deny predictably and preserve identity. |
| State graph | State commits are explicit, hash/reference-backed, and handoff validated. |
| Trace store | Tick, message, work-order, sync, handoff, denial, and failure events are ordered and identity-linked. |
| Replay | Replay reconstructs local and distributed causality without unsafe execution. |
| Message | Local and remote typed messages preserve schemas, identity, and causal parents. |
| Work order | Signed scoped work orders authorize runs; invalid work orders create no run. |
| Fleet/node identity | Fleet/node/instance IDs remain separate from tenant/agent/run IDs. |
| Node registry | Resident nodes and instances register, heartbeat, and advertise capabilities. |
| Placement | Placement v0 selects or rejects targets deterministically without widening authority. |
| Trace aggregation | Local buffers sync to a central index without reordering or duplicating events. |
| State handoff | Explicit snapshots/references move state without hidden shared memory. |
| Fleet telemetry | Telemetry reports health/status/failure facts and remains observational only. |
| SDK/API | Daemon and TypeScript/Python surfaces remain thin clients of kernel semantics. |
| OpenAPI contract | Exposed daemon API operations and schemas are validated and used by E2E clients. |

## 6. Required final journey

The final K-E2E-008 scenario must prove the following causal journey:

```text
authenticated caller through OpenAPI-described daemon API
  -> signed scoped work order
  -> resident node registration and placement
  -> local orchestrator run
  -> scoped local specialist delegation
  -> trace-linked remote typed message
  -> gateway-mediated allowed action
  -> denied disallowed action
  -> explicit state commit
  -> local trace buffer sync
  -> interruption and explicit state handoff
  -> validated resume on compatible instance
  -> replay causal graph without side effects
  -> fleet telemetry snapshot without runtime authority
```

The final journey must export a causal graph containing:

- fleet ID, node IDs, instance IDs;
- tenant ID, agent IDs, run IDs, tick IDs;
- work-order ID;
- message IDs and causal parent trace IDs;
- action IDs and gateway/verifier results;
- state node IDs, parent links, and state hashes;
- trace event IDs for send/receive, denial, failure, sync, handoff, resume, and
  completion;
- telemetry snapshot reference.

## 7. Test evidence matrix

| Scenario | Purpose | Required direct command | Evidence artifact |
| --- | --- | --- | --- |
| K-E2E-001 | Local loop, gateway, state, trace, replay | `cargo test -p splendor-kernel --test integration_kernel_e2e_001_local_loop` | `target/splendor-e2e/K-E2E-001-*` |
| K-E2E-002 | Daemon/client auth, scopes, work-order run control | `cargo test -p splendor-daemon --test integration_kernel_e2e_002_daemon_boundary` and `npm test` | `target/splendor-e2e/K-E2E-002-*` |
| K-E2E-003 | Local multi-agent messages, delegation, isolation | `cargo test -p splendor-kernel --test integration_kernel_e2e_003_local_multi_agent` | `target/splendor-e2e/K-E2E-003-*` |
| K-E2E-004 | Resident node registry, work order, placement | `cargo test -p splendor-kernel --test integration_kernel_e2e_004_resident_work_order` | `target/splendor-e2e/K-E2E-004-*` |
| K-E2E-005 | Cross-instance remote messages | `cargo test -p splendor-kernel --test integration_kernel_e2e_005_remote_messages` | `target/splendor-e2e/K-E2E-005-*` |
| K-E2E-006 | Trace aggregation and state handoff | `cargo test -p splendor-kernel --test integration_kernel_e2e_006_trace_state_handoff` | `target/splendor-e2e/K-E2E-006-*` |
| K-E2E-007 | Fleet telemetry non-authoritative status | `cargo test -p splendor-kernel --test integration_kernel_e2e_007_fleet_telemetry` | `target/splendor-e2e/K-E2E-007-*` |
| K-E2E-008 | Complete 0.03 integrated journey | `cargo test -p splendor-kernel --test integration_kernel_e2e_008_final_journey` | `target/splendor-e2e/K-E2E-008-*` |
| K-E2E-009 | Data-local finance report on resident VPC/on-prem node | `cargo test -p splendor-kernel --test integration_kernel_e2e_009_data_local_finance` | `target/splendor-e2e/K-E2E-009-*` |
| K-E2E-010 | Shared specialist isolation across tenants | `cargo test -p splendor-kernel --test integration_kernel_e2e_010_shared_specialist_isolation` | `target/splendor-e2e/K-E2E-010-*` |
| K-E2E-011 | Remote helper proposal without direct authority | `cargo test -p splendor-kernel --test integration_kernel_e2e_011_remote_helper_proposal` | `target/splendor-e2e/K-E2E-011-*` |
| K-E2E-012 | Placement fallback under stale/capability mismatch | `cargo test -p splendor-kernel --test integration_kernel_e2e_012_placement_fallback` | `target/splendor-e2e/K-E2E-012-*` |
| K-E2E-013 | Read-only state reference collaboration | `cargo test -p splendor-kernel --test integration_kernel_e2e_013_read_only_state_reference` | `target/splendor-e2e/K-E2E-013-*` |
| K-E2E-014 | Adapter failure and safe retry boundaries | `cargo test -p splendor-kernel --test integration_kernel_e2e_014_adapter_failure_retry` | `target/splendor-e2e/K-E2E-014-*` |
| K-E2E-015 | OpenAPI daemon API contract and client workflow | `cargo test -p splendor-daemon --test integration_kernel_e2e_015_openapi_contract` and `npm test` | `target/splendor-e2e/K-E2E-015-*` |

The aggregate script must collect all artifacts into:

```text
target/splendor-e2e/0.03-kernel-e2e-report.json
```

## 8. Acceptance criteria

- [ ] All K-E2E scenarios pass from a clean checkout.
- [ ] The aggregate evidence report is produced and includes FR mappings.
- [ ] At least one positive path, denial path, failure path, trace path, state path,
  replay path, quota/permission path, and compatibility path are proven.
- [ ] No side-effect path bypasses the action gateway.
- [ ] Required verifiers fail closed.
- [ ] Trace events are ordered within each run and identity-linked across local and
  remote boundaries.
- [ ] State commits and handoffs are explicit, versioned, and integrity-checked.
- [ ] Replay suppresses unsafe side effects by default.
- [ ] Work orders are signed, scoped, expiry-bound, revocation-aware, and recorded in
  trace/audit evidence.
- [ ] OpenAPI daemon contract is parsed, operation-covered, schema-validated,
  parity-checked against canonical Splendor primitive/security contracts, and used
  by the daemon/client E2E path.
- [ ] Specialists cannot launder permissions locally or remotely.
- [ ] Telemetry is observational and cannot authorize runtime behavior.
- [ ] Non-goals from 0.04 and 0.05 remain out of scope.

## 9. Review and failure policy

If any required scenario fails, the 0.03 final integration gate is not met. Do not
replace a failing E2E path with a unit test or a manual screenshot. Use the failing
scenario ID to isolate the primitive, fix the smallest kernel path, and rerun the
aggregate command.

If behavior is unclear, preserve fail-closed semantics and document the gap before
claiming milestone completion.
