# Policy Distribution Reference

Policy distribution is the 0.04-S5 governance path for installing signed policy
bundle authority into a local run without requiring a central-manager round trip
on every tick.

This strengthens the `policy`, `action gateway`, `trace store`, `governance`,
and `SDK/API` primitives. It does not introduce a policy authoring language,
policy UI, fleet consensus system, or approval workflow engine.

## Purpose

A policy bundle tells a Splendor runtime which centrally issued policy version is
currently trusted for a tenant or agent. The bundle is explicit runtime
authority; it is validated before policy invocation, recorded in run/trace
metadata, and consulted by a gateway wrapper before side-effectful actions can
reach adapters.

The implemented 0.04-S5 reference path supports:

- signed policy bundle envelopes;
- tenant and optional agent binding;
- bundle ID and version metadata;
- TTL (`issued_at`/`expires_at`);
- revocation markers;
- degraded cached-policy behavior for disconnected local runtimes;
- trace events for accept, reject, sync failure, expiry, and revocation.

## Schema

Rust contract:

```rust
splendor_types::{
    PolicyBundle,
    PolicyBundleEnvelope,
    PolicyBundleTraceContext,
    PolicyDegradedMode,
    PolicyBundleKeyring,
    validate_policy_bundle,
}
```

Serialized shape:

```yaml
schema_version: splendor.policy_bundle.v1
policy_bundle_id: pol_finance_weekly
version: v2026.05.29
tenant_id: 11111111-1111-1111-1111-111111111111
agent_id: 22222222-2222-2222-2222-222222222222
issued_at: "2026-05-29T12:00:00Z"
expires_at: "2026-05-29T13:00:00Z"
revocation: active
degraded_mode:
  allow_low_risk_cached: true
signature:
  key_id: policy-local-key
  signature: <detached-signature>
```

### Required fields

| Field | Purpose |
| --- | --- |
| `schema_version` | Must be `splendor.policy_bundle.v1`. Unsupported versions fail closed. |
| `policy_bundle_id` | Central-manager-issued policy bundle identity. Distinct from run, action, state, trace, message, work-order, and approval IDs. |
| `version` | Operator/audit version label. |
| `tenant_id` | Tenant authority boundary governed by the bundle. |
| `agent_id` | Optional agent binding. `null` means tenant-wide for the receiving run context. |
| `issued_at` / `expires_at` | TTL window. `expires_at` must be after `issued_at`. |
| `revocation` | `active` or `{ revoked: { reason } }`. Revoked bundles fail closed. |
| `degraded_mode.allow_low_risk_cached` | Whether disconnected runtimes may allow read-only/low-risk actions from an expired cached bundle. |
| `signature` | Detached signature metadata. Missing or empty signature metadata fails closed. |

## Signature envelope

The reference local/resident verifier uses `blake3-keyed-v1`:

1. Serialize the `PolicyBundle` payload deterministically with `serde_json`.
2. Exclude the envelope `signature` field from the signed payload.
3. Derive a 32-byte verifier key by hashing the configured shared secret with
   BLAKE3.
4. Compute a keyed BLAKE3 MAC over the serialized payload.
5. Compare expected and supplied signatures in constant time.

This is a reference path for local tests and early resident integrations. It is
not a PKI or enterprise key-management product. Later fleet work can replace key
sources without changing the policy bundle payload contract.

## Validation lifecycle

`validate_policy_bundle(envelope, context, keyring)` checks, in order:

1. schema shape and non-empty required metadata;
2. detached signature presence;
3. known verification key;
4. signature correctness;
5. TTL expiry;
6. revocation marker;
7. tenant compatibility;
8. optional agent compatibility.

Any failed or unavailable check returns `PolicyBundleValidationError` and denies
installation. Invalid bundles are rejected before policy invocation.

Stable sanitized reason codes include:

```text
unsigned_policy_bundle
unknown_policy_signature_key
bad_policy_signature
expired_policy_bundle
revoked_policy_bundle
malformed_policy_bundle
incompatible_policy_bundle
```

## Runtime lifecycle

### Run creation

`CreateRunRequest` includes:

- `policy_bundle_required: bool`;
- `policy_bundle: Option<PolicyBundleEnvelope>`.

