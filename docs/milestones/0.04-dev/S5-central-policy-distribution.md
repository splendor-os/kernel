# 0.04-S5 — Central Policy Distribution

## Objective

Implement the 0.04-S5 policy distribution primitive so a Splendor runtime can
validate, cache, trace, expire, and revoke centrally issued policy bundles
without calling a central manager on every tick.

## Functional scope

- Adds signed `PolicyBundle` / `PolicyBundleEnvelope` contracts with bundle ID,
  version, tenant/agent scope, TTL, revocation, degraded mode, and detached
  signature metadata.
- Adds a local `PolicyCache` and `PolicyDistributionGateway` wrapper so missing,
  expired, or revoked policy fails closed before policy invocation and before
  adapter execution.
- Adds run-scoped daemon policy sync at `POST /runs/{run_id}/policies/sync` with
  endpoint scope `splendor.policies.sync`.
- Records policy bundle metadata in run inspect responses and trace-safe run
  metadata.
- Adds Rust and TypeScript schema parity for policy bundle and policy sync types.

## Non-goals

- No policy authoring UI.
- No enterprise policy language product.
- No global consensus on policy state.
- No full 0.05 physical/edge offline policy cache.
- No fleet-wide policy distribution service or central revocation API.

## Public contracts changed

- Rust types:
  - `splendor_types::PolicyBundle`
  - `splendor_types::PolicyBundleEnvelope`
  - `splendor_types::PolicyBundleTraceContext`
  - `splendor_types::PolicyDegradedMode`
  - `splendor_types::PolicyBundleKeyring`
  - `splendor_kernel::PolicyCache`
  - `splendor_kernel::PolicyDistributionGateway`
- Daemon API:
  - `CreateRunRequest.policy_bundle_required`
  - `CreateRunRequest.policy_bundle`
  - `RunInspectResponse.policy_bundle`
  - `POST /runs/{run_id}/policies/sync`
  - `PolicySyncRequest`, `PolicyCacheStatusResponse`, `PolicySyncResponse`
- Daemon security:
  - `EndpointScope::PoliciesSync`
  - canonical scope string `splendor.policies.sync`
- Trace events:
  - `PolicyBundleAccepted`
  - `PolicyBundleRejected`
  - `PolicySyncFailed`
  - `PolicyExpired`
  - `PolicyRevoked`
- TypeScript:
  - policy bundle and policy sync schema types in `@splendor/types`.
- Docs/examples:
  - `docs/reference/policy-distribution.md`
  - `docs/reference/offline-policy-cache.md`
  - `examples/policy-cache-degraded-mode/README.md`

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | policy invocation can fail closed before `PolicyInvoked` when required policy is missing, expired, or revoked |
| Gateway | `PolicyDistributionGateway` denies expired/revoked policy before adapters execute |
| Verifier | policy status is represented as fail-closed verification results |
| State graph | no new state nodes; policy cache is explicit run authority, while agent metadata records bundle ID/version |
| Trace store | policy accept/reject/sync/expiry/revocation events added |
| Replay | reconstructs policy authority decisions without contacting central policy services or executing side effects |
| Message | none |
| Work order | no shape change; work orders still authorize runs separately from policy bundles |
| Governance | policy TTL and revocation become enforceable governance gates |

## Trace behavior

New event variants:

- `PolicyBundleAccepted { bundle }`
- `PolicyBundleRejected { policy_bundle_id, version, reason }`
- `PolicySyncFailed { policy_bundle_id, version, reason }`
- `PolicyExpired { policy_bundle_id, version, action }`
- `PolicyRevoked { policy_bundle_id, version, reason }`

`PolicyBundleAccepted` is emitted after a validated bundle is attached to a run.
`PolicySyncFailed` is emitted when sync failure is reported and the existing cache
is preserved. `PolicyExpired` / `PolicyRevoked` explain fail-closed runtime
policy decisions.

## State behavior

No state graph format changes are introduced. Policy bundle metadata is recorded
in run/agent metadata and trace-safe `PolicyBundleTraceContext`. Runtime state
commits remain explicit state graph nodes and are not replaced by the policy
cache.

## Gateway and verifier behavior

- Missing required policy denies with `policy_unavailable`.
- Expired policy denies with `policy_expired` unless disconnected low-risk cached
  mode applies to a read-only action.
- Revoked policy denies with `policy_revoked`.
- Invalid bundle signature/schema/scope fails closed before installation.
- Denied actions do not reach adapters.

## Replay behavior

Replay is inspect-only. It can explain accepted policy bundle metadata, failed
sync attempts, expiry denials, and revocation denials from trace records. It does
not re-verify signatures, contact central policy services, invoke policies, call
gateways, or execute adapters.

## Failure behavior

- Invalid signature/schema rejects run creation or sync.
- Expired bundle rejects installation and blocks runtime authority.
- Revoked bundle rejects installation or marks current cached authority revoked.
- Central sync failure records `PolicySyncFailed` and keeps cached authority
  unchanged. Raw sync/revocation reasons are sanitized before trace/cache output.
- Daemon run creation rejects `policy_bundle_required` requests that omit a
  signed bundle. Kernel-level caches still fail closed on missing required policy
  for direct/runtime use.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | policy bundle signature/schema/TTL/revocation validation | `cargo test -p splendor-types policy_distribution` |
| unit | cache expiry, revocation, low-risk disconnected behavior, inner gateway suppression | `cargo test -p splendor-kernel policy_cache` |
| unit/integration | loop engine records bundle metadata and denies before `PolicyInvoked` | `cargo test -p splendor-kernel loop_engine` |
| integration | daemon bundle metadata, sync failure trace, invalid bundle rejection, revocation side-effect denial | `cargo test -p splendor-daemon policy_bundle` |
| full QA | workspace, Python, and TypeScript validation | run before PR finalization |

## Example or fixture

See `examples/policy-cache-degraded-mode/README.md` for the degraded cached-policy
scenario and smoke commands.

## Future extension notes

Later 0.05 physical/edge work can reuse `PolicyDegradedMode` and
`PolicyCacheSnapshot` when adding device-local policy caches and reconnect sync.
Later fleet work can replace the local shared-secret keyring with central
attestation or key-distribution infrastructure without changing the signed bundle
payload fields.
