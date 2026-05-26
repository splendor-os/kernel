# 0.03-S3 — Signed work orders

## Objective

Implement signed work orders as the explicit authority object for starting or
resuming local/resident runs. The primitive strengthened is `work order`, with
supporting trace-store and gateway/verifier integration so invalid authority
fails closed before runtime execution.

## Functional scope

- Added canonical `WorkOrder` and `WorkOrderEnvelope` Rust schemas.
- Added reference detached signature verification (`blake3-keyed-v1`) using a
  caller-supplied keyring.
- Added work-order ingestion to `splendorctl run` for local resident examples.
- Added fail-closed checks for missing/unsigned, bad-signature, expired,
  revoked, malformed, and incompatible work orders.
- Added runtime policy/quota narrowing from validated work-order scopes.
- Added trace events for accepted and rejected work-order ingestion.

## Non-goals

- No full PKI product.
- No OAuth/OIDC or enterprise identity provider integration.
- No fleet dispatch or remote transport.
- No placement engine beyond local target compatibility checks.
- No governance approval workflow engine.
- No physical/edge orchestration.

## Public contracts changed

- New `splendor-types` exports: `WorkOrder`, `WorkOrderEnvelope`,
  `WorkOrderId`, `WorkOrderKeyring`, `WorkOrderPlacement`,
  `WorkOrderQuotaPolicy`, `WorkOrderValidationContext`,
  `WorkOrderValidationError`, `ValidatedWorkOrder`,
  `WORK_ORDER_SCHEMA_VERSION`, and `WORK_ORDER_SIGNATURE_ALGORITHM`.
- New `TraceEventKind` variants: `WorkOrderAccepted` and `WorkOrderRejected`.
- New kernel helpers: `TenantPolicy::constrain_to_work_order`,
  `QuotaPolicy::constrain_to_work_order`,
  `LoopEngine::with_trace_store_and_work_order`, and
  `LoopEngine::resume_from_trace_store_with_work_order`.
- `splendorctl` run config accepts a signed `work_order` envelope for local
  resident authority.
- `splendorctl` run config rejects missing `work_order` authority unless
  `allow_unsigned_local_run: true` is explicitly set for local-development
  compatibility.
- Documentation added under `docs/reference/work-orders.md`,
  `docs/security/signed-work-orders.md`, and
  `examples/signed-work-order-local-resident/README.md`.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | unchanged; invalid work orders prevent percept collection |
| Policy | unchanged; invalid work orders prevent policy invocation |
| Gateway | work-order scope narrows gateway policy/quota checks |
| Verifier | added work-order ingestion verifier path |
| State graph | unchanged; invalid work orders prevent state store creation/commit in CLI path |
| Trace store | added acceptance/rejection audit events |
| Replay | unchanged inspect-only behavior; work-order events are reconstructable |
| Message | unchanged |
| Work order | added canonical schema, signature envelope, validation, and runtime scope |
| Governance | unchanged; approval workflows remain 0.04 scope |

## Trace behavior

- `RunStarted` remains the first event for valid new persisted runs.
- `WorkOrderAccepted` follows `RunStarted` when a validated work order is bound
  to the run.
- `WorkOrderRejected` is emitted as a management/audit trace for validation
  failure before tick events when a run identity is available.
- Rejection trace fields are sanitized: identity fields plus reason code only;
  no signatures or verifier secrets.

## State behavior

- Valid work orders do not create extra state nodes.
- Invalid work orders fail before state store creation in `splendorctl`, before
  state commits, and before any state head update.
- Existing state graph replay/commit behavior is unchanged.

## Gateway and verifier behavior

- Work-order validation fails closed on missing work-order authority, missing
  signature, unknown key, bad signature, expiry, revocation, malformed scope,
  tenant/agent/run mismatch, or placement target mismatch.
- Allowed actions, adapters, and permissions are intersected with tenant policy.
- Quotas are narrowed to the smaller tenant/work-order limit per field.
- Side effects still execute only through the Action Gateway after normal
  permission, quota, invariant, and adapter checks.

## Replay behavior

- Replay can decode accepted/rejected work-order trace events.
- Replay does not re-check signatures, call revocation sources, invoke policies,
  run verifiers, or execute adapters.
- There is no side-effectful replay mode.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | Work-order schema/signature validation | `cargo test -p splendor-types` |
| unit | Policy/quota narrowing | `cargo test -p splendor-kernel tenancy::tests::work_order_scope_only_narrows_policy_and_quota` |
| integration-ish CLI unit | Valid signed work order creates run metadata/trace | `run_from_config_validates_signed_work_order_and_records_metadata` |
| negative | Missing work order records sanitized audit and no side effect/state DB | `run_from_config_rejects_missing_work_order_by_default` |
| negative | Bad signature records sanitized audit and no side effect/state DB | `run_from_config_bad_work_order_signature_records_audit_without_starting_run` |
| negative/gateway | Work-order action allowlist denial reaches gateway and skips adapter side effect | `work_order_scope_denies_actions_outside_delegated_allowlist` |
| replay/trace | Work-order events decode through existing trace validation and replay remains inspect-only | `cargo test -p splendorctl` |

## Example or fixture

- Example guide: `examples/signed-work-order-local-resident/README.md`.
- Executable fixture path: `cargo test -p splendorctl run_from_config_validates_signed_work_order_and_records_metadata`.

## Future extension notes

- 0.03-S4 placement can consume `WorkOrderPlacement` without changing the core
  work-order schema.
- 0.03 fleet dispatch can replace the local shared-secret key source with a
  manager-provided verifier/keyring while retaining the detached envelope.
- 0.04 governance can add approval policy references without making work orders
  broad credentials.
