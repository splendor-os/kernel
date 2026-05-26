# Changelog

## 0.02-dev — Local multi-agent runtime + daemon control

### Implemented primitives

- Added a 0.02-S0 daemon security boundary reference contract in Rust for
  caller principals, endpoint scopes, tenant/fleet binding, audience binding,
  credential/work-order expiry and revocation, explicit insecure local dev mode,
  and trace/audit attribution.
- Added a 0.02-S1 message schema contract in Rust for `MessageId`, `Message`,
  `MessageEnvelope`, schema-version validation, delivery status vocabulary,
  message trace links, and message lifecycle trace event definitions.
- Added a 0.02-S2 local message router for in-process inbox/outbox delivery with
  trace-linked queued, delivered, rejected, expired, and consumed events.
- Added a 0.02-S3 agent isolation ledger for per-agent permission checks,
  per-agent quota counters, local message schema/recipient grants, denial trace
  artifacts, and replay-visible message decisions.
- Added a 0.02-S4 local delegation model in Rust for typed task
  request/response messages, parent/child run metadata, explicit delegated
  authority, local child-run trace events, structured child failures, parent
  cancellation denial, and inspect-only delegation replay reconstruction.
- Hardened 0.02-S4 local delegation with duplicate child-run ID rejection and
  terminal child completion/failure guards that avoid duplicate response messages
  or terminal traces.
- Added a 0.02-S5 local runtime daemon API crate with endpoints for run
  lifecycle, percept append, ordered traces, state-head lookup, inspect-only
  replay, gateway-mediated action submission, health, and capabilities.
- Added state-node lookup through `StateStore::get_node` so daemon state-head
  responses verify that returned nodes exist in the state graph.
- Added the 0.02-S6 TypeScript surface with `@splendor/types`, a thin
  authenticated `@splendor/client`, schema parity tests, SDK docs, and a minimal
  daemon-client example.
- Added CI coverage for the 0.02-S6 TypeScript runner so package build,
  typecheck, schema/client tests, and Node coverage gates run with Rust and
  Python release checks.
- Added 0.02-S7 inspect-only multi-agent replay output for message lifecycle
  causality, local parent/child run links, and permission-laundering denial
  evidence without re-executing side effects.

### Explicitly not included

- No production OAuth/OIDC provider, PKI management, fleet mTLS rollout, node
  bootstrap, governance workflow, message broker, remote transport, fleet
  scheduler, fleet placement, long-lived child services, native Node binding,
  browser runtime, cross-instance replay, distributed trace sync, or TypeScript
  runtime enforcement.

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
