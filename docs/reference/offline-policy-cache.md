# Offline Policy Cache Reference

The offline policy cache is the 0.04-S5 local cache behavior for centrally
distributed policy bundles. It allows a run to remember the last validated bundle
and fail closed when central policy sync is unavailable, expired, or revoked.

This is not the full 0.05 physical/edge offline policy cache. It only defines the
local policy TTL/degraded-mode semantics needed by central policy distribution.

## Runtime object

Rust contract:

```rust
splendor_kernel::{
    PolicyCache,
    PolicyCacheConfig,
    PolicyCacheSnapshot,
    PolicyDistributionGateway,
    PolicyRuntimeAuthority,
    PolicyDistributionStatus,
}
```

`PolicyCache` stores explicit governance state:

| Field | Purpose |
| --- | --- |
| `enforcement_required` | Whether missing policy authority denies policy invocation/action execution. |
| `disconnected` | Whether the runtime is explicitly operating without central policy connectivity. |
| `bundle` | Current validated bundle metadata and authority. |
| `revoked_reason` | Local revocation marker applied to the current bundle. |
| `last_sync_failure` | Sanitized sync failure reason and timestamp. |

`PolicyCacheSnapshot` exposes trace/API-safe status. It contains
`PolicyBundleTraceContext`, not detached signatures or verification secrets.

## Lifecycle

1. A run is created with `policy_bundle_required` and a signed bundle, or a bundle
   is synced later through `/runs/{run_id}/policies/sync`.
2. The daemon validates the bundle with `validate_policy_bundle` before installing
   it.
3. The loop engine records `PolicyBundleAccepted` and run metadata.
4. Before `PolicyInvoked`, `PolicyRuntimeAuthority` checks availability, TTL, and
   revocation.
5. Before adapters can execute, `PolicyDistributionGateway` checks the same cache
   for action authority.
6. Sync failures update cache status and emit `PolicySyncFailed` without changing
   the current bundle.

## Degraded cached-policy behavior

Expired policy normally fails closed. The only implemented exception is:

```text
bundle expired
AND runtime cache is explicitly disconnected
AND bundle.degraded_mode.allow_low_risk_cached == true
AND action.side_effect_class == read_only
```

Only then may the cached bundle allow the action to continue to the normal
gateway/verifier path. Side-effectful actions such as filesystem writes, network
calls, database mutations, shell commands, webhooks, email, artifact publishing,
or device commands remain denied.

## Sync failure behavior

When central sync fails:

- `PolicyCache::record_sync_failure` stores a sanitized reason;
- daemon `POST /runs/{run_id}/policies/sync` returns `accepted: false`;
- `PolicySyncFailed` is emitted to trace;
- the previous validated bundle remains unchanged;
- no new action, adapter, permission, or tenant scope is added.

This makes connectivity failure observable without widening permissions.
Raw upstream exception text is not persisted in cache snapshots or trace events.

## Revocation behavior

A revoked bundle fails validation before installation. If a revocation is synced
for the currently cached bundle, `PolicyCache::revoke_current` marks the cache as
revoked and future policy invocation/action checks deny with `policy_revoked`.

Revocation is a fail-closed status. A later bundle can restore authority only if
it validates successfully and is not revoked or expired.

## Trace behavior

Offline/degraded cache decisions are visible through:

- `PolicyBundleAccepted` for installed authority;
- `PolicySyncFailed` for failed central sync;
- `PolicyExpired` for TTL denial;
- `PolicyRevoked` for revocation denial;
- normal `ActionDenied` / `OutcomeRecorded` events when the gateway denies an
  action.

## Replay behavior

Replay can reconstruct which cached bundle was present, whether central sync
failed, and whether expiry/revocation denied a policy or action. Replay does not
reconnect to central policy distribution, refresh bundles, invoke policy code, or
execute adapters.

## Failure modes

| Condition | Result |
| --- | --- |
| Required policy missing | Deny with `policy_unavailable`. |
| Bundle expired while connected | Deny policy invocation and actions. |
| Bundle expired while disconnected, low-risk cached disabled | Deny. |
| Bundle expired while disconnected, low-risk cached enabled, read-only action | Continue to normal gateway verification. |
| Bundle expired while disconnected, side-effectful action | Deny with `policy_expired`. |
| Bundle revoked | Deny with `policy_revoked`. |
| Central sync failed | Record failure and keep previous authority. |

## Security notes

The offline cache is not an ambient permission store. It can only preserve the
last validated bundle and only within that bundle's degraded-mode rules. It never
grants broader actions, adapters, permissions, data references, tenant scope, or
agent scope than the existing run/gateway/verifier chain already allows.
