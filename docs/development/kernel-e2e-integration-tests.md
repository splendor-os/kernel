# Kernel E2E Integration Tests Through 0.03

This guide explains how to call, reproduce, and review the kernel-level
end-to-end integration tests required by
`docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md`.

The suite covers the complete integration surface through Splendor0.03 final:

- local kernel loop and replay from 0.01;
- daemon/client security boundary and local multi-agent runtime from 0.02;
- resident node/fleet identity, signed work orders, placement, remote messaging,
  trace aggregation, state handoff, and fleet telemetry from 0.03.

This is a kernel E2E guide, not a unit-test inventory. Tests must use real kernel
enforcement paths and must prove denial, failure, trace, state, and replay behavior.

---

## 1. Aggregate command

The required final entry point is:

```bash
bash scripts/verify-0.03-kernel-e2e.sh
```

The script must fail non-zero if any required K-E2E scenario fails or if the
evidence report cannot be written.

Expected report:

```text
target/splendor-e2e/0.03-kernel-e2e-report.json
```

Until the aggregate script exists, run the manual command set below and attach the
outputs to the 0.03 final evidence doc or PR.

---

## 2. Clean-checkout reproduction

From the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pytest python/tests
npm test
```

Then run the 0.01 baseline command:

```bash
bash scripts/verify-0.01-baseline.sh
```

When implemented, run the 0.03 aggregate command:

```bash
bash scripts/verify-0.03-kernel-e2e.sh
```

The 0.03 script must set deterministic local test defaults. Recommended defaults:

```bash
SPLENDOR_E2E=1
SPLENDOR_E2E_MODE=local-deterministic
SPLENDOR_E2E_OUTPUT=target/splendor-e2e
```

The suite must not require external SaaS, production credentials, a Kubernetes
cluster, a real fleet manager, or a physical device. Loopback daemon transports,
in-memory stores, temp directories, and fixture adapters are acceptable when they
preserve the same kernel contracts.

---

## 3. Required test layout and names

The aggregate script should call stable scenario test names so reviewers can rerun
a failing slice directly.

Recommended Rust integration files:

```text
crates/splendor-kernel/tests/integration_kernel_e2e_001_local_loop.rs
crates/splendor-daemon/tests/integration_kernel_e2e_002_daemon_boundary.rs
crates/splendor-kernel/tests/integration_kernel_e2e_003_local_multi_agent.rs
crates/splendor-kernel/tests/integration_kernel_e2e_004_resident_work_order.rs
crates/splendor-kernel/tests/integration_kernel_e2e_005_remote_messages.rs
crates/splendor-kernel/tests/integration_kernel_e2e_006_trace_state_handoff.rs
crates/splendor-kernel/tests/integration_kernel_e2e_007_fleet_telemetry.rs
crates/splendor-kernel/tests/integration_kernel_e2e_008_final_journey.rs
crates/splendor-kernel/tests/integration_kernel_e2e_009_data_local_finance.rs
crates/splendor-kernel/tests/integration_kernel_e2e_010_shared_specialist_isolation.rs
crates/splendor-kernel/tests/integration_kernel_e2e_011_remote_helper_proposal.rs
crates/splendor-kernel/tests/integration_kernel_e2e_012_placement_fallback.rs
crates/splendor-kernel/tests/integration_kernel_e2e_013_read_only_state_reference.rs
crates/splendor-kernel/tests/integration_kernel_e2e_014_adapter_failure_retry.rs
crates/splendor-daemon/tests/integration_kernel_e2e_015_openapi_contract.rs
```

Recommended TypeScript and Python checks:

```text
typescript/tests/kernel-e2e-client.test.ts
python/tests/test_kernel_e2e_runtime.py
```

Existing tests may remain in their current files, but the aggregate evidence report
must map them to the stable K-E2E IDs.

---

## 4. Scenario calls and reproduction notes

### K-E2E-001 — Local kernel loop, gateway, state, trace, and replay

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_001_local_loop
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-kernel --test integration_loop_engine_state_trace_persistence
cargo test -p splendor-kernel --test integration_loop_engine_quota_denial
cargo test -p splendor-kernel --test integration_scheduler_state_trace_resume
cargo test -p splendor-kernel --test integration_adapters_filesystem_http_gateway
```

Fixture/example references:

- `examples/local-basic-loop/README.md`
- `examples/replay-local-run/README.md`
- `examples/python-sdk-basic/README.md`

