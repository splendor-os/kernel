# Changelog

## 0.03-dev — Resident nodes + fleet execution foundation

### Implemented primitives

- Added the 0.03-S1 distributed identity model with distinct fleet, node,
  instance, tenant, agent, run, tick, action, state-node, trace-event, and
  message IDs.
- Renamed serialized trace event identity to `trace_event_id` while accepting
  legacy `trace_id` during deserialization.
- Added trace identity context fields and fail-closed gateway validation for
  invalid action, tenant, agent, or run identity before adapter execution.
- Added state metadata/commit linkage for tenant, agent, run, and trace-event
  scope.
- Added 0.03-S2 node and instance registry contracts with capabilities,
  heartbeats, deterministic stale detection, and management audit events.
- Added 0.03-S3 signed work-order schemas, detached reference signature
  verification, local `splendorctl` ingestion, scoped runtime policy/quota
  narrowing, and accepted/rejected work-order trace events.
- `splendorctl run` now rejects missing work-order authority by default;
  legacy local quickstarts must opt into `allow_unsigned_local_run: true` and are
  visibly warned.
- Added a 0.03-S4 placement v0 contract in Rust for deterministic target-class,
  capability, data-locality, runtime-version, execution-mode, and
  dedicated-instance matching with explicit rejection reasons and management
  trace/audit evidence.

### Explicitly not included

- No autoscaling, multi-region optimizer, cost optimizer, Kubernetes operator,
  remote dispatch, full PKI/OAuth product, governance approval workflow, remote
  transport, distributed consensus, central manager, or physical/edge
  orchestration.

## 0.02-dev — Local multi-agent runtime + daemon control

### Implemented primitives

- Added a 0.02-S0 daemon security boundary reference contract in Rust for
  caller principals, endpoint scopes, tenant/fleet binding, audience binding,
  credential/work-order expiry and revocation, explicit insecure local dev mode,
  and trace/audit attribution.
- Added a 0.02-S1 message schema contract in Rust for `MessageId`, `Message`,
  `MessageEnvelope`, schema-version validation, delivery status vocabulary,
  message trace links, and message lifecycle trace event definitions.

### Explicitly not included

- No daemon server, HTTP listener, OAuth/OIDC provider, PKI management, fleet
  mTLS rollout, node bootstrap, governance workflow, message router, message
  broker, remote transport, or TypeScript runtime enforcement.

## 0.01-dev — Local kernel baseline

### Implemented primitives

- Local scheduler and loop engine for persistent agent ticks.
- Tenant policy, adapter allowlist, permission, quota, invariant, precondition,
  and postcondition checks before side-effectful adapter execution.
- SQLite-backed state graph with state nodes and snapshots.
- Append-only trace store with per-run sequence numbers, deterministic trace IDs,
  and hash-chain metadata.
- CLI workflows for version, config-driven local run, trace export, state-head
  inspection, and replay.
- Replay from trace/state stores without repeating side effects.
- Python SDK hooks for policies, perceptors, constraints, adapters, trace
  subscription, and inspect-only replay.

### Hardening changes

- Added `splendorctl state head` and `splendorctl --version`.
- Added replay validation for run scope, sequence continuity, trace IDs, and
  trace integrity-chain continuity.
- Added baseline conformance docs, release notes, known limitations, SDK docs,
  and runnable examples.

### Explicitly not included

- No typed local message router, daemon API, or TypeScript client.
- No fleet registry, remote transport, signed work orders, or trace aggregation.
- No governance workflow engine, approvals, circuit breakers, policy TTL, or kill
  switch.
- No physical/edge device orchestration.
- No 0.1 stable compatibility guarantee.
