# Policy Cache Degraded Mode Example

This example documents the 0.04-S5 degraded cached-policy behavior. It is a
reference scenario rather than a standalone binary: the behavior is proven by the
policy distribution, policy cache, loop engine, and daemon tests listed below.

## What this proves

- A signed policy bundle is validated before run authority changes.
- The runtime records policy bundle ID/version in run and trace metadata.
- Central sync failure is trace-visible and does not replace cached authority.
- An expired cached policy denies side-effectful actions.
- A disconnected runtime may continue read-only/low-risk cached operation only
  when the bundle explicitly sets `degraded_mode.allow_low_risk_cached = true`.
- Replay can inspect these events without re-running policies or adapters.

## Scenario

1. A central manager issues a signed bundle:

   ```yaml
   schema_version: splendor.policy_bundle.v1
   policy_bundle_id: pol_cache_demo
   version: v1
   tenant_id: <tenant-id>
   agent_id: <agent-id>
   issued_at: "2026-05-29T12:00:00Z"
   expires_at: "2026-05-29T13:00:00Z"
   revocation: active
   degraded_mode:
     allow_low_risk_cached: true
   signature:
     key_id: policy-local-key
     signature: <detached-signature>
   ```

2. The daemon creates a run with:

   ```json
   {
     "policy_bundle_required": true,
     "policy_bundle": { "...": "signed envelope" }
   }
   ```

3. The runtime records `PolicyBundleAccepted` and run inspect returns
   `policy_bundle.policy_bundle_id` and `policy_bundle.version`.

4. Later, central policy sync fails and the daemon receives:

   ```json
   {
     "policy_bundle": null,
     "sync_error": "central_unavailable",
     "disconnected": true
   }
   ```

   The runtime records `PolicySyncFailed` and keeps the prior validated bundle.

5. If the cached bundle expires while disconnected:

   - `read_only` actions may continue to normal gateway verification only when
     `allow_low_risk_cached` is true;
   - `filesystem`, `network`, `external`, and other side-effectful actions are
     denied with `policy_expired` before adapters execute.

## Smoke commands

Run the targeted tests:

```bash
cargo test -p splendor-types policy_distribution
cargo test -p splendor-kernel policy_cache
cargo test -p splendor-kernel loop_engine
cargo test -p splendor-daemon policy_bundle
```

Run full Rust validation:

```bash
cargo test --workspace
```

## Trace expectations

A successful degraded-cache scenario should include:

```text
PolicyBundleAccepted
PolicySyncFailed
ActionDenied { result.reasons contains "policy_expired" }  # for side effects
OutcomeRecorded
```

Replay should report these events from trace records and must not call the policy
distributor, policy host, action gateway, verifier chain, or adapter.

## Non-goals

- No physical device policy cache or trace reconnect sync.
- No central policy authoring UI.
- No production key-management or PKI implementation.
- No global consensus or fleet-wide policy state.