When `policy_bundle_required` is true, the daemon validates the supplied bundle
before creating the run. Validation failure returns a structured API error and no
perceptor, policy callback, gateway, adapter, state commit, or tick execution is
started.

When a valid bundle is supplied, the loop engine records trace-safe metadata in
the run context and agent metadata:

- `policy_bundle_id`;
- `policy_bundle_version`.

Signature material is never stored in trace metadata.

### Policy sync

The run-scoped sync endpoint is:

```text
POST /runs/{run_id}/policies/sync
scope: splendor.policies.sync
```

The endpoint can:

- install a newly validated bundle;
- record a central sync failure without changing cached authority;
- mark the local run cache as disconnected or reconnected;
- apply a revocation marker when a revoked bundle is received.

Sync failures are visible in the run trace and cache status, but they never
broaden authority. Free-form upstream error strings are reduced to sanitized
reason codes before entering trace or cache snapshots.

## Trace events

0.04-S5 adds these `TraceEventKind` variants:

| Variant | Canonical event class | Purpose |
| --- | --- | --- |
| `PolicyBundleAccepted { bundle }` | `policy.bundle.accepted` | A valid bundle was installed or recorded for a run. |
| `PolicyBundleRejected { policy_bundle_id?, version?, reason }` | `policy.bundle.rejected` | A supplied bundle failed validation before runtime authority changed. |
| `PolicySyncFailed { policy_bundle_id?, version?, reason }` | `policy.sync.failed` | Central sync failed and cached authority was left unchanged. |
| `PolicyExpired { policy_bundle_id, version, action? }` | `policy.expired` | Expired policy denied policy invocation or action execution. |
| `PolicyRevoked { policy_bundle_id, version, reason }` | `policy.revoked` | Revocation denied policy invocation or action execution. |

`PolicyBundleAccepted` uses `PolicyBundleTraceContext`, which includes only
bundle identity, version, tenant/agent scope, expiry, and degraded-mode flags.

## Gateway behavior

The kernel wraps the existing action gateway with `PolicyDistributionGateway`.
The wrapper checks policy availability, expiry, revocation, disconnected state,
and degraded-mode allowance before forwarding to the inner gateway. If policy
status denies the action, the adapter is not called and the denial is returned as
a normal `ActionOutcome` with `ActionStatus::Denied`.

This preserves the invariant that no side effect bypasses the action gateway.

## Failure modes

| Condition | Behavior |
| --- | --- |
| Missing required bundle | Daemon run creation returns `missing_policy_bundle`; lower-level runtime caches deny before policy invocation or action forwarding with `policy_unavailable`. |
| Unsupported schema | Reject bundle with `malformed_policy_bundle`. |
| Missing or bad signature | Reject bundle with `unsigned_policy_bundle` or `bad_policy_signature`. |
| Unknown key | Reject bundle with `unknown_policy_signature_key`. |
| Expired bundle at installation | Reject bundle with `expired_policy_bundle`. |
| Expired cached bundle at runtime | Deny policy invocation and side-effectful actions unless disconnected low-risk cached mode applies to read-only actions. |
| Revoked bundle | Reject installation or deny future policy/action authority with `policy_revoked`. |
| Sync failure | Record `PolicySyncFailed`; keep prior cached authority unchanged. |

## Replay behavior

Replay remains inspect-only. It can inspect policy acceptance, rejection, sync
failure, expiry, and revocation trace events and explain why authority changed or
failed closed. It does not call policy distributors, re-verify signatures, invoke
policies, submit actions, or execute adapters.

## Security notes

- Caller credentials authorize daemon endpoint access only.
- Signed work orders authorize run creation/resume.
- Policy bundles authorize policy version/TTL state only.
- The Action Gateway still authorizes side effects.
- A policy sync failure never grants new authority.
- Trace records omit detached signatures, shared secrets, caller tokens, and
  policy-language internals.
- Trace-visible sync and revocation reasons are sanitized reason codes, not raw
  central-manager exception text.

## Compatibility notes

This is a 0.04-dev contract and not the 0.1 stable compatibility line. Field
names are schema-aligned across Rust and TypeScript so later central-manager and
resident-node work can reuse the same payload without changing local runtime
semantics.
