# Work Orders

Work orders are the explicit authority objects for starting or resuming resident
or distributed Splendor runs. They scope a run to one tenant, one agent, an
optional run identity, explicit action/adapter/permission allowlists, data
references, quotas, placement hints, expiry, revocation status, and detached
signature metadata.

Sprint 0.03-S3 implements a local/resident ingestion path and reference verifier.
It does **not** implement fleet dispatch, a PKI product, an OAuth server, an
enterprise identity provider, or an approval workflow engine.

## Schema

Rust contract: `splendor_types::{WorkOrder, WorkOrderEnvelope}`.

Serialized shape:

```yaml
schema_version: splendor.work_order.v1
work_order_id: wo_123
tenant_id: 11111111-1111-1111-1111-111111111111
agent_id: 22222222-2222-2222-2222-222222222222
run_id: 33333333-3333-3333-3333-333333333333
objective: Generate weekly revenue dashboard
allowed_actions:
  - sql.query
  - artifact.create
allowed_adapters:
  - sql
  - artifact-store
allowed_permissions:
  - finance.read
  - artifact.create
data_refs:
  - dataset:finance.revenue_monthly_v4
quotas:
  max_actions_per_tick: 5
  max_action_duration_ms: 30000
  max_filesystem_read_bytes: null
  max_filesystem_write_bytes: null
  max_network_read_bytes: null
  max_network_write_bytes: null
  max_http_requests_per_minute: 60
placement:
  target: resident_cloud_pool
  data_locality: eu-west
  requires_gpu: false
  dedicated_instance: false
  required_capabilities: []
  max_runtime_ms: null
issued_at: "2026-05-25T00:00:00Z"
expires_at: "2026-05-25T01:00:00Z"
revocation: active
signature:
  key_id: manager-local-key
  signature: <detached-signature>
```

### Required fields

- `schema_version`: currently `splendor.work_order.v1`.
- `work_order_id`: manager-issued string. It is distinct from `run_id`,
  `action_id`, `state_node_id`, `trace_event_id`, and `message_id`.
- `tenant_id`, `agent_id`, `run_id`: scope the authority. `run_id` is optional in
  the payload for future create paths, but resume must be bound to the resumed
  run. `splendorctl` resolves missing local run IDs from the work order when
  present.
- `allowed_actions`, `allowed_adapters`: non-empty allowlists.
- `allowed_permissions`: delegated permission tokens. Empty means no delegated
  permissions.
- `data_refs`: explicit data references in scope.
- `quotas`: optional per-field limits. A missing field does not increase tenant
  quota.
- `placement`: compatibility hints only in 0.03-S3. Placement decisions remain
  0.03-S4 scope.
- `issued_at`, `expires_at`: RFC3339 timestamps. `expires_at` must be after
  `issued_at`, and expired work orders fail closed.
- `revocation`: `active` or a revoked marker from a revocation list,
  introspection endpoint, or invalidated signing key path.
- `signature`: detached signature metadata. Missing or empty `key_id` or
  `signature` fails closed.

## Signature envelope

The reference 0.03-S3 verifier uses `blake3-keyed-v1`:

1. Serialize the `WorkOrder` payload deterministically with `serde_json`.
2. Exclude the `signature` field from the signed payload.
3. Derive a 32-byte verifier key by hashing the configured shared secret with
   BLAKE3.
4. Compute a keyed BLAKE3 MAC over the serialized payload.
5. Compare the expected and supplied signatures in constant time.

This is a local/resident reference path for tests and early integration. It is
not a PKI or key-management product. Later fleet work can replace the key source
without changing the WorkOrder payload contract.

## Validation lifecycle

`validate_work_order(envelope, context, keyring)` checks:

1. schema shape and non-empty allowlists;
2. detached signature presence;
3. known verification key;
4. signature correctness;
5. expiry;
6. revocation marker;
7. tenant, agent, run, and placement target compatibility.

Any failed or unavailable check returns a `WorkOrderValidationError` and denies
run ingestion.

## Runtime constraints

When a work order validates, the runtime narrows tenant policy:

```text
effective actions     = tenant.allowed_actions ∩ work_order.allowed_actions
effective adapters    = tenant.allowed_adapters ∩ work_order.allowed_adapters
effective permissions = tenant.allowed_permissions ∩ work_order.allowed_permissions
effective quota       = min(tenant quota, work-order quota) per field
```

This prevents permission laundering: a work order can only reduce runtime
authority, never broaden tenant or agent authority. Side-effectful actions still
must pass through the Action Gateway and verifier chain before adapter execution.

## Trace events

0.03-S3 adds these trace events:

- `WorkOrderAccepted { work_order_id, tenant_id, agent_id, run_id }`
- `WorkOrderRejected { work_order_id?, tenant_id?, agent_id?, run_id?, reason }`

`WorkOrderAccepted` is emitted after `RunStarted` for a new persisted local run.
`WorkOrderRejected` is emitted as a management/audit trace before any perceptor,
policy, state commit, gateway, or adapter execution when a run identity is
available. Rejection events contain a sanitized reason code such as
`unsigned_work_order`, `bad_signature`,
`expired_work_order`, `revoked_work_order`, `malformed_work_order`, or
`incompatible_work_order`; they do not contain signature material or verifier
secrets.

## Replay behavior

Replay remains inspect-only. Work-order acceptance/rejection trace records can be
decoded and inspected, but replay does not re-verify external signatures, call
revocation services, invoke policies, or execute adapters. This preserves the
0.01 replay invariant: replay reconstructs behavior without accidental side
effects.

## Failure modes

Validation fails closed for:

- missing or empty signature metadata;
- missing work-order authority unless `allow_unsigned_local_run: true` is
  explicitly set for local development;
- unknown signing key;
- bad signature;
- expired work order;
- revoked work order;
- malformed schema or empty allowlists;
- tenant, agent, run, or placement incompatibility;
- trace persistence failure while writing rejection audit records.