Review the exported trace and state report for ordered tick events, gateway/verifier
evidence, adapter call counts, final state head, and replay side-effect suppression.
Also review Python SDK evidence for policy, percept/perceptor, constraint/verifier,
adapter, trace subscription, and replay hooks, plus CLI evidence for run execution,
trace export, and replay.

### K-E2E-002 — Daemon/client boundary

Run directly when implemented:

```bash
cargo test -p splendor-daemon --test integration_kernel_e2e_002_daemon_boundary
npm test
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-daemon --test runtime_daemon_api_tests
npm test
```

Fixture/example references:

- `examples/daemon-client-local/README.md`
- `examples/typescript-daemon-client/README.md`

Review caller identity, endpoint scope, tenant/fleet binding, audience binding,
expiry, revocation, signed work-order use, trace/audit attribution, state-head
query, trace read, and replay request evidence.
Also review that the daemon workflow is driven through HTTP-shaped API requests
validated against `openapi/splendor-runtime-daemon.yaml`, not private runtime calls.

### K-E2E-003 — Local multi-agent delegation and replay

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_003_local_multi_agent
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-kernel message_router
cargo test -p splendor-kernel local_delegation
```

Fixture/example references:

- `examples/local-multi-agent-router/README.md`
- `examples/local-orchestrator-specialists/README.md`
- `examples/local-specialist-scoped-delegation/README.md`
- `examples/local-multi-agent-replay/README.md`

Review message schema validation, per-agent inbox/outbox isolation, scoped
delegated permissions, quota separation, permission-laundering denial, parent/child
run trace links, and replay causal graph output.

### K-E2E-004 — Resident node, work order, and placement

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_004_resident_work_order
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-types work_order
cargo test -p splendor-types placement
cargo test -p splendor-types node_registry
cargo test -p splendor-kernel node_registry
```

Fixture/example references:

- `examples/resident-node-registration/README.md`
- `examples/signed-work-order-local-resident/README.md`
- `examples/placement-basic/README.md`

Review node/instance registration, heartbeat behavior, capability validation,
placement explanation, signed compatible work-order acceptance, and invalid
work-order rejection with no run or adapter execution.

### K-E2E-005 — Remote typed messages

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_005_remote_messages
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-kernel remote_message_transport
cargo test -p splendor-types message
```

Fixture/example reference:

- `examples/two-instance-message/README.md`

Review remote envelope preservation of canonical message fields, receiver authority
validation, trace-linked send/accept/deliver/consume events, duplicate detection,
transport failure evidence, and replay causal linkage.

### K-E2E-006 — Trace aggregation and state handoff

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_006_trace_state_handoff
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-store trace_sync
cargo test -p splendor-types state_handoff
```

Fixture/example references:

- `examples/resident-trace-sync/README.md`
- `examples/state-handoff-basic/README.md`

Review trace sync ordering, duplicate sync idempotence, missing/corrupt segment
rejection, central trace index query identities, snapshot hash verification,
state-owner/work-order validation, failed import state preservation, and replay
handoff boundary output.

### K-E2E-007 — Fleet telemetry

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_007_fleet_telemetry
```

Existing related coverage to keep passing:

```bash
cargo test -p splendor-kernel fleet_telemetry
cargo test -p splendor-types fleet_telemetry
```

Fixture/example reference:

- `examples/fleet-telemetry-basic/README.md`

Review online/stale/offline node states, instance runtime metadata, canonical run
status vocabulary, quota and denial identity, trace-sync lag/failure, failure
categories, malformed telemetry behavior, and telemetry non-authority evidence.

### K-E2E-008 — Final 0.03 cross-primitive journey

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_008_final_journey
```

Fixture/example reference:

- `examples/kernel-e2e-through-0.03/README.md`

Review that this scenario ties K-E2E-001 through K-E2E-007 together in one causal
graph and proves no gateway bypass, no verifier silent allow, no permission
laundering, no replay side effects, and no telemetry-as-authority.

### K-E2E-009 — Data-local finance report

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_009_data_local_finance
```

Review signed work-order data refs, customer-VPC/on-prem placement, gateway-mediated
read/artifact actions, broader dataset denial, state/report trace linkage, replay
suppression, and telemetry.

### K-E2E-010 — Shared specialist isolation

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_010_shared_specialist_isolation
```

Review two tenant-scoped work orders, separate quotas/state/traces, explicit
delegated document refs, cross-tenant denial, no permission reuse, and replay of
both causal graphs.

