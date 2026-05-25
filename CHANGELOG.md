# Changelog

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
