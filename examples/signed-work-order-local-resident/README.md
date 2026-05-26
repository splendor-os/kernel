# Signed Work Order Local Resident Example

This example documents the 0.03-S3 local/resident work-order ingestion path. A
signed work order scopes one local run before perceptors, policy, state commits,
gateway verification, or adapters can run.

The runnable fixture is covered by the `splendorctl` test suite because the
signature is generated from the canonical Rust payload to avoid checking local
verification secrets into an example config.

## Run the fixture

```bash
cargo test -p splendorctl run_from_config_validates_signed_work_order_and_records_metadata
cargo test -p splendorctl run_from_config_bad_work_order_signature_records_audit_without_starting_run
cargo test -p splendorctl work_order_scope_denies_actions_outside_delegated_allowlist
```

## What the fixture proves

- A valid `WorkOrderEnvelope` is verified before run creation.
- `work_order_id` is stored in agent runtime metadata.
- `RunStarted` and `WorkOrderAccepted` appear before tick events.
- Work-order allowed actions/adapters/permissions narrow tenant policy.
- A bad signature writes a sanitized `WorkOrderRejected` management/audit trace.
- The rejected path does not create the state database, collect percepts, invoke
  policy, reach the gateway, or execute the filesystem adapter.

## Minimal config shape

The config below is schematic. The `signature` value must be computed over the
canonical `WorkOrder` payload with the reference `blake3-keyed-v1` verifier.

```yaml
trace_db: ./examples/signed-work-order-local-resident/data/trace.db
state_db: ./examples/signed-work-order-local-resident/data/state.db
run_id: 33333333-3333-3333-3333-333333333333
tenants:
  - id: 11111111-1111-1111-1111-111111111111
    allowed_actions: ["write_file", "delete_file"]
    allowed_adapters: ["filesystem"]
    allowed_permissions: ["fs.write"]
    quotas:
      max_actions_per_tick: 5
      max_filesystem_write_bytes: 1024
agents:
  - id: 22222222-2222-2222-2222-222222222222
    tenant_id: 11111111-1111-1111-1111-111111111111
    run_id: 33333333-3333-3333-3333-333333333333
    policy:
      type: static
      actions:
        - name: write_file
          adapter: filesystem
          side_effect_class: filesystem
          required_permissions: ["fs.write"]
          params:
            path: hello.txt
            contents: hi
          usage:
            actions: 1
            filesystem_write_bytes: 2
adapters:
  filesystem:
    base_dir: ./examples/signed-work-order-local-resident/data/fs
work_order:
  schema_version: splendor.work_order.v1
  work_order_id: wo_local_resident_example
  tenant_id: 11111111-1111-1111-1111-111111111111
  agent_id: 22222222-2222-2222-2222-222222222222
  run_id: 33333333-3333-3333-3333-333333333333
  objective: exercise signed local resident work-order ingestion
  allowed_actions: ["write_file"]
  allowed_adapters: ["filesystem"]
  allowed_permissions: ["fs.write"]
  data_refs: ["dataset:example.local"]
  quotas:
    max_actions_per_tick: 1
    max_filesystem_write_bytes: 64
  placement:
    target: local_resident
    data_locality: local
    requires_gpu: false
    dedicated_instance: false
    required_capabilities: ["filesystem"]
  issued_at: "2026-05-25T00:00:00Z"
  expires_at: "2026-05-25T01:00:00Z"
  revocation: active
  signature:
    key_id: local-test
    signature: <computed-detached-signature>
  verification_secret: <local-fixture-secret>
  expected_placement_target: local_resident
```

## Expected traces

Valid path:

```text
RunStarted
WorkOrderAccepted
LoopTickStarted
PerceptsReceived
StateLoaded
PolicyInvoked
PolicyCompleted
CandidatesProposed
ConstraintsEvaluated
ActionVerificationStarted
ActionVerificationCompleted
ActionExecuted | ActionDenied
OutcomeRecorded
StateCommitted
LoopTickCompleted
```

Rejected signature path:

```text
WorkOrderRejected(reason = "bad_signature")
```

No policy, state commit, gateway submission, or adapter execution follows a
rejected work order.

## Non-goals

- No fleet scheduler or remote dispatch.
- No production PKI or key-management rollout.
- No governance approval workflow.
- No broad credentials in work orders.