### K-E2E-011 — Remote helper proposal

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_011_remote_helper_proposal
```

Review helper request/response messages, proposal artifact/reference semantics,
origin-only gateway authority, helper state ownership, wrong-scope rejection, and
replay without resend or adapter execution.

### K-E2E-012 — Placement fallback

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_012_placement_fallback
```

Review stale heartbeat handling, capability mismatch explanation, compatible-node
selection or explicit rejection, malformed heartbeat/capability quarantine, and
telemetry non-authority.

### K-E2E-013 — Read-only state reference

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_013_read_only_state_reference
```

Review exported read-only state ref fields, receiver validation, mutation denial,
origin-owned state commit, trace linkage, and replay of the reference boundary.

### K-E2E-014 — Adapter failure and retry

Run directly when implemented:

```bash
cargo test -p splendor-kernel --test integration_kernel_e2e_014_adapter_failure_retry
```

Review adapter failure outcome, failed-action trace, explicit state behavior,
idempotent retry quota/trace, non-idempotent retry denial, telemetry failure signal,
and replay without adapter execution.

### K-E2E-015 — OpenAPI daemon contract

Run directly when implemented:

```bash
cargo test -p splendor-daemon --test integration_kernel_e2e_015_openapi_contract
npm test
```

Review OpenAPI 3.1 parsing, operation coverage for all exposed daemon endpoints,
request/response schema validation, canonical primitive parity for run statuses,
endpoint scopes, caller/work-order fields and action outcomes, required
`redaction_policy` on trace reads, TypeScript client schema parity or
generated-client evidence, undocumented-path rejection, and replay/action security
behavior through the exposed API.

---

## 5. Evidence report schema

The aggregate report must be JSON and include at least:

```json
{
  "schema": "splendor.kernel_e2e_report.v1",
  "milestone": "Splendor0.03-dev",
  "generated_at": "2026-05-27T00:00:00Z",
  "repository_revision": "<git-sha-or-equivalent>",
  "commands": ["bash scripts/verify-0.03-kernel-e2e.sh"],
  "scenarios": [
    {
      "id": "K-E2E-001",
      "status": "passed",
      "frs": ["FR-0.01-01"],
      "trace_event_ids": ["trace_..."],
      "run_ids": ["run_..."],
      "state_node_ids": ["state_..."],
      "message_ids": [],
      "work_order_ids": [],
      "negative_cases": ["missing_permission_denied"],
      "cli_evidence": ["run_execution", "trace_export", "replay_request"],
      "python_sdk_evidence": ["policy", "percept", "verifier", "adapter", "trace_subscription", "replay"],
      "typescript_client_evidence": [],
      "openapi_contract_version": "0.02-S5",
      "openapi_operations_used": ["createRun", "appendPercept", "startRun", "getRunTraces", "getStateHead", "replayRun"],
      "openapi_schema_validation": "passed",
      "openapi_canonical_parity": ["run_status", "endpoint_scopes", "caller_credentials", "work_order_authorization", "action_outcome", "trace_redaction"],
      "policy_failure_cases": ["policy_unavailable_fail_closed", "policy_expired_fail_closed"],
      "replay": {
        "mode": "inspect_only",
        "side_effects_suppressed": true
      },
      "artifacts": ["target/splendor-e2e/K-E2E-001-trace.json"]
    }
  ]
}
```

The actual report may include more fields, but it must not omit identity, trace,
state, denial, and replay evidence.

---

## 6. Review checklist

Before accepting a 0.03 final E2E result, reviewers must confirm:

- [ ] `docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md` scenarios K-E2E-001 through K-E2E-015 are all mapped to passing evidence.
- [ ] Positive, denial, failure, trace, state, replay, quota/permission, and compatibility evidence are present.
- [ ] No test uses a mocked gateway, verifier chain, trace store, or state store while claiming integrated runtime correctness.
- [ ] No side-effectful adapter is called during replay.
- [ ] Work-order rejection paths create no run and execute no policy or adapter.
- [ ] Local and remote specialists cannot launder permissions.
- [ ] Trace sync and state handoff preserve identity and integrity.
- [ ] Fleet telemetry remains observational and non-authoritative.
- [ ] Commands can be run from a clean checkout without undocumented services.
- [ ] OpenAPI contract validation covers every exposed daemon operation used by the E2E suite.
- [ ] OpenAPI schemas are parity-checked against canonical Splendor primitive vocabularies and security fields.
